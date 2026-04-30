#[cfg(target_os = "linux")]
mod fb;
mod ical;
#[cfg(target_os = "linux")]
mod input;
mod render;
mod taxi;
mod weather;

use serde::Deserialize;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::ical::Event;
use crate::taxi::TaxiPickup;
use crate::weather::WeatherData;

const OUTPUT_PATH: &str = "/tmp/weather.png";
const REFETCH_INTERVAL: Duration = Duration::from_secs(2 * 60 * 60);

#[derive(Deserialize)]
struct Config {
    location_name: String,
    latitude: f64,
    longitude: f64,
    calendar_url: String,
    taxi: Option<TaxiConfig>,
}

#[derive(Deserialize)]
struct TaxiConfig {
    api_token: String,
    customer_rivi_id: u64,
    /// `[Mon, Tue, Wed, Thu, Fri]`
    weekday_ids: [u64; 5],
}

fn load_config() -> Config {
    let path = std::env::var("WEATHER_CONFIG").ok().unwrap_or_else(|| {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));
        if let Some(dir) = exe_dir {
            let next_to_exe = dir.join("config.toml");
            if next_to_exe.exists() {
                return next_to_exe.to_string_lossy().into_owned();
            }
            let in_extension = dir.join("../config.toml");
            if in_extension.exists() {
                return in_extension.to_string_lossy().into_owned();
            }
        }
        "config.toml".to_string()
    });
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read config {}: {}", path, e));
    toml::from_str(&raw).unwrap_or_else(|e| panic!("parse config {}: {}", path, e))
}

// Kindle Paperwhite 2 (Pinot/Wario): 758 x 1024 @ 212 dpi
const CANVAS_W: u32 = 758;
const CANVAS_H: u32 = 1024;

struct FetchedData {
    weather: Option<WeatherData>,
    events: Vec<Event>,
    pickup: Option<TaxiPickup>,
}

fn read_battery_level() -> Option<u32> {
    let p = "/usr/bin/lipc-get-prop";
    if !std::path::Path::new(p).exists() {
        return None;
    }
    let out = Command::new(p)
        .args(["com.lab126.powerd", "battLevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    std::str::from_utf8(&out.stdout).ok()?.trim().parse().ok()
}

fn fetch_data(config: &Config) -> FetchedData {
    set_wifi(true);
    wait_for_network();
    let result = fetch_data_inner(config);
    set_wifi(false);
    result
}

fn fetch_data_inner(config: &Config) -> FetchedData {
    let weather = match weather::fetch(config.latitude, config.longitude) {
        Ok(w) => Some(w),
        Err(e) => {
            eprintln!("weather fetch failed: {}", e);
            None
        }
    };
    let events = ical::fetch_and_process(&config.calendar_url, 3).unwrap_or_else(|e| {
        eprintln!("calendar fetch failed: {}", e);
        Vec::new()
    });
    let pickup = config.taxi.as_ref().and_then(|t| {
        match taxi::fetch(&t.api_token, t.customer_rivi_id, &t.weekday_ids) {
            Ok(Some(p)) => {
                let dt = chrono::DateTime::from_timestamp(p.scheduled_ts, 0)
                    .unwrap()
                    .with_timezone(&chrono_tz::Europe::Helsinki);
                println!("taxi: next pickup {}", dt.format("%Y-%m-%d %H:%M"));
                Some(p)
            }
            Ok(None) => {
                eprintln!("taxi: no upcoming pickup found");
                None
            }
            Err(e) => {
                eprintln!("taxi fetch failed: {}", e);
                None
            }
        }
    });
    FetchedData { weather, events, pickup }
}

fn render_and_paint(
    config: &Config,
    data: &FetchedData,
    on_device: bool,
) {
    let svg = render::build_svg(
        &config.location_name,
        data.weather.as_ref(),
        &data.events,
        data.pickup.as_ref(),
        read_battery_level(),
        CANVAS_W,
        CANVAS_H,
    );
    let pixmap = match render::render_to_pixmap(&svg, CANVAS_W, CANVAS_H) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("render failed: {}", e);
            return;
        }
    };

    if let Ok(png) = render::pixmap_to_png(&pixmap) {
        if std::fs::write(OUTPUT_PATH, &png).is_ok() {
            println!("wrote {} ({} bytes)", OUTPUT_PATH, png.len());
        }
    }

    if on_device {
        let gray = render::pixmap_to_grayscale(&pixmap);
        #[cfg(target_os = "linux")]
        if let Err(e) = fb::paint_grayscale(&gray, CANVAS_W, CANVAS_H) {
            eprintln!("fb paint failed: {}", e);
        }
        #[cfg(not(target_os = "linux"))]
        let _ = gray;
    }
}

fn main() {
    std::env::set_var("TZ", "Europe/Helsinki");

    let config = load_config();

    let on_device = cfg!(target_os = "linux") && std::path::Path::new("/dev/fb0").exists();

    let mut data = fetch_data(&config);
    let mut last_fetch = Instant::now();

    if !on_device {
        // Dev mode: render once and exit so the PNG can be inspected.
        render_and_paint(&config, &data, false);
        return;
    }

    let _screensaver_guard = ScreensaverGuard::engage();

    #[cfg(target_os = "linux")]
    let watcher = input::InputWatcher::spawn();

    loop {
        render_and_paint(&config, &data, true);

        let timeout = next_tick_timeout(data.pickup.as_ref());

        #[cfg(target_os = "linux")]
        if watcher.wait(timeout) {
            println!("got input, exiting");
            return;
        }
        #[cfg(not(target_os = "linux"))]
        thread::sleep(timeout);

        if last_fetch.elapsed() >= REFETCH_INTERVAL {
            data = fetch_data(&config);
            last_fetch = Instant::now();
        }
    }
}

/// Returns how long to wait until the next render tick.
/// Hourly when the taxi countdown shows whole hours, minutely when it shows minutes.
fn next_tick_timeout(pickup: Option<&TaxiPickup>) -> Duration {
    let now_secs = chrono::Utc::now().timestamp();
    let hourly = pickup
        .map(|p| {
            let remaining = p.scheduled_ts - 10 * 60 - now_secs;
            remaining >= 60 * 60
        })
        .unwrap_or(true);
    let secs = if hourly {
        3600 - now_secs.rem_euclid(3600)
    } else {
        60 - now_secs.rem_euclid(60)
    };
    Duration::from_secs(secs as u64)
}

fn lipc_path() -> Option<&'static str> {
    let p = "/usr/bin/lipc-set-prop";
    std::path::Path::new(p).exists().then_some(p)
}

fn set_wifi(enabled: bool) {
    let Some(lipc) = lipc_path() else { return };
    let _ = Command::new(lipc)
        .args([
            "com.lab126.cmd",
            "wirelessEnable",
            if enabled { "1" } else { "0" },
        ])
        .status();
}

fn wait_for_network() {
    if lipc_path().is_none() {
        return;
    }
    for _ in 0..15 {
        let ok = Command::new("ping")
            .args(["-c", "1", "-W", "1", "1.1.1.1"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return;
        }
        thread::sleep(Duration::from_secs(1));
    }
}

struct ScreensaverGuard;

impl ScreensaverGuard {
    fn engage() -> Self {
        Self::set(true);
        ScreensaverGuard
    }

    fn set(prevent: bool) {
        let Some(lipc) = lipc_path() else { return };
        let _ = Command::new(lipc)
            .args([
                "com.lab126.powerd",
                "preventScreenSaver",
                if prevent { "1" } else { "0" },
            ])
            .status();
    }
}

impl Drop for ScreensaverGuard {
    fn drop(&mut self) {
        Self::set(false);
    }
}
