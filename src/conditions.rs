use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::species::SpeciesProfile;
use crate::{solunar, water, weather};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Fetched,
    Provided,
    Derived,
}

#[derive(Debug, Clone, Serialize)]
pub struct Resolved<T> {
    pub value: T,
    pub source: Source,
}

impl<T> Resolved<T> {
    pub fn new(value: T, source: Source) -> Self {
        Self { value, source }
    }
}

/// `None` is the third state design.md calls "Unknown" - no source, not
/// supplied. Omitted from response JSON rather than serialized as a
/// null/"unknown" source.
pub type Field<T> = Option<Resolved<T>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeasonPhase {
    Winter,
    PreSpawn,
    Spawn,
    PostSpawn,
    Summer,
    Fall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureTrend {
    Falling,
    Rising,
    Stable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempTrend {
    Stable,
    Warming,
    Cooling,
    ColdFrontRecent,
    RecoveringFromFront,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterClarity {
    Clear,
    Stained,
    Muddy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterBodyType {
    Pond,
    NaturalLake,
    Reservoir,
    River,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterLevelTrend {
    Rising,
    Falling,
    Stable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cover {
    Vegetation,
    Laydowns,
    Riprap,
    Docks,
    Timber,
    NoneKnown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolunarPeriod {
    Major,
    Minor,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    Dawn,
    Day,
    Dusk,
    Night,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sky {
    Clear,
    PartlyCloudy,
    Overcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrecipRecent {
    None,
    Light,
    Heavy,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ResolvedConditions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_temp_f: Field<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sky: Field<Sky>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_mph: Field<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_direction_deg: Field<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pressure_mb: Field<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precip_recent: Field<PrecipRecent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pressure_trend: Field<PressureTrend>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_trend: Field<TempTrend>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_temp_f: Field<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_level_trend: Field<WaterLevelTrend>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_cfs: Field<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solunar_period: Field<SolunarPeriod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_of_day: Field<TimeOfDay>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_phase: Field<SeasonPhase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_clarity: Field<WaterClarity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_body_type: Field<WaterBodyType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Field<Vec<Cover>>,
}

/// Caller-supplied overrides for a `/api/v1/suggest` request. Any set field
/// wins over its live-fetched counterpart (source `Provided`). Fields with
/// no live source at all (clarity, water body type, cover) are only ever
/// populated this way.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct ConditionOverrides {
    pub water_clarity: Option<WaterClarity>,
    pub water_body_type: Option<WaterBodyType>,
    pub cover: Option<Vec<Cover>>,
    pub water_temp_f: Option<f32>,
    pub water_level_trend: Option<WaterLevelTrend>,
    pub air_temp_f: Option<f32>,
}

/// Orchestrates every live fetch concurrently, degrades failures to
/// Unknown (logged, never propagated as a request error - see
/// docs/design.md SS5), and applies overrides on top. All three fetchers
/// each supply several fields and only some of those are individually
/// overridable (e.g. water.rs also supplies `flow_cfs`, which has no
/// override) - so overrides win in the merge step below rather than
/// skipping the fetch itself, which would silently drop the other,
/// non-overridable fields that fetch would have supplied.
pub async fn resolve(
    client: &reqwest::Client,
    lat: f64,
    lon: f64,
    species: &SpeciesProfile,
    gauge_radius_mi: f64,
    overrides: &ConditionOverrides,
) -> ResolvedConditions {
    let now = Utc::now();

    let (weather_result, water_result, solunar_result) = tokio::join!(
        weather::fetch(client, lat, lon),
        water::fetch(client, lat, lon, gauge_radius_mi),
        solunar::fetch(client, lat, lon, now),
    );

    if let Err(e) = &weather_result {
        eprintln!("weather fetch failed: {e:#}");
    }
    if let Err(e) = &water_result {
        eprintln!("water fetch failed: {e:#}");
    }
    if let Err(e) = &solunar_result {
        eprintln!("solunar fetch failed: {e:#}");
    }

    let weather = weather_result.ok();
    let water = water_result.ok().flatten();
    let sun_moon = solunar_result.ok();

    if let Some(w) = &water {
        println!("using water gauge '{}' ({:.1}mi away)", w.site_name, w.distance_mi);
    }

    let mut c = ResolvedConditions::default();

    c.air_temp_f = match overrides.air_temp_f {
        Some(v) => Some(Resolved::new(v, Source::Provided)),
        None => weather.as_ref().map(|w| Resolved::new(w.air_temp_f, Source::Fetched)),
    };
    c.sky = weather.as_ref().map(|w| Resolved::new(w.sky, Source::Fetched));
    c.wind_mph = weather.as_ref().map(|w| Resolved::new(w.wind_mph, Source::Fetched));
    c.wind_direction_deg = weather.as_ref().map(|w| Resolved::new(w.wind_direction_deg, Source::Fetched));
    c.pressure_mb = weather.as_ref().map(|w| Resolved::new(w.pressure_mb, Source::Fetched));
    c.precip_recent = weather.as_ref().map(|w| Resolved::new(w.precip_recent, Source::Fetched));

    c.pressure_trend = weather
        .as_ref()
        .and_then(|w| classify_pressure_trend(&w.pressure_points))
        .map(|t| Resolved::new(t, Source::Derived));

    // Water temp trend takes precedence over air temp trend when available.
    let temp_trend_value = water
        .as_ref()
        .and_then(|w| classify_temp_trend(&w.water_temp_points))
        .or_else(|| weather.as_ref().and_then(|w| classify_temp_trend(&w.temp_points)));
    c.temp_trend = temp_trend_value.map(|t| Resolved::new(t, Source::Derived));

    c.water_temp_f = match overrides.water_temp_f {
        Some(v) => Some(Resolved::new(v, Source::Provided)),
        None => water.as_ref().and_then(|w| w.water_temp_f).map(|v| Resolved::new(v, Source::Fetched)),
    };
    c.water_level_trend = match overrides.water_level_trend {
        Some(v) => Some(Resolved::new(v, Source::Provided)),
        None => water.as_ref().and_then(|w| w.water_level_trend).map(|v| Resolved::new(v, Source::Fetched)),
    };
    c.flow_cfs = water.as_ref().and_then(|w| w.flow_cfs).map(|v| Resolved::new(v, Source::Fetched));

    c.solunar_period = sun_moon.as_ref().map(|s| Resolved::new(s.solunar_period, Source::Derived));
    c.time_of_day = sun_moon
        .as_ref()
        .map(|s| Resolved::new(classify_time_of_day(s.sunrise, s.sunset, now), Source::Derived));

    let season_input_temp = c
        .water_temp_f
        .as_ref()
        .map(|r| r.value)
        .or_else(|| c.air_temp_f.as_ref().map(|r| r.value));
    c.season_phase = season_input_temp.map(|t| {
        let trend = c.temp_trend.as_ref().map(|r| r.value);
        Resolved::new(
            classify_season_phase(t, species.spawn_temp_range_f, trend, now.date_naive()),
            Source::Derived,
        )
    });

    c.water_clarity = overrides.water_clarity.map(|v| Resolved::new(v, Source::Provided));
    c.water_body_type = overrides.water_body_type.map(|v| Resolved::new(v, Source::Provided));
    c.cover = overrides.cover.clone().map(|v| Resolved::new(v, Source::Provided));

    c
}

/// Dawn/dusk windows are 45min before to 30min after sunrise, and 30min
/// before to 45min after sunset - slightly asymmetric since low-light bass
/// activity tends to build before sunrise/linger after sunset rather than
/// being centered exactly on it.
fn classify_time_of_day(sunrise: DateTime<Utc>, sunset: DateTime<Utc>, now: DateTime<Utc>) -> TimeOfDay {
    let dawn_start = sunrise - Duration::minutes(45);
    let dawn_end = sunrise + Duration::minutes(30);
    let dusk_start = sunset - Duration::minutes(30);
    let dusk_end = sunset + Duration::minutes(45);

    if now >= dawn_start && now <= dawn_end {
        TimeOfDay::Dawn
    } else if now >= dusk_start && now <= dusk_end {
        TimeOfDay::Dusk
    } else if now > dawn_end && now < dusk_start {
        TimeOfDay::Day
    } else {
        TimeOfDay::Night
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PressurePoint {
    pub time: DateTime<Utc>,
    pub pressure_mb: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct TempPoint {
    // Not read by classify_temp_trend (which only needs ordered values),
    // but kept alongside the value since fetchers build points from a
    // date-keyed series and it's useful for debugging/future logging.
    #[allow(dead_code)]
    pub date: NaiveDate,
    pub mean_temp_f: f32,
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len() as f32
}

/// Falling >=2mb over the last 6h vs. the 6h window 24-42h back is the
/// classic pre-frontal signal; a matching rise after a low in the trailing
/// 48h is the post-frontal signal. See docs/design.md SS4.1.
pub fn classify_pressure_trend(points: &[PressurePoint]) -> Option<PressureTrend> {
    let now = points.iter().map(|p| p.time).max()?;
    let recent_start = now - Duration::hours(6);

    let recent: Vec<f32> = points
        .iter()
        .filter(|p| p.time >= recent_start)
        .map(|p| p.pressure_mb)
        .collect();
    let prior: Vec<f32> = points
        .iter()
        .filter(|p| p.time >= now - Duration::hours(42) && p.time < now - Duration::hours(24))
        .map(|p| p.pressure_mb)
        .collect();

    if recent.is_empty() || prior.is_empty() {
        return None;
    }

    let delta = mean(&recent) - mean(&prior);

    if delta <= -2.0 {
        return Some(PressureTrend::Falling);
    }
    if delta >= 2.0 {
        let min_point = points
            .iter()
            .min_by(|a, b| a.pressure_mb.partial_cmp(&b.pressure_mb).unwrap())?;
        if min_point.time < recent_start {
            return Some(PressureTrend::Rising);
        }
    }
    Some(PressureTrend::Stable)
}

/// Trailing 6 days split into two 3-day windows; thresholds sourced from
/// real cold-front water-temp-drop reporting (3-5F typical, 6-8F+ strong).
/// See docs/design.md SS4.2. `points` must be ordered oldest -> newest.
pub fn classify_temp_trend(points: &[TempPoint]) -> Option<TempTrend> {
    if points.len() < 6 {
        return None;
    }
    let n = points.len();
    let recent = &points[n - 3..];
    let early = &points[n - 6..n - 3];

    let recent_vals: Vec<f32> = recent.iter().map(|p| p.mean_temp_f).collect();
    let early_vals: Vec<f32> = early.iter().map(|p| p.mean_temp_f).collect();
    let delta = mean(&recent_vals) - mean(&early_vals);

    if delta.abs() < 4.0 {
        return Some(TempTrend::Stable);
    }
    if (4.0..8.0).contains(&delta) {
        return Some(TempTrend::Warming);
    }
    if (-8.0..=-4.0).contains(&delta) {
        return Some(TempTrend::Cooling);
    }
    if delta >= 8.0 {
        return Some(TempTrend::Warming);
    }

    // delta <= -8.0: strong drop. Still near the low -> tough-bite window;
    // already climbing back off it -> recovering.
    let min_recent = recent_vals.iter().cloned().fold(f32::INFINITY, f32::min);
    let last_two_mean = mean(&recent_vals[1..]);
    if last_two_mean - min_recent >= 2.0 {
        Some(TempTrend::RecoveringFromFront)
    } else {
        Some(TempTrend::ColdFrontRecent)
    }
}

/// temp_trend is the primary signal (warming -> spring-side, cooling ->
/// fall-side); calendar is only a fallback for the pre/post-spawn and
/// summer/post-spawn ambiguity when trend itself is stable/unknown. See
/// docs/design.md and the planning conversation for why trend leads here.
pub fn classify_season_phase(
    temp_f: f32,
    spawn_range_f: (f32, f32),
    temp_trend: Option<TempTrend>,
    now: NaiveDate,
) -> SeasonPhase {
    let (spawn_lo, spawn_hi) = spawn_range_f;

    if temp_f >= spawn_lo && temp_f <= spawn_hi {
        return SeasonPhase::Spawn;
    }

    // First half of the calendar year trends toward spawn temps (spring
    // warm-up); second half trends away from them, toward winter.
    let approaching_spawn_season = now.ordinal() < 182;

    if temp_f > spawn_hi {
        match temp_trend {
            Some(TempTrend::Cooling) | Some(TempTrend::ColdFrontRecent) => SeasonPhase::Fall,
            Some(TempTrend::Warming) | Some(TempTrend::RecoveringFromFront) => {
                SeasonPhase::PostSpawn
            }
            _ => {
                if approaching_spawn_season {
                    SeasonPhase::PostSpawn
                } else {
                    SeasonPhase::Summer
                }
            }
        }
    } else {
        match temp_trend {
            Some(TempTrend::Warming) | Some(TempTrend::RecoveringFromFront) => {
                SeasonPhase::PreSpawn
            }
            Some(TempTrend::Cooling) | Some(TempTrend::ColdFrontRecent) => SeasonPhase::Winter,
            _ => {
                if approaching_spawn_season {
                    SeasonPhase::PreSpawn
                } else {
                    SeasonPhase::Winter
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pp(hours_ago: i64, pressure_mb: f32) -> PressurePoint {
        PressurePoint {
            time: Utc::now() - Duration::hours(hours_ago),
            pressure_mb,
        }
    }

    #[test]
    fn pressure_trend_falling() {
        let points = vec![pp(0, 1008.0), pp(3, 1009.0), pp(6, 1010.0), pp(30, 1013.0), pp(36, 1012.5)];
        assert_eq!(classify_pressure_trend(&points), Some(PressureTrend::Falling));
    }

    #[test]
    fn pressure_trend_rising_after_low() {
        // low point sits before the recent 6h window; recent is >=2mb above the prior window
        let points = vec![
            pp(0, 1015.0),
            pp(3, 1014.0),
            pp(6, 1013.5),
            pp(20, 1005.0), // the low
            pp(30, 1008.0),
            pp(36, 1009.0),
        ];
        assert_eq!(classify_pressure_trend(&points), Some(PressureTrend::Rising));
    }

    #[test]
    fn pressure_trend_stable() {
        let points = vec![pp(0, 1013.0), pp(3, 1013.2), pp(30, 1013.1), pp(36, 1012.9)];
        assert_eq!(classify_pressure_trend(&points), Some(PressureTrend::Stable));
    }

    #[test]
    fn pressure_trend_none_without_data() {
        assert_eq!(classify_pressure_trend(&[]), None);
    }

    fn tp(days_ago: i64, temp: f32) -> TempPoint {
        TempPoint {
            date: chrono::Utc::now().date_naive() - Duration::days(days_ago),
            mean_temp_f: temp,
        }
    }

    fn ordered_points(early: [f32; 3], recent: [f32; 3]) -> Vec<TempPoint> {
        vec![
            tp(6, early[0]),
            tp(5, early[1]),
            tp(4, early[2]),
            tp(3, recent[0]),
            tp(2, recent[1]),
            tp(1, recent[2]),
        ]
    }

    #[test]
    fn temp_trend_stable_under_4f() {
        let points = ordered_points([70.0, 70.0, 70.0], [72.0, 71.0, 71.0]);
        assert_eq!(classify_temp_trend(&points), Some(TempTrend::Stable));
    }

    #[test]
    fn temp_trend_mild_warming() {
        let points = ordered_points([60.0, 60.0, 60.0], [65.0, 65.0, 66.0]);
        assert_eq!(classify_temp_trend(&points), Some(TempTrend::Warming));
    }

    #[test]
    fn temp_trend_mild_cooling() {
        let points = ordered_points([70.0, 70.0, 70.0], [65.0, 65.0, 64.0]);
        assert_eq!(classify_temp_trend(&points), Some(TempTrend::Cooling));
    }

    #[test]
    fn temp_trend_cold_front_recent() {
        // strong drop, still sitting near the bottom
        let points = ordered_points([75.0, 75.0, 75.0], [66.0, 65.0, 65.5]);
        assert_eq!(classify_temp_trend(&points), Some(TempTrend::ColdFrontRecent));
    }

    #[test]
    fn temp_trend_recovering_from_front() {
        // strong drop overall, but the last two days are climbing back off the low
        let points = ordered_points([75.0, 75.0, 75.0], [64.0, 66.5, 67.5]);
        assert_eq!(classify_temp_trend(&points), Some(TempTrend::RecoveringFromFront));
    }

    #[test]
    fn temp_trend_rapid_warmup_tags_as_warming() {
        let points = ordered_points([55.0, 55.0, 55.0], [65.0, 66.0, 66.0]);
        assert_eq!(classify_temp_trend(&points), Some(TempTrend::Warming));
    }

    #[test]
    fn temp_trend_none_with_insufficient_data() {
        assert_eq!(classify_temp_trend(&[tp(1, 60.0), tp(2, 61.0)]), None);
    }

    #[test]
    fn season_within_spawn_range_is_spawn() {
        let now = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        assert_eq!(
            classify_season_phase(65.0, (60.0, 75.0), Some(TempTrend::Stable), now),
            SeasonPhase::Spawn
        );
    }

    #[test]
    fn season_below_range_warming_is_pre_spawn() {
        let now = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        assert_eq!(
            classify_season_phase(52.0, (60.0, 75.0), Some(TempTrend::Warming), now),
            SeasonPhase::PreSpawn
        );
    }

    #[test]
    fn season_below_range_cooling_is_winter() {
        let now = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        assert_eq!(
            classify_season_phase(45.0, (60.0, 75.0), Some(TempTrend::Cooling), now),
            SeasonPhase::Winter
        );
    }

    #[test]
    fn season_above_range_cooling_is_fall() {
        let now = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        assert_eq!(
            classify_season_phase(80.0, (60.0, 75.0), Some(TempTrend::Cooling), now),
            SeasonPhase::Fall
        );
    }

    #[test]
    fn season_above_range_warming_is_post_spawn() {
        let now = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        assert_eq!(
            classify_season_phase(80.0, (60.0, 75.0), Some(TempTrend::Warming), now),
            SeasonPhase::PostSpawn
        );
    }

    #[test]
    fn season_above_range_stable_second_half_is_summer() {
        let now = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        assert_eq!(
            classify_season_phase(85.0, (60.0, 75.0), Some(TempTrend::Stable), now),
            SeasonPhase::Summer
        );
    }

    #[test]
    fn season_below_range_unknown_trend_falls_back_to_calendar() {
        let winter_side = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        assert_eq!(
            classify_season_phase(40.0, (60.0, 75.0), None, winter_side),
            SeasonPhase::Winter
        );
        let spring_side = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
        assert_eq!(
            classify_season_phase(40.0, (60.0, 75.0), None, spring_side),
            SeasonPhase::PreSpawn
        );
    }

    fn sun_dt(h: u32, m: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(2026, 8, 10).unwrap().and_hms_opt(h, m, 0).unwrap().and_utc()
    }

    #[test]
    fn time_of_day_windows() {
        let sunrise = sun_dt(6, 30);
        let sunset = sun_dt(20, 0);
        assert_eq!(classify_time_of_day(sunrise, sunset, sun_dt(6, 0)), TimeOfDay::Dawn);
        assert_eq!(classify_time_of_day(sunrise, sunset, sun_dt(12, 0)), TimeOfDay::Day);
        assert_eq!(classify_time_of_day(sunrise, sunset, sun_dt(19, 45)), TimeOfDay::Dusk);
        assert_eq!(classify_time_of_day(sunrise, sunset, sun_dt(2, 0)), TimeOfDay::Night);
    }

    #[tokio::test]
    #[ignore = "hits real Open-Meteo/USGS/sunrise-sunset.io APIs - run with `cargo test -- --ignored`"]
    async fn resolve_live_produces_plausible_mix_and_respects_overrides() {
        let client = reqwest::Client::new();
        let profile = crate::species::find("largemouth_bass").unwrap();
        let overrides = ConditionOverrides {
            water_clarity: Some(WaterClarity::Stained),
            water_temp_f: Some(77.0),
            ..Default::default()
        };

        let c = resolve(&client, 36.13, -97.07, profile, 25.0, &overrides).await;

        // Live-fetched fields should show up as Fetched or Derived, never absent
        // for a well-covered CONUS location.
        assert!(c.air_temp_f.is_some(), "expected air_temp_f from weather");
        assert!(c.pressure_trend.is_some(), "expected a derived pressure_trend");
        assert!(c.solunar_period.is_some(), "expected a derived solunar_period");

        // Overrides win and are tagged Provided.
        let clarity = c.water_clarity.expect("water_clarity should be set from override");
        assert_eq!(clarity.value, WaterClarity::Stained);
        assert_eq!(clarity.source, Source::Provided);

        let temp = c.water_temp_f.expect("water_temp_f should be set from override");
        assert_eq!(temp.value, 77.0);
        assert_eq!(temp.source, Source::Provided);

        // Fields with no live source and no override stay Unknown.
        assert!(c.water_body_type.is_none());
    }
}
