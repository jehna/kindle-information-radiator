use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};
use chrono_tz::Europe::Helsinki;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Routepoint {
    #[serde(rename = "RoutepointsTime")]
    time: String,
}

#[derive(Deserialize)]
struct Response {
    items: Vec<Routepoint>,
}

#[derive(Deserialize)]
struct ScheduleResp {
    items: Vec<WeekSchedule>,
}

#[derive(Deserialize)]
struct WeekSchedule {
    #[serde(rename = "MaPvm", default)] ma_pvm: String,
    #[serde(rename = "MaApTyyppi", default)] ma_ap: String,
    #[serde(rename = "TiPvm", default)] ti_pvm: String,
    #[serde(rename = "TiApTyyppi", default)] ti_ap: String,
    #[serde(rename = "KePvm", default)] ke_pvm: String,
    #[serde(rename = "KeApTyyppi", default)] ke_ap: String,
    #[serde(rename = "ToPvm", default)] to_pvm: String,
    #[serde(rename = "ToApTyyppi", default)] to_ap: String,
    #[serde(rename = "PePvm", default)] pe_pvm: String,
    #[serde(rename = "PeApTyyppi", default)] pe_ap: String,
}

fn parse_pvm(s: &str) -> Option<NaiveDate> {
    // e.g. "2026-04-27T00:00:00+03:00"
    s.split('T').next().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
}

fn fetch_morning_types(
    api_token: &str,
    customer_rivi_id: u64,
) -> Result<HashMap<NaiveDate, String>, String> {
    let url = format!(
        "https://intra.kuntalogistiikka.fi/api/MunAppAPI/v1/model/MunAppSchedule/{}",
        customer_rivi_id
    );
    let resp = minreq::get(&url)
        .with_header("apiAccessToken", api_token)
        .with_header("Origin", "https://munapp.kuntalogistiikka.fi")
        .with_header("Referer", "https://munapp.kuntalogistiikka.fi/")
        .with_header("Accept", "*/*")
        .with_timeout(15)
        .send()
        .map_err(|e| format!("http: {}", e))?;
    if resp.status_code != 200 {
        return Err(format!("HTTP {}", resp.status_code));
    }
    let body = resp.as_str().map_err(|e| format!("body: {}", e))?;
    let parsed: ScheduleResp = serde_json::from_str(body).map_err(|e| format!("json: {}", e))?;

    let mut map = HashMap::new();
    for week in parsed.items {
        for (pvm, ap) in [
            (week.ma_pvm, week.ma_ap),
            (week.ti_pvm, week.ti_ap),
            (week.ke_pvm, week.ke_ap),
            (week.to_pvm, week.to_ap),
            (week.pe_pvm, week.pe_ap),
        ] {
            if let Some(d) = parse_pvm(&pvm) {
                map.insert(d, ap);
            }
        }
    }
    Ok(map)
}

pub struct TaxiPickup {
    /// Scheduled pickup time as a Helsinki-local unix timestamp.
    /// Real pickup tends to be ~5 minutes earlier than this.
    pub scheduled_ts: i64,
}

/// `weekday_ids` is `[Mon, Tue, Wed, Thu, Fri]`.
pub fn fetch(
    api_token: &str,
    customer_rivi_id: u64,
    weekday_ids: &[u64; 5],
) -> Result<Option<TaxiPickup>, String> {
    let now = chrono::Utc::now().with_timezone(&Helsinki);
    let today = now.date_naive();

    let morning_types = match fetch_morning_types(api_token, customer_rivi_id) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("taxi schedule fetch failed: {}", e);
            HashMap::new()
        }
    };

    let mut filters: Vec<String> = Vec::new();
    for offset in 0..14 {
        let date = today + Duration::days(offset);
        let idx = match date.weekday() {
            Weekday::Mon => 0,
            Weekday::Tue => 1,
            Weekday::Wed => 2,
            Weekday::Thu => 3,
            Weekday::Fri => 4,
            _ => continue,
        };
        if let Some(t) = morning_types.get(&date) {
            if !t.is_empty() && t != "Lukujärjestys" {
                println!("taxi: skip {} ({})", date, t);
                continue;
            }
        }
        filters.push(format!(
            "kouvola_{}%20{}",
            date.format("%Y-%m-%d"),
            weekday_ids[idx]
        ));
        if filters.len() >= 10 {
            break;
        }
    }

    let url = format!(
        "https://intra.kuntalogistiikka.fi/api/MunAppAPI/v1/model/VehicleReservationsRoutepoints\
         ?ReservationsExportSystemIdIn={}&RoutepointsCustomerRiviIdIn={}",
        filters.join(","),
        customer_rivi_id
    );

    let resp = minreq::get(&url)
        .with_header("apiAccessToken", api_token)
        .with_header("Origin", "https://munapp.kuntalogistiikka.fi")
        .with_header("Referer", "https://munapp.kuntalogistiikka.fi/")
        .with_header("Accept", "*/*")
        .with_timeout(15)
        .send()
        .map_err(|e| format!("http: {}", e))?;

    if resp.status_code != 200 {
        return Err(format!("HTTP {}", resp.status_code));
    }

    let body = resp.as_str().map_err(|e| format!("body: {}", e))?;
    let parsed: Response = serde_json::from_str(body).map_err(|e| format!("json: {}", e))?;

    let now_ts = now.timestamp();
    let mut next_ts: Option<i64> = None;
    for rp in &parsed.items {
        let Ok(naive) = NaiveDateTime::parse_from_str(&rp.time, "%Y-%m-%dT%H:%M:%S") else {
            continue;
        };
        let chrono::LocalResult::Single(dt) = naive.and_local_timezone(Helsinki) else {
            continue;
        };
        let ts = dt.timestamp();
        if ts > now_ts && next_ts.map_or(true, |n| ts < n) {
            next_ts = Some(ts);
        }
    }

    Ok(next_ts.map(|ts| TaxiPickup { scheduled_ts: ts }))
}
