use anyhow::{bail, Context};
use chrono::NaiveDate;
use serde::Deserialize;

use crate::conditions::{PrecipRecent, PressurePoint, Sky, TempPoint};

pub struct WeatherSnapshot {
    pub air_temp_f: f32,
    pub sky: Sky,
    pub wind_mph: f32,
    pub wind_direction_deg: f32,
    pub pressure_mb: f32,
    pub precip_recent: PrecipRecent,
    pub pressure_points: Vec<PressurePoint>,
    pub temp_points: Vec<TempPoint>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoResponse {
    current: CurrentBlock,
    hourly: HourlyBlock,
    daily: DailyBlock,
}

#[derive(Debug, Deserialize)]
struct CurrentBlock {
    temperature_2m: f32,
    wind_speed_10m: f32,
    wind_direction_10m: f32,
    weather_code: u32,
    pressure_msl: f32,
}

#[derive(Debug, Deserialize)]
struct HourlyBlock {
    time: Vec<String>,
    pressure_msl: Vec<f32>,
    precipitation: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct DailyBlock {
    time: Vec<String>,
    temperature_2m_mean: Vec<f32>,
}

/// A single `/v1/forecast` call with `past_days=7` covers both the 48h
/// pressure window and the 6-day temp window - Open-Meteo's dedicated
/// historical-archive endpoint has a multi-day lag that would be unusable
/// for the pressure window. See docs/design.md SS4.
pub async fn fetch(client: &reqwest::Client, lat: f64, lon: f64) -> anyhow::Result<WeatherSnapshot> {
    let response = client
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            (
                "current",
                "temperature_2m,wind_speed_10m,wind_direction_10m,weather_code,pressure_msl".to_string(),
            ),
            ("hourly", "pressure_msl,precipitation".to_string()),
            ("daily", "temperature_2m_mean".to_string()),
            ("past_days", "7".to_string()),
            ("forecast_days", "1".to_string()),
            ("temperature_unit", "fahrenheit".to_string()),
            ("wind_speed_unit", "mph".to_string()),
            ("timezone", "UTC".to_string()),
        ])
        .send()
        .await
        .context("failed to reach Open-Meteo")?;

    if !response.status().is_success() {
        bail!("Open-Meteo returned {}", response.status());
    }

    let parsed: OpenMeteoResponse = response
        .json()
        .await
        .context("failed to parse Open-Meteo response")?;

    build_snapshot(parsed)
}

fn build_snapshot(resp: OpenMeteoResponse) -> anyhow::Result<WeatherSnapshot> {
    let sky = sky_from_weather_code(resp.current.weather_code);

    let recent_precip_mm: f32 = resp.hourly.precipitation.iter().rev().take(24).sum();
    let precip_recent = classify_precip(recent_precip_mm);

    let pressure_points = resp
        .hourly
        .time
        .iter()
        .zip(resp.hourly.pressure_msl.iter())
        .filter_map(|(t, p)| {
            parse_hourly_time(t).map(|time| PressurePoint { time, pressure_mb: *p })
        })
        .collect();

    let temp_points = resp
        .daily
        .time
        .iter()
        .zip(resp.daily.temperature_2m_mean.iter())
        .filter_map(|(d, t)| parse_date(d).map(|date| TempPoint { date, mean_temp_f: *t }))
        .collect();

    Ok(WeatherSnapshot {
        air_temp_f: resp.current.temperature_2m,
        sky,
        wind_mph: resp.current.wind_speed_10m,
        wind_direction_deg: resp.current.wind_direction_10m,
        pressure_mb: resp.current.pressure_msl,
        precip_recent,
        pressure_points,
        temp_points,
    })
}

fn classify_precip(mm_24h: f32) -> PrecipRecent {
    if mm_24h >= 10.0 {
        PrecipRecent::Heavy
    } else if mm_24h > 0.5 {
        PrecipRecent::Light
    } else {
        PrecipRecent::None
    }
}

/// WMO weather codes: 0-1 clear/mainly clear, 2 partly cloudy, everything
/// else (overcast, fog, precipitation) treated as overcast for the "sky"
/// tag's purposes.
fn sky_from_weather_code(code: u32) -> Sky {
    match code {
        0 | 1 => Sky::Clear,
        2 => Sky::PartlyCloudy,
        _ => Sky::Overcast,
    }
}

fn parse_hourly_time(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .ok()
        .map(|naive| naive.and_utc())
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
{
  "current": {
    "temperature_2m": 101.8,
    "wind_speed_10m": 13.7,
    "wind_direction_10m": 178,
    "weather_code": 0,
    "pressure_msl": 1011.7
  },
  "hourly": {
    "time": ["2026-08-09T20:00", "2026-08-09T21:00", "2026-08-10T20:00", "2026-08-10T21:00"],
    "pressure_msl": [1013.0, 1012.5, 1011.9, 1011.7],
    "precipitation": [0.0, 0.0, 2.0, 0.0]
  },
  "daily": {
    "time": ["2026-08-04", "2026-08-05", "2026-08-06", "2026-08-07", "2026-08-08", "2026-08-09", "2026-08-10"],
    "temperature_2m_mean": [86.8, 89.7, 89.7, 84.5, 82.9, 89.8, 90.3]
  }
}
"#;

    #[test]
    fn parses_fixture_into_snapshot() {
        let parsed: OpenMeteoResponse = serde_json::from_str(FIXTURE).unwrap();
        let snapshot = build_snapshot(parsed).unwrap();

        assert_eq!(snapshot.air_temp_f, 101.8);
        assert_eq!(snapshot.sky, Sky::Clear);
        assert_eq!(snapshot.wind_mph, 13.7);
        assert_eq!(snapshot.wind_direction_deg, 178.0);
        assert_eq!(snapshot.pressure_mb, 1011.7);
        assert_eq!(snapshot.pressure_points.len(), 4);
        assert_eq!(snapshot.temp_points.len(), 7);
        assert_eq!(snapshot.temp_points[0].mean_temp_f, 86.8);
    }

    #[test]
    fn precip_recent_thresholds() {
        assert_eq!(classify_precip(0.0), PrecipRecent::None);
        assert_eq!(classify_precip(2.0), PrecipRecent::Light);
        assert_eq!(classify_precip(15.0), PrecipRecent::Heavy);
    }

    #[test]
    fn weather_code_mapping() {
        assert_eq!(sky_from_weather_code(0), Sky::Clear);
        assert_eq!(sky_from_weather_code(1), Sky::Clear);
        assert_eq!(sky_from_weather_code(2), Sky::PartlyCloudy);
        assert_eq!(sky_from_weather_code(3), Sky::Overcast);
        assert_eq!(sky_from_weather_code(61), Sky::Overcast);
    }

    #[tokio::test]
    #[ignore = "hits real Open-Meteo API - run with `cargo test -- --ignored`"]
    async fn fetch_live() {
        let client = reqwest::Client::new();
        let snapshot = fetch(&client, 36.13, -97.07).await.unwrap();
        assert!(snapshot.air_temp_f > -50.0 && snapshot.air_temp_f < 130.0);
        assert!(!snapshot.pressure_points.is_empty());
        assert!(snapshot.temp_points.len() >= 6);
    }
}
