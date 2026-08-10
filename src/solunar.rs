//! Sun/moon rise-set fetch (sunrise-sunset.io) plus Knight's solunar theory
//! implemented ourselves - no paid solunar API needed, per docs/design.md.
//!
//! Knight's theory ties "major" feeding periods to moon transit (when the
//! moon crosses the local meridian, both overhead and underfoot) and
//! "minor" periods to moonrise/moonset. sunrise-sunset.io doesn't expose
//! transit time directly, so it's approximated as the midpoint between
//! moonrise and moonset (or derived from whichever one is known using the
//! ~24h50m moon cycle) - a standard simplification, not true ephemeris
//! computation. Worth revisiting with a proper astronomical library if the
//! approximation ever matters enough to justify it.

use anyhow::{bail, Context};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use serde::Deserialize;

use crate::conditions::SolunarPeriod;

const MAJOR_HALF_WIDTH_MIN: i64 = 60; // major periods are ~2h wide
const MINOR_HALF_WIDTH_MIN: i64 = 30; // minor periods are ~1h wide
const LUNAR_DAY_MINUTES: i64 = (24.833 * 60.0) as i64; // moon's transit-to-transit cycle, ~24h50m

pub struct SunMoonSnapshot {
    pub sunrise: DateTime<Utc>,
    pub sunset: DateTime<Utc>,
    pub solunar_period: SolunarPeriod,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    results: ApiResults,
    status: String,
}

#[derive(Debug, Deserialize)]
struct ApiResults {
    date: NaiveDate,
    sunrise: String,
    sunset: String,
    moonrise: Option<String>,
    moonset: Option<String>,
}

pub async fn fetch(client: &reqwest::Client, lat: f64, lon: f64, at: DateTime<Utc>) -> anyhow::Result<SunMoonSnapshot> {
    let response = client
        .get("https://api.sunrisesunset.io/json")
        .query(&[
            ("lat", lat.to_string()),
            ("lng", lon.to_string()),
            ("timezone", "UTC".to_string()),
            ("date", at.date_naive().to_string()),
        ])
        .send()
        .await
        .context("failed to reach sunrise-sunset.io")?;

    if !response.status().is_success() {
        bail!("sunrise-sunset.io returned {}", response.status());
    }

    let parsed: ApiResponse = response
        .json()
        .await
        .context("failed to parse sunrise-sunset.io response")?;

    if parsed.status != "OK" {
        bail!("sunrise-sunset.io returned status {}", parsed.status);
    }

    build_snapshot(parsed.results, at)
}

fn build_snapshot(r: ApiResults, at: DateTime<Utc>) -> anyhow::Result<SunMoonSnapshot> {
    let sunrise_time = parse_clock_time(&r.sunrise).context("failed to parse sunrise time")?;
    let sunrise = r.date.and_time(sunrise_time).and_utc();
    let sunset = parse_relative_to(r.date, sunrise_time, &r.sunset).context("failed to parse sunset time")?;
    let moonrise = r.moonrise.as_deref().and_then(|s| parse_relative_to(r.date, sunrise_time, s).ok());
    let moonset = r.moonset.as_deref().and_then(|s| parse_relative_to(r.date, sunrise_time, s).ok());

    let windows = knights_solunar_windows(moonrise, moonset);
    let solunar_period = current_solunar_period(&windows, at);

    Ok(SunMoonSnapshot { sunrise, sunset, solunar_period })
}

fn parse_clock_time(s: &str) -> anyhow::Result<NaiveTime> {
    NaiveTime::parse_from_str(s, "%I:%M:%S %p").with_context(|| format!("bad time string: {s}"))
}

/// Times in this API's response can roll past UTC midnight relative to the
/// queried calendar date (e.g. sunset landing at 1 AM UTC the next day, for
/// a location whose afternoon falls after 00:00 UTC). Anchors everything
/// to sunrise - if a field's clock time is earlier than sunrise's, it's
/// assumed to belong to the following day.
fn parse_relative_to(date: NaiveDate, sunrise_time: NaiveTime, s: &str) -> anyhow::Result<DateTime<Utc>> {
    let t = parse_clock_time(s)?;
    let mut dt = date.and_time(t).and_utc();
    if t < sunrise_time {
        dt += Duration::days(1);
    }
    Ok(dt)
}

pub struct SolunarWindow {
    pub kind: SolunarPeriod,
    pub center: DateTime<Utc>,
}

pub fn knights_solunar_windows(moonrise: Option<DateTime<Utc>>, moonset: Option<DateTime<Utc>>) -> Vec<SolunarWindow> {
    let mut windows = Vec::new();

    if let Some(mr) = moonrise {
        windows.push(SolunarWindow { kind: SolunarPeriod::Minor, center: mr });
    }
    if let Some(ms) = moonset {
        windows.push(SolunarWindow { kind: SolunarPeriod::Minor, center: ms });
    }

    let half_cycle = Duration::minutes(LUNAR_DAY_MINUTES / 2);
    let transit = match (moonrise, moonset) {
        (Some(mr), Some(ms)) if ms > mr => Some(mr + (ms - mr) / 2),
        (Some(mr), _) => Some(mr + half_cycle / 2),
        (_, Some(ms)) => Some(ms - half_cycle / 2),
        (None, None) => None,
    };

    if let Some(t) = transit {
        windows.push(SolunarWindow { kind: SolunarPeriod::Major, center: t });
        windows.push(SolunarWindow { kind: SolunarPeriod::Major, center: t + half_cycle });
        windows.push(SolunarWindow { kind: SolunarPeriod::Major, center: t - half_cycle });
    }

    windows
}

pub fn current_solunar_period(windows: &[SolunarWindow], at: DateTime<Utc>) -> SolunarPeriod {
    let within = |w: &&SolunarWindow, half_width_min: i64| (at - w.center).num_minutes().abs() <= half_width_min;

    if windows.iter().any(|w| w.kind == SolunarPeriod::Major && within(&w, MAJOR_HALF_WIDTH_MIN)) {
        return SolunarPeriod::Major;
    }
    if windows.iter().any(|w| w.kind == SolunarPeriod::Minor && within(&w, MINOR_HALF_WIDTH_MIN)) {
        return SolunarPeriod::Minor;
    }
    SolunarPeriod::Neutral
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap().and_utc()
    }

    #[test]
    fn minor_windows_center_on_rise_and_set() {
        let mr = dt(2026, 8, 10, 8, 58);
        let ms = dt(2026, 8, 10, 21, 30);
        let windows = knights_solunar_windows(Some(mr), Some(ms));
        assert!(windows.iter().any(|w| w.kind == SolunarPeriod::Minor && w.center == mr));
        assert!(windows.iter().any(|w| w.kind == SolunarPeriod::Minor && w.center == ms));
    }

    #[test]
    fn major_window_is_midpoint_of_rise_and_set() {
        let mr = dt(2026, 8, 10, 8, 0);
        let ms = dt(2026, 8, 10, 20, 0);
        let windows = knights_solunar_windows(Some(mr), Some(ms));
        let majors: Vec<_> = windows.iter().filter(|w| w.kind == SolunarPeriod::Major).collect();
        assert_eq!(majors.len(), 3); // midpoint transit + opposite transit on either side
        assert!(majors.iter().any(|w| w.center == dt(2026, 8, 10, 14, 0)));
    }

    #[test]
    fn current_period_matches_within_major_window() {
        let windows = vec![SolunarWindow { kind: SolunarPeriod::Major, center: dt(2026, 8, 10, 14, 0) }];
        assert_eq!(current_solunar_period(&windows, dt(2026, 8, 10, 14, 30)), SolunarPeriod::Major);
        assert_eq!(current_solunar_period(&windows, dt(2026, 8, 10, 17, 0)), SolunarPeriod::Neutral);
    }

    #[test]
    fn current_period_matches_within_minor_window() {
        let windows = vec![SolunarWindow { kind: SolunarPeriod::Minor, center: dt(2026, 8, 10, 8, 58) }];
        assert_eq!(current_solunar_period(&windows, dt(2026, 8, 10, 9, 10)), SolunarPeriod::Minor);
        assert_eq!(current_solunar_period(&windows, dt(2026, 8, 10, 10, 0)), SolunarPeriod::Neutral);
    }

    #[test]
    fn major_takes_priority_over_overlapping_minor() {
        let windows = vec![
            SolunarWindow { kind: SolunarPeriod::Minor, center: dt(2026, 8, 10, 14, 0) },
            SolunarWindow { kind: SolunarPeriod::Major, center: dt(2026, 8, 10, 14, 0) },
        ];
        assert_eq!(current_solunar_period(&windows, dt(2026, 8, 10, 14, 0)), SolunarPeriod::Major);
    }

    #[test]
    fn no_moon_data_gives_no_windows() {
        assert!(knights_solunar_windows(None, None).is_empty());
    }

    #[test]
    fn parses_fixture_response() {
        let json = r#"{
            "results": {
                "date": "2026-08-10",
                "sunrise": "11:39:35 AM",
                "sunset": "1:27:06 AM",
                "moonrise": "8:58:38 AM",
                "moonset": null
            },
            "status": "OK",
            "tzid": "UTC"
        }"#;
        let parsed: ApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.status, "OK");
        let snapshot = build_snapshot(parsed.results, dt(2026, 8, 10, 12, 0)).unwrap();
        // sunset (1:27:06 AM) is earlier-clock-time than sunrise (11:39:35 AM) -> rolls to next day
        assert_eq!(
            snapshot.sunset,
            NaiveDate::from_ymd_opt(2026, 8, 11).unwrap().and_hms_opt(1, 27, 6).unwrap().and_utc()
        );
        assert_eq!(
            snapshot.sunrise,
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap().and_hms_opt(11, 39, 35).unwrap().and_utc()
        );
    }

    #[tokio::test]
    #[ignore = "hits real sunrise-sunset.io API - run with `cargo test -- --ignored`"]
    async fn fetch_live() {
        let client = reqwest::Client::new();
        let now = Utc::now();
        let snapshot = fetch(&client, 36.13, -97.07, now).await.unwrap();
        assert!(snapshot.sunset > snapshot.sunrise);
    }
}
