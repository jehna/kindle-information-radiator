use crate::{ical::Event, weather::WeatherData};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use chrono_tz::{Europe::Helsinki, Tz};

const APP_TZ: Tz = Helsinki;
use std::fmt::Write;
use std::sync::Arc;

const CLOUD_PATH: &str = "<path d='M -10,3 a 4,4 0 0,1 4,-4 a 6,6 0 0,1 11,2 a 3,3 0 0,1 0,6 l -16,0 a 3,3 0 0,1 1,-4 z' fill='white' stroke='black' stroke-width='1.5'/>";

const FI_WEEKDAYS: [&str; 7] = ["su", "ma", "ti", "ke", "to", "pe", "la"];

fn icon_svg(code: u32, cx: f64, cy: f64) -> String {
    let head = format!("<g transform='translate({},{})'>", cx, cy);
    let tail = "</g>";
    if code == 0 {
        format!(
            "{head}<circle r='4.5' fill='black'/>\
             <g stroke='black' stroke-width='1.5' stroke-linecap='round'>\
             <line x1='0' y1='-7' x2='0' y2='-10'/>\
             <line x1='0' y1='7' x2='0' y2='10'/>\
             <line x1='-7' y1='0' x2='-10' y2='0'/>\
             <line x1='7' y1='0' x2='10' y2='0'/>\
             <line x1='-5' y1='-5' x2='-7.5' y2='-7.5'/>\
             <line x1='5' y1='-5' x2='7.5' y2='-7.5'/>\
             <line x1='-5' y1='5' x2='-7.5' y2='7.5'/>\
             <line x1='5' y1='5' x2='7.5' y2='7.5'/>\
             </g>{tail}"
        )
    } else if code <= 3 {
        format!("{head}<circle cx='-4' cy='-4' r='4' fill='black'/>{CLOUD_PATH}{tail}")
    } else if code <= 48 {
        format!("{head}{CLOUD_PATH}{tail}")
    } else if code <= 67 || (code >= 80 && code <= 82) {
        format!(
            "{head}{CLOUD_PATH}\
             <g stroke='black' stroke-width='1.5' stroke-linecap='round'>\
             <line x1='-5' y1='6' x2='-7' y2='10'/>\
             <line x1='0' y1='6' x2='-2' y2='10'/>\
             <line x1='5' y1='6' x2='3' y2='10'/>\
             </g>{tail}"
        )
    } else if code <= 77 {
        format!(
            "{head}<g stroke='black' stroke-width='1.5' stroke-linecap='round'>\
             <line x1='0' y1='-9' x2='0' y2='9'/>\
             <line x1='-9' y1='0' x2='9' y2='0'/>\
             <line x1='-6.4' y1='-6.4' x2='6.4' y2='6.4'/>\
             <line x1='-6.4' y1='6.4' x2='6.4' y2='-6.4'/>\
             </g>{tail}"
        )
    } else {
        format!("{head}{CLOUD_PATH}<path d='M -2,4 L -4,8 L 0,8 L -2,12 L 4,6 L 1,6 L 3,2 z' fill='black'/>{tail}")
    }
}

fn smooth_path(coords: &[(f64, f64)]) -> String {
    if coords.len() < 2 {
        return String::new();
    }
    let mut s = format!("M {} {}", coords[0].0, coords[0].1);
    for i in 0..coords.len() - 1 {
        let p0 = coords[i.saturating_sub(1)];
        let p1 = coords[i];
        let p2 = coords[i + 1];
        let p3 = coords[(i + 2).min(coords.len() - 1)];
        let c1x = p1.0 + (p2.0 - p0.0) / 6.0;
        let c1y = p1.1 + (p2.1 - p0.1) / 6.0;
        let c2x = p2.0 - (p3.0 - p1.0) / 6.0;
        let c2y = p2.1 - (p3.1 - p1.1) / 6.0;
        let _ = write!(
            s,
            " C {} {} {} {} {} {}",
            c1x, c1y, c2x, c2y, p2.0, p2.1
        );
    }
    s
}

fn weather_widget(
    location: &str,
    data: &WeatherData,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> String {
    let times = &data.hourly.time;
    let temps = &data.hourly.temperature_2m;
    let codes = &data.hourly.weather_code;
    let n = times.len().min(12);
    if n < 2 {
        return String::new();
    }

    let raw_lo = temps[..n].iter().cloned().fold(f64::INFINITY, f64::min);
    let raw_hi = temps[..n].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let lo = (raw_lo / 5.0).floor() * 5.0;
    let mut hi = (raw_hi / 5.0).ceil() * 5.0;
    if hi - lo < 5.0 {
        hi = lo + 5.0;
    }
    let span = hi - lo;

    let header = format!(
        "<text x='{}' y='{}' font-size='28' font-weight='bold' font-family='sans-serif'>{}</text>\
         <text x='{}' y='{}' font-size='52' font-weight='bold' font-family='sans-serif'>{}°C</text>",
        x + 16.0, y + 36.0, location,
        x + 16.0, y + 90.0, data.current.temperature_2m.round() as i64
    );

    let margin_l = x + 50.0;
    let margin_r = 16.0;
    let margin_t = y + 130.0;
    let margin_b = 28.0;
    let plot_w = (x + w) - margin_l - margin_r;
    let plot_h = (y + h) - margin_t - margin_b;
    let step = plot_w / (n as f64 - 1.0);

    let coords: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let px = margin_l + step * i as f64;
            let py = margin_t + plot_h - (temps[i] - lo) / span * plot_h;
            (px, py)
        })
        .collect();

    let mut grid = String::new();
    let mut temp = lo as i64;
    while temp as f64 <= hi {
        let py = margin_t + plot_h - (temp as f64 - lo) / span * plot_h;
        let major = temp.rem_euclid(5) == 0;
        let color = if major { "#666" } else { "#bbb" };
        let sw = if major { "0.8" } else { "0.3" };
        let _ = write!(
            grid,
            "<line x1='{}' y1='{}' x2='{}' y2='{}' stroke='{}' stroke-width='{}'/>",
            margin_l, py, x + w - margin_r, py, color, sw
        );
        if major {
            let _ = write!(
                grid,
                "<text x='{}' y='{}' font-size='13' text-anchor='end' fill='#555' font-family='sans-serif'>{}°</text>",
                margin_l - 6.0, py + 5.0, temp
            );
        }
        temp += 1;
    }
    for i in 0..n {
        let px = margin_l + step * i as f64;
        let major = i % 3 == 0;
        let color = if major { "#888" } else { "#ccc" };
        let sw = if major { "0.6" } else { "0.4" };
        let _ = write!(
            grid,
            "<line x1='{}' y1='{}' x2='{}' y2='{}' stroke='{}' stroke-width='{}'/>",
            px, margin_t, px, margin_t + plot_h, color, sw
        );
    }

    let mut icons = String::new();
    for i in 0..n {
        let px = margin_l + step * i as f64;
        icons.push_str(&icon_svg(codes[i], px, margin_t - 22.0));
    }

    let mut labels = String::new();
    for i in 0..n {
        let px = margin_l + step * i as f64;
        let py = margin_t + plot_h + 18.0;
        let label = times[i]
            .split('T')
            .nth(1)
            .and_then(|t| t.split(':').next())
            .unwrap_or("");
        let _ = write!(
            labels,
            "<text x='{}' y='{}' font-size='12' text-anchor='middle' fill='#333' font-family='sans-serif'>{}</text>",
            px, py, label
        );
    }

    let path_d = smooth_path(&coords);

    format!(
        "{header}{grid}\
         <path d='{path_d}' fill='none' stroke='black' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'/>\
         {icons}{labels}"
    )
}

fn day_bucket(ts: i64) -> i64 {
    let dt = APP_TZ.timestamp_opt(ts, 0).single().unwrap();
    let d = dt.date_naive();
    APP_TZ
        .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
        .single()
        .unwrap()
        .timestamp()
}

fn format_event_time(ts: i64, all_day: bool) -> String {
    let now = Utc::now().timestamp();
    let diff_days = (day_bucket(ts) - day_bucket(now)) / 86400;
    let dt: DateTime<Tz> = APP_TZ.timestamp_opt(ts, 0).single().unwrap();
    let prefix = if diff_days == 0 {
        "Tänään".to_string()
    } else if diff_days == 1 {
        "Huomenna".to_string()
    } else {
        let wday_idx = dt.weekday().num_days_from_sunday() as usize;
        format!("{} {}.{}.", FI_WEEKDAYS[wday_idx], dt.day(), dt.month())
    };
    if all_day {
        prefix
    } else {
        format!("{} {}", prefix, dt.format("%H:%M"))
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn calendar_widget(events: &[Event], x: f64, y: f64, w: f64, h: f64) -> String {
    let _ = w;
    let title = format!(
        "<text x='{}' y='{}' font-size='32' font-weight='bold' font-family='sans-serif'>Tulevat</text>",
        x + 24.0, y + 44.0
    );
    if events.is_empty() {
        return format!(
            "{title}<text x='{}' y='{}' font-size='22' fill='#444' font-family='sans-serif'>Ei tulevia tapahtumia</text>",
            x + 24.0, y + 90.0
        );
    }
    let count = events.len() as f64;
    let row_h = (h - 70.0) / count;
    let mut body = String::new();
    for (i, ev) in events.iter().enumerate() {
        let row_y = y + 70.0 + i as f64 * row_h;
        let when = format_event_time(ev.start_ts, ev.all_day);
        let _ = write!(
            body,
            "<text x='{}' y='{}' font-size='22' fill='#333' font-family='sans-serif'>{}</text>",
            x + 24.0, row_y + 30.0, xml_escape(&when)
        );
        let _ = write!(
            body,
            "<text x='{}' y='{}' font-size='38' font-weight='bold' font-family='sans-serif'>{}</text>",
            x + 24.0, row_y + 78.0, xml_escape(&ev.summary)
        );
    }
    format!("{title}{body}")
}

pub fn build_svg(
    location: &str,
    weather: Option<&WeatherData>,
    events: &[Event],
    w: u32,
    h: u32,
) -> String {
    let weather_h = (h as f64 * 0.62).floor();
    let cal_y = weather_h + 1.0;
    let cal_h = h as f64 - cal_y;

    let mut svg = format!(
        "<?xml version='1.0' encoding='UTF-8'?>\
         <svg xmlns='http://www.w3.org/2000/svg' width='{w}' height='{h}' viewBox='0 0 {w} {h}'>\
         <rect width='{w}' height='{h}' fill='white'/>"
    );
    if let Some(weather) = weather {
        svg.push_str(&weather_widget(location, weather, 0.0, 0.0, w as f64, weather_h));
    }
    let _ = write!(
        svg,
        "<line x1='30' y1='{}' x2='{}' y2='{}' stroke='#333' stroke-width='2'/>",
        weather_h,
        w as f64 - 30.0,
        weather_h
    );
    svg.push_str(&calendar_widget(events, 0.0, cal_y, w as f64, cal_h));
    svg.push_str("</svg>");
    svg
}

pub fn render_to_pixmap(
    svg: &str,
    w: u32,
    h: u32,
) -> Result<resvg::tiny_skia::Pixmap, String> {
    let mut fontdb = resvg::usvg::fontdb::Database::new();
    fontdb.load_system_fonts();
    for path in &[
        "/usr/java/lib/fonts",
        "/usr/share/fonts",
        "/system/usr/share/fonts",
        "/var/local/font",
        "/opt/amazon/ebook/fonts",
        "/mnt/us/fonts",
    ] {
        fontdb.load_fonts_dir(path);
    }

    let first_family = fontdb
        .faces()
        .next()
        .and_then(|f| f.families.first().map(|(name, _)| name.clone()));
    if let Some(family) = &first_family {
        fontdb.set_sans_serif_family(family);
        fontdb.set_serif_family(family);
        eprintln!("using font family: {} ({} faces total)", family, fontdb.len());
    } else {
        eprintln!("WARNING: no fonts loaded; text will not render");
    }
    let opt = resvg::usvg::Options {
        fontdb: Arc::new(fontdb),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or("pixmap")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

pub fn pixmap_to_png(pixmap: &resvg::tiny_skia::Pixmap) -> Result<Vec<u8>, String> {
    pixmap.encode_png().map_err(|e| e.to_string())
}

pub fn pixmap_to_grayscale(pixmap: &resvg::tiny_skia::Pixmap) -> Vec<u8> {
    pixmap
        .data()
        .chunks_exact(4)
        .map(|c| {
            let r = c[0] as u32;
            let g = c[1] as u32;
            let b = c[2] as u32;
            ((r * 299 + g * 587 + b * 114) / 1000) as u8
        })
        .collect()
}
