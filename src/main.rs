#[cfg(target_os = "linux")]
mod fb;
mod ical;
mod input;
mod render;
mod weather;

use serde::Deserialize;
use std::process::Command;
use std::thread;
use std::time::Duration;

const OUTPUT_PATH: &str = "/tmp/weather.png";

#[derive(Deserialize)]
struct Config {
    location_name: String,
    latitude: f64,
    longitude: f64,
    calendar_url: String,
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

fn main() {
    std::env::set_var("TZ", "Europe/Helsinki");

    let config = load_config();

    enable_wifi_and_wait();

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

    let svg = render::build_svg(&config.location_name, weather.as_ref(), &events, CANVAS_W, CANVAS_H);
    let pixmap = render::render_to_pixmap(&svg, CANVAS_W, CANVAS_H).expect("render");

    // Save PNG copy for debugging / off-device preview
    if let Ok(png) = render::pixmap_to_png(&pixmap) {
        if std::fs::write(OUTPUT_PATH, &png).is_ok() {
            println!("wrote {} ({} bytes)", OUTPUT_PATH, png.len());
        }
    }

    let gray = render::pixmap_to_grayscale(&pixmap);

    #[cfg(target_os = "linux")]
    if std::path::Path::new("/dev/fb0").exists() {
        match fb::paint_grayscale(&gray, CANVAS_W, CANVAS_H) {
            Ok(()) => {
                println!("painted to /dev/fb0");
                println!("waiting for input event…");
                input::wait_for_input();
                println!("got input, exiting");
            }
            Err(e) => eprintln!("fb paint failed: {}", e),
        }
    }
}

fn enable_wifi_and_wait() {
    if !std::path::Path::new("/usr/bin/lipc-set-prop").exists() {
        return;
    }
    let _ = Command::new("/usr/bin/lipc-set-prop")
        .args(["com.lab126.cmd", "wirelessEnable", "1"])
        .status();
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
