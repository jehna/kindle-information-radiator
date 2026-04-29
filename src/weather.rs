use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct WeatherData {
    pub current: Current,
    pub hourly: Hourly,
}

#[derive(Deserialize, Debug)]
pub struct Current {
    pub temperature_2m: f64,
    pub weather_code: u32,
}

#[derive(Deserialize, Debug)]
pub struct Hourly {
    pub time: Vec<String>,
    pub temperature_2m: Vec<f64>,
    pub weather_code: Vec<u32>,
}

pub fn fetch(lat: f64, lon: f64) -> Result<WeatherData, String> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
         &current=temperature_2m,weather_code\
         &hourly=temperature_2m,weather_code\
         &timezone=Europe%2FHelsinki&forecast_hours=12",
        lat, lon
    );
    let resp = minreq::get(&url)
        .with_timeout(15)
        .send()
        .map_err(|e| format!("http: {}", e))?;
    if resp.status_code != 200 {
        return Err(format!("HTTP {}", resp.status_code));
    }
    let body = resp.as_str().map_err(|e| format!("body: {}", e))?;
    serde_json::from_str(body).map_err(|e| format!("json: {}", e))
}
