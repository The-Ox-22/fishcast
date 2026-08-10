//! USGS Water Data (api.waterdata.usgs.gov, the OGC API replacing the
//! legacy waterservices.usgs.gov) is the sole live water-condition source
//! in v1. USACE CWMS (the reservoir-elevation second source design.md
//! proposed) was investigated during implementation and dropped: it has no
//! lat/lon or bbox search at all - locations are only queryable by USACE
//! district office, and there is no API to resolve "which office covers
//! this point" either. A real integration would need a static
//! lat/lon -> office lookup table (out of scope for v1). USGS does cover
//! some reservoirs directly (site_type_code `LK`), so reservoir coverage
//! is opportunistic rather than guaranteed - consistent with the "no
//! single source covers all lakes" reality docs/design.md already called
//! out.

use anyhow::Context;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Deserialize;

use crate::conditions::{TempPoint, WaterLevelTrend};

const USGS_BASE: &str = "https://api.waterdata.usgs.gov/ogcapi/v0";
const PARAM_WATER_TEMP_C: &str = "00010";
const PARAM_DISCHARGE_CFS: &str = "00060";
const PARAM_GAGE_HEIGHT_FT: &str = "00065";
/// A "latest" reading older than this isn't trustworthy as current data -
/// it means the site's sensor for that parameter has gone quiet.
const FRESHNESS_LIMIT_HOURS: i64 = 72;

pub struct WaterSnapshot {
    pub water_temp_f: Option<f32>,
    pub flow_cfs: Option<f32>,
    pub water_level_trend: Option<WaterLevelTrend>,
    /// Trailing daily water-temp history, oldest -> newest, for
    /// `classify_temp_trend` - takes precedence over air temp when present.
    pub water_temp_points: Vec<TempPoint>,
    pub site_name: String,
    pub distance_mi: f64,
}

/// `Ok(None)` means the query succeeded but nothing usable was found within
/// `radius_mi` - a legitimate "no data here" outcome, distinct from `Err`
/// (network/parse failure), which the caller should log.
pub async fn fetch(
    client: &reqwest::Client,
    lat: f64,
    lon: f64,
    radius_mi: f64,
) -> anyhow::Result<Option<WaterSnapshot>> {
    let candidates = find_candidate_sites(client, lat, lon, radius_mi).await?;

    for candidate in candidates {
        if let Some(snapshot) = try_build_snapshot(client, &candidate).await? {
            return Ok(Some(snapshot));
        }
    }
    Ok(None)
}

struct CandidateSite {
    id: String,
    name: String,
    distance_mi: f64,
}

#[derive(Debug, Deserialize)]
struct FeatureCollection<P> {
    features: Vec<Feature<P>>,
}

#[derive(Debug, Deserialize)]
struct Feature<P> {
    properties: P,
    geometry: Geometry,
}

#[derive(Debug, Deserialize)]
struct Geometry {
    coordinates: (f64, f64), // (lon, lat), GeoJSON order
}

#[derive(Debug, Deserialize)]
struct MonitoringLocationProps {
    id: String,
    monitoring_location_name: String,
}

async fn find_candidate_sites(
    client: &reqwest::Client,
    lat: f64,
    lon: f64,
    radius_mi: f64,
) -> anyhow::Result<Vec<CandidateSite>> {
    let (min_lon, min_lat, max_lon, max_lat) = bbox(lat, lon, radius_mi);
    let bbox_str = format!("{min_lon},{min_lat},{max_lon},{max_lat}");

    // The API's site_type_code filter takes exactly one value - a
    // comma-separated list is matched as a literal string (always zero
    // results), not an OR. Query each type separately and merge.
    let mut candidates: Vec<CandidateSite> = Vec::new();
    for site_type in ["ST", "LK"] {
        let response = client
            .get(format!("{USGS_BASE}/collections/monitoring-locations/items"))
            .query(&[
                ("bbox", bbox_str.as_str()),
                ("site_type_code", site_type),
                ("f", "json"),
                // dense metro bboxes can have 800+ stream sites; the API's
                // default page size (200) silently truncates before distance
                // filtering gets a chance to find the actually-nearest one.
                ("limit", "2000"),
            ])
            .send()
            .await
            .context("failed to reach USGS monitoring-locations")?;

        if !response.status().is_success() {
            anyhow::bail!("USGS monitoring-locations returned {}", response.status());
        }

        let parsed: FeatureCollection<MonitoringLocationProps> = response
            .json()
            .await
            .context("failed to parse USGS monitoring-locations response")?;

        candidates.extend(parsed.features.into_iter().map(|f| {
            let (site_lon, site_lat) = f.geometry.coordinates;
            CandidateSite {
                id: f.properties.id,
                name: f.properties.monitoring_location_name,
                distance_mi: haversine_mi(lat, lon, site_lat, site_lon),
            }
        }));
    }

    candidates.retain(|c| c.distance_mi <= radius_mi);
    candidates.sort_by(|a, b| a.distance_mi.partial_cmp(&b.distance_mi).unwrap());
    Ok(candidates)
}

#[derive(Debug, Deserialize)]
struct LatestContinuousProps {
    parameter_code: String,
    time: DateTime<Utc>,
    value: String,
}

async fn try_build_snapshot(
    client: &reqwest::Client,
    candidate: &CandidateSite,
) -> anyhow::Result<Option<WaterSnapshot>> {
    let response = client
        .get(format!("{USGS_BASE}/collections/latest-continuous/items"))
        .query(&[
            ("monitoring_location_id", candidate.id.as_str()),
            (
                "parameter_code",
                &format!("{PARAM_WATER_TEMP_C},{PARAM_DISCHARGE_CFS},{PARAM_GAGE_HEIGHT_FT}"),
            ),
            ("f", "json"),
            ("limit", "50"),
        ])
        .send()
        .await
        .with_context(|| format!("failed to reach USGS latest-continuous for {}", candidate.id))?;

    if !response.status().is_success() {
        anyhow::bail!("USGS latest-continuous returned {}", response.status());
    }

    let parsed: FeatureCollection<LatestContinuousProps> = response
        .json()
        .await
        .context("failed to parse USGS latest-continuous response")?;

    let now = Utc::now();
    let mut latest_by_param: std::collections::HashMap<String, (DateTime<Utc>, f64)> = std::collections::HashMap::new();
    for f in parsed.features {
        let Ok(value) = f.properties.value.parse::<f64>() else {
            continue;
        };
        latest_by_param
            .entry(f.properties.parameter_code)
            .and_modify(|(t, v)| {
                if f.properties.time > *t {
                    *t = f.properties.time;
                    *v = value;
                }
            })
            .or_insert((f.properties.time, value));
    }

    let fresh = |param: &str| -> Option<f64> {
        latest_by_param.get(param).and_then(|(t, v)| {
            (now - *t <= Duration::hours(FRESHNESS_LIMIT_HOURS)).then_some(*v)
        })
    };

    let water_temp_f = fresh(PARAM_WATER_TEMP_C).map(|c| celsius_to_fahrenheit(c) as f32);
    let flow_cfs = fresh(PARAM_DISCHARGE_CFS).map(|v| v as f32);

    if water_temp_f.is_none() && flow_cfs.is_none() && fresh(PARAM_GAGE_HEIGHT_FT).is_none() {
        return Ok(None);
    }

    let water_level_trend = fetch_level_trend(client, &candidate.id).await.unwrap_or(None);
    let water_temp_points = fetch_water_temp_points(client, &candidate.id).await.unwrap_or_default();

    Ok(Some(WaterSnapshot {
        water_temp_f,
        flow_cfs,
        water_level_trend,
        water_temp_points,
        site_name: candidate.name.clone(),
        distance_mi: candidate.distance_mi,
    }))
}

#[derive(Debug, Deserialize)]
struct DailyValueProps {
    time: NaiveDate,
    value: String,
}

/// Prefers gage height (the more direct "water level" signal) over
/// discharge as a proxy, since not every site computes a daily gage-height
/// statistic. Either way, `classify_level_trend`'s threshold is a
/// reasonable default (design.md doesn't specify exact numbers for this
/// axis) - worth tuning once this has been observed against real data.
async fn fetch_level_trend(client: &reqwest::Client, site_id: &str) -> anyhow::Result<Option<WaterLevelTrend>> {
    for param in [PARAM_GAGE_HEIGHT_FT, PARAM_DISCHARGE_CFS] {
        let points = fetch_daily_series(client, site_id, param).await?;
        let values: Vec<f64> = points.into_iter().map(|(_, v)| v).collect();
        if let Some(trend) = classify_level_trend(&values) {
            return Ok(Some(trend));
        }
    }
    Ok(None)
}

async fn fetch_daily_series(client: &reqwest::Client, site_id: &str, param: &str) -> anyhow::Result<Vec<(NaiveDate, f64)>> {
    let now = Utc::now();
    let start = now - Duration::days(7);
    let response = client
        .get(format!("{USGS_BASE}/collections/daily/items"))
        .query(&[
            ("monitoring_location_id", site_id),
            ("parameter_code", param),
            ("statistic_id", "00003"), // daily mean
            ("datetime", &format!("{}/{}", start.to_rfc3339(), now.to_rfc3339())),
            ("f", "json"),
            ("limit", "20"),
        ])
        .send()
        .await
        .with_context(|| format!("failed to reach USGS daily values for {site_id}"))?;

    if !response.status().is_success() {
        anyhow::bail!("USGS daily values returned {}", response.status());
    }

    let parsed: FeatureCollection<DailyValueProps> = response
        .json()
        .await
        .context("failed to parse USGS daily values response")?;

    let mut points: Vec<(NaiveDate, f64)> = parsed
        .features
        .into_iter()
        .filter_map(|f| f.properties.value.parse::<f64>().ok().map(|v| (f.properties.time, v)))
        .collect();
    points.sort_by_key(|(d, _)| *d);
    Ok(points)
}

/// Water temp trend takes precedence over air temp trend when available
/// (docs/design.md SS4.2) since it's the more direct signal - this is what
/// callers use to build that series.
async fn fetch_water_temp_points(client: &reqwest::Client, site_id: &str) -> anyhow::Result<Vec<TempPoint>> {
    let points = fetch_daily_series(client, site_id, PARAM_WATER_TEMP_C).await?;
    Ok(points
        .into_iter()
        .map(|(date, c)| TempPoint { date, mean_temp_f: celsius_to_fahrenheit(c) as f32 })
        .collect())
}

/// Splits the trailing week into two halves and compares their means; a
/// relative (not absolute) threshold since this is shared between gage
/// height (feet, small numbers) and discharge (cfs, large numbers).
fn classify_level_trend(points: &[f64]) -> Option<WaterLevelTrend> {
    if points.len() < 4 {
        return None;
    }
    let mid = points.len() / 2;
    let (early, recent) = points.split_at(mid);
    let early_mean = early.iter().sum::<f64>() / early.len() as f64;
    let recent_mean = recent.iter().sum::<f64>() / recent.len() as f64;
    if early_mean == 0.0 {
        return None;
    }
    let relative_change = (recent_mean - early_mean) / early_mean;

    if relative_change >= 0.05 {
        Some(WaterLevelTrend::Rising)
    } else if relative_change <= -0.05 {
        Some(WaterLevelTrend::Falling)
    } else {
        Some(WaterLevelTrend::Stable)
    }
}

fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

fn bbox(lat: f64, lon: f64, radius_mi: f64) -> (f64, f64, f64, f64) {
    const MI_PER_DEG_LAT: f64 = 69.0;
    let lat_delta = radius_mi / MI_PER_DEG_LAT;
    let lon_delta = radius_mi / (MI_PER_DEG_LAT * lat.to_radians().cos().max(0.01));
    (lon - lon_delta, lat - lat_delta, lon + lon_delta, lat + lat_delta)
}

fn haversine_mi(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_MI: f64 = 3958.8;
    let (lat1, lon1, lat2, lon2) = (lat1.to_radians(), lon1.to_radians(), lat2.to_radians(), lon2.to_radians());
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    EARTH_RADIUS_MI * 2.0 * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_zero_distance() {
        assert!(haversine_mi(36.13, -97.07, 36.13, -97.07) < 0.001);
    }

    #[test]
    fn haversine_known_distance_roughly_correct() {
        // Stillwater, OK to Tulsa, OK is roughly 60 miles
        let d = haversine_mi(36.13, -97.07, 36.15, -95.99);
        assert!((50.0..70.0).contains(&d), "got {d}");
    }

    #[test]
    fn bbox_contains_the_center_point() {
        let (min_lon, min_lat, max_lon, max_lat) = bbox(36.13, -97.07, 25.0);
        assert!(min_lat < 36.13 && 36.13 < max_lat);
        assert!(min_lon < -97.07 && -97.07 < max_lon);
    }

    #[test]
    fn nearest_candidate_sorting() {
        let mut candidates = vec![
            CandidateSite { id: "far".into(), name: "far".into(), distance_mi: 20.0 },
            CandidateSite { id: "near".into(), name: "near".into(), distance_mi: 2.0 },
            CandidateSite { id: "mid".into(), name: "mid".into(), distance_mi: 10.0 },
        ];
        candidates.sort_by(|a, b| a.distance_mi.partial_cmp(&b.distance_mi).unwrap());
        let ids: Vec<&str> = candidates.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["near", "mid", "far"]);
    }

    #[test]
    fn celsius_conversion() {
        assert!((celsius_to_fahrenheit(0.0) - 32.0).abs() < 0.01);
        assert!((celsius_to_fahrenheit(31.2) - 88.16).abs() < 0.01);
    }

    #[test]
    fn level_trend_rising_falling_stable() {
        assert_eq!(classify_level_trend(&[10.0, 10.0, 12.0, 12.0]), Some(WaterLevelTrend::Rising));
        assert_eq!(classify_level_trend(&[12.0, 12.0, 10.0, 10.0]), Some(WaterLevelTrend::Falling));
        assert_eq!(classify_level_trend(&[10.0, 10.1, 10.0, 9.95]), Some(WaterLevelTrend::Stable));
        assert_eq!(classify_level_trend(&[10.0, 10.0]), None);
    }

    #[test]
    fn latest_continuous_props_parse_string_value_and_rfc3339_time() {
        let json = r#"{"features":[{"properties":{"parameter_code":"00010","time":"2026-08-10T20:50:00+00:00","value":"31.2"},"geometry":{"coordinates":[-77.1,38.9]}}]}"#;
        let parsed: FeatureCollection<LatestContinuousProps> = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.features.len(), 1);
        assert_eq!(parsed.features[0].properties.value, "31.2");
    }

    #[tokio::test]
    #[ignore = "hits real USGS Water Data API - run with `cargo test -- --ignored`"]
    async fn fetch_live_near_a_well_instrumented_river() {
        let client = reqwest::Client::new();
        // Near the Potomac gauge (USGS-01646500), which reliably reports water temp
        let result = fetch(&client, 38.9498, -77.1276, 25.0).await.unwrap();
        let snapshot = result.expect("expected a nearby gauge with data");
        assert!(snapshot.water_temp_f.is_some() || snapshot.flow_cfs.is_some());
    }

    #[tokio::test]
    #[ignore = "hits real USGS Water Data API - run with `cargo test -- --ignored`"]
    async fn fetch_live_returns_none_far_from_any_gauge() {
        let client = reqwest::Client::new();
        // middle of the Pacific ocean - no gauges within 25mi
        let result = fetch(&client, 20.0, -150.0, 25.0).await.unwrap();
        assert!(result.is_none());
    }
}
