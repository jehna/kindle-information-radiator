use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::{Europe::Helsinki, Tz};
use rrule::RRuleSet;

const APP_TZ: Tz = Helsinki;

#[derive(Debug, Clone)]
pub struct Event {
    pub summary: String,
    pub start_ts: i64,
    pub all_day: bool,
}

pub fn fetch_and_process(url: &str, count: usize) -> Result<Vec<Event>, String> {
    let resp = minreq::get(url)
        .with_timeout(20)
        .send()
        .map_err(|e| format!("http: {}", e))?;
    if resp.status_code != 200 {
        return Err(format!("HTTP {}", resp.status_code));
    }
    let body = resp.as_str().map_err(|e| format!("body: {}", e))?;
    let events = parse_ics(body);

    let now = Utc::now().timestamp();
    let total = events.len();
    let mut upcoming: Vec<Event> = events
        .into_iter()
        .filter_map(|e| next_event(e, now))
        .filter(|e| e.start_ts >= now)
        .collect();
    upcoming.sort_by_key(|e| e.start_ts);
    eprintln!(
        "ical: {} events parsed, {} upcoming, taking first {}",
        total,
        upcoming.len(),
        count
    );
    upcoming.truncate(count);
    for ev in &upcoming {
        let dt: DateTime<Tz> = APP_TZ.timestamp_opt(ev.start_ts, 0).single().unwrap();
        eprintln!("  {} {}", dt.format("%Y-%m-%d %H:%M"), ev.summary);
    }
    Ok(upcoming)
}

fn unfold(text: &str) -> String {
    text.replace("\r\n ", "")
        .replace("\r\n\t", "")
        .replace("\n ", "")
        .replace("\n\t", "")
}

#[derive(Default)]
struct Builder {
    summary: Option<String>,
    dtstart_line: Option<String>,
    rrule_line: Option<String>,
    exdate_lines: Vec<String>,
    parsed_start_ts: Option<i64>,
    all_day: bool,
}

fn parse_ics(text: &str) -> Vec<EventRaw> {
    let unfolded = unfold(text);
    let mut events = Vec::new();
    let mut current: Option<Builder> = None;
    for line in unfolded.lines() {
        if line == "BEGIN:VEVENT" {
            current = Some(Builder::default());
        } else if line == "END:VEVENT" {
            if let Some(b) = current.take() {
                if let (Some(summary), Some(start_ts), Some(dtstart_line)) =
                    (b.summary, b.parsed_start_ts, b.dtstart_line)
                {
                    let rrule_input = b.rrule_line.map(|rrule| {
                        let mut s = dtstart_line;
                        s.push('\n');
                        s.push_str(&rrule);
                        for ex in &b.exdate_lines {
                            s.push('\n');
                            s.push_str(ex);
                        }
                        s
                    });
                    events.push(EventRaw {
                        summary,
                        start_ts,
                        all_day: b.all_day,
                        rrule_input,
                    });
                }
            }
        } else if let Some(b) = current.as_mut() {
            if let Some((prop, value)) = line.split_once(':') {
                let (key, params) = prop.split_once(';').unwrap_or((prop, ""));
                match key {
                    "SUMMARY" => {
                        b.summary = Some(unescape_text(value));
                    }
                    "DTSTART" => {
                        b.dtstart_line = Some(line.to_string());
                        if let Some((ts, all_day)) = parse_dt(value, params) {
                            b.parsed_start_ts = Some(ts);
                            b.all_day = all_day;
                        }
                    }
                    "RRULE" => {
                        b.rrule_line = Some(line.to_string());
                    }
                    "EXDATE" => {
                        b.exdate_lines.push(line.to_string());
                    }
                    _ => {}
                }
            }
        }
    }
    events
}

fn unescape_text(s: &str) -> String {
    s.replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\n", " ")
        .replace("\\N", " ")
}

fn parse_dt(value: &str, params: &str) -> Option<(i64, bool)> {
    let utc = value.ends_with('Z');
    let trimmed = value.trim_end_matches('Z');
    if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y%m%dT%H%M%S") {
        let ts = if utc {
            Utc.from_utc_datetime(&dt).timestamp()
        } else {
            APP_TZ.from_local_datetime(&dt).single()?.timestamp()
        };
        return Some((ts, false));
    }
    if let Ok(d) = NaiveDate::parse_from_str(trimmed, "%Y%m%d") {
        let dt = d.and_time(NaiveTime::from_hms_opt(0, 0, 0)?);
        let ts = APP_TZ.from_local_datetime(&dt).single()?.timestamp();
        let all_day = params.contains("VALUE=DATE") && !params.contains("DATE-TIME");
        return Some((ts, all_day));
    }
    None
}

#[derive(Debug, Clone)]
struct EventRaw {
    summary: String,
    start_ts: i64,
    all_day: bool,
    rrule_input: Option<String>,
}

fn next_event(raw: EventRaw, now: i64) -> Option<Event> {
    let start_ts = if let Some(input) = &raw.rrule_input {
        let rrule_set: RRuleSet = match input.parse() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("rrule parse failed for '{}': {}", raw.summary, e);
                return None;
            }
        };
        let result = rrule_set.all(5000);
        result
            .dates
            .into_iter()
            .find(|d| d.timestamp() >= now)
            .map(|d| d.timestamp())?
    } else {
        raw.start_ts
    };
    Some(Event {
        summary: raw.summary,
        start_ts,
        all_day: raw.all_day,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        APP_TZ
            .with_ymd_and_hms(y, mo, d, h, mi, 0)
            .single()
            .unwrap()
            .timestamp()
    }

    #[test]
    fn parses_dtstart_summary_and_rrule_input() {
        let body = "BEGIN:VEVENT\r\n\
                    DTSTART;TZID=Europe/Helsinki:20250630T174000\r\n\
                    DTEND;TZID=Europe/Helsinki:20250630T183000\r\n\
                    RRULE:FREQ=DAILY;COUNT=5\r\n\
                    DTSTAMP:20260430T092227Z\r\n\
                    SUMMARY:Anonymized swim class\r\n\
                    END:VEVENT\r\n";
        let raw = parse_ics(body);
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].summary, "Anonymized swim class");
        assert_eq!(raw[0].start_ts, ts(2025, 6, 30, 17, 40));
        assert!(raw[0]
            .rrule_input
            .as_deref()
            .unwrap()
            .contains("RRULE:FREQ=DAILY;COUNT=5"));
    }

    #[test]
    fn finite_count_recurrence_returns_none_after_series_ends() {
        let body = "BEGIN:VEVENT\r\n\
                    DTSTART;TZID=Europe/Helsinki:20250630T174000\r\n\
                    RRULE:FREQ=DAILY;COUNT=5\r\n\
                    SUMMARY:swim\r\n\
                    END:VEVENT\r\n";
        let raw = parse_ics(body);
        let now = ts(2026, 4, 30, 12, 0);
        assert!(next_event(raw[0].clone(), now).is_none());
    }

    #[test]
    fn finite_count_recurrence_returns_correct_occurrence_during_series() {
        let body = "BEGIN:VEVENT\r\n\
                    DTSTART;TZID=Europe/Helsinki:20250630T174000\r\n\
                    RRULE:FREQ=DAILY;COUNT=5\r\n\
                    SUMMARY:x\r\n\
                    END:VEVENT\r\n";
        let raw = parse_ics(body);
        let now = ts(2025, 7, 2, 9, 0);
        let ev = next_event(raw[0].clone(), now).unwrap();
        assert_eq!(ev.start_ts, ts(2025, 7, 2, 17, 40));
    }

    #[test]
    fn weekly_unbounded_advances_to_future_keeping_weekday() {
        use chrono::Datelike;
        let body = "BEGIN:VEVENT\r\n\
                    DTSTART;TZID=Europe/Helsinki:20250101T100000\r\n\
                    RRULE:FREQ=WEEKLY\r\n\
                    SUMMARY:x\r\n\
                    END:VEVENT\r\n";
        let raw = parse_ics(body);
        let now = ts(2026, 4, 30, 12, 0);
        let ev = next_event(raw[0].clone(), now).unwrap();
        let dt = APP_TZ.timestamp_opt(ev.start_ts, 0).single().unwrap();
        assert_eq!(dt.weekday(), chrono::Weekday::Wed);
        assert!(ev.start_ts >= now);
    }

    #[test]
    fn until_in_past_drops_event() {
        let body = "BEGIN:VEVENT\r\n\
                    DTSTART;TZID=Europe/Helsinki:20240101T100000\r\n\
                    RRULE:FREQ=WEEKLY;UNTIL=20240601T000000Z\r\n\
                    SUMMARY:x\r\n\
                    END:VEVENT\r\n";
        let raw = parse_ics(body);
        let now = ts(2026, 4, 30, 12, 0);
        assert!(next_event(raw[0].clone(), now).is_none());
    }

    #[test]
    fn exdate_skips_excluded_occurrence() {
        let body = "BEGIN:VEVENT\r\n\
                    DTSTART;TZID=Europe/Helsinki:20260423T170000\r\n\
                    RRULE:FREQ=WEEKLY\r\n\
                    EXDATE;TZID=Europe/Helsinki:20260430T170000\r\n\
                    SUMMARY:x\r\n\
                    END:VEVENT\r\n";
        let raw = parse_ics(body);
        let now = ts(2026, 4, 24, 12, 0);
        let ev = next_event(raw[0].clone(), now).unwrap();
        assert_eq!(ev.start_ts, ts(2026, 5, 7, 17, 0));
    }

    #[test]
    fn exdate_with_multiple_dates_on_one_line() {
        let body = "BEGIN:VEVENT\r\n\
                    DTSTART;TZID=Europe/Helsinki:20260423T170000\r\n\
                    RRULE:FREQ=WEEKLY\r\n\
                    EXDATE;TZID=Europe/Helsinki:20260430T170000,20260507T170000\r\n\
                    SUMMARY:x\r\n\
                    END:VEVENT\r\n";
        let raw = parse_ics(body);
        let now = ts(2026, 4, 24, 12, 0);
        let ev = next_event(raw[0].clone(), now).unwrap();
        assert_eq!(ev.start_ts, ts(2026, 5, 14, 17, 0));
    }

    #[test]
    fn non_recurring_past_event_kept_for_filter_to_drop() {
        let body = "BEGIN:VEVENT\r\n\
                    DTSTART;TZID=Europe/Helsinki:20250101T100000\r\n\
                    SUMMARY:x\r\n\
                    END:VEVENT\r\n";
        let raw = parse_ics(body);
        let now = ts(2026, 4, 30, 12, 0);
        let ev = next_event(raw[0].clone(), now).unwrap();
        assert_eq!(ev.start_ts, ts(2025, 1, 1, 10, 0));
        assert!(ev.start_ts < now);
    }

    #[test]
    fn byday_weekly_with_specific_days() {
        // Original DTSTART is Monday but BYDAY says only Tuesday and Thursday.
        // The rrule crate handles BYDAY correctly — our previous hand-rolled code did not.
        let body = "BEGIN:VEVENT\r\n\
                    DTSTART;TZID=Europe/Helsinki:20260420T170000\r\n\
                    RRULE:FREQ=WEEKLY;BYDAY=TU,TH\r\n\
                    SUMMARY:x\r\n\
                    END:VEVENT\r\n";
        let raw = parse_ics(body);
        // After Monday 2026-04-20, next occurrence should be Tuesday 2026-04-21
        let now = ts(2026, 4, 20, 18, 0);
        let ev = next_event(raw[0].clone(), now).unwrap();
        assert_eq!(ev.start_ts, ts(2026, 4, 21, 17, 0));
    }
}
