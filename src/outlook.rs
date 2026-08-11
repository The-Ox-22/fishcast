//! "Is now (or the next 48h) a good time to fish" - independent of bait
//! choice, this is a general conditions-favorability read from the same
//! signals research.md already established: pressure trend is the classic
//! single strongest factor, temp trend/stability next, then light/wind
//! combos and solunar timing. A small additive point score buckets into
//! four tiers rather than a TOML rule engine - unlike bait selection there's
//! no combinatorial output space here (one score, one dimension), so the
//! sparse-rule-matching machinery in rules.rs would be overkill.
//!
//! The 48h outlook reuses the same scorer against forecast data, but two
//! factors are intentionally dropped for forecast windows: temp_trend
//! (would need daily-mean data the way classify_temp_trend expects, which
//! doesn't extend usefully into an hourly forecast) and season_phase is
//! instead held constant at whatever it resolved to for "now" (it won't
//! meaningfully change within 48h). Everything else - pressure trend
//! within the window, sky, wind, solunar - is computed fresh per window.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::conditions::{
    PressureTrend, ResolvedConditions, SeasonPhase, Sky, SolunarPeriod, TempTrend, TimeOfDay,
};
use crate::solunar::SolunarWindow;
use crate::weather::HourlyForecastPoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FishingQuality {
    Poor,
    Fair,
    Good,
    Excellent,
}

struct QualityPoints {
    pressure_trend: Option<PressureTrend>,
    temp_trend: Option<TempTrend>,
    light_bonus: bool,
    calm_and_bright: bool,
    wind_moderate: bool,
    solunar_period: Option<SolunarPeriod>,
    season_phase: Option<SeasonPhase>,
}

fn score_points(f: &QualityPoints) -> i32 {
    let mut score = 0;

    score += match f.pressure_trend {
        Some(PressureTrend::Falling) => 2,
        Some(PressureTrend::Rising) => -2,
        Some(PressureTrend::Stable) | None => 0,
    };
    score += match f.temp_trend {
        Some(TempTrend::ColdFrontRecent) => -2,
        Some(TempTrend::Cooling) => -1,
        Some(TempTrend::RecoveringFromFront) | Some(TempTrend::Warming) => 1,
        Some(TempTrend::Stable) | None => 0,
    };
    if f.calm_and_bright {
        score -= 1;
    } else if f.light_bonus {
        score += 1;
    }
    if f.wind_moderate {
        score += 1;
    }
    score += match f.solunar_period {
        Some(SolunarPeriod::Major) => 2,
        Some(SolunarPeriod::Minor) => 1,
        Some(SolunarPeriod::Neutral) | None => 0,
    };
    score += match f.season_phase {
        Some(SeasonPhase::PreSpawn) | Some(SeasonPhase::Fall) => 1,
        Some(SeasonPhase::Winter) => -1,
        _ => 0,
    };

    score
}

fn bucket(score: i32) -> FishingQuality {
    if score <= -3 {
        FishingQuality::Poor
    } else if score <= 0 {
        FishingQuality::Fair
    } else if score <= 3 {
        FishingQuality::Good
    } else {
        FishingQuality::Excellent
    }
}

fn wind_moderate(wind_mph: f32) -> bool {
    (10.0..18.0).contains(&wind_mph)
}

/// `None` if there isn't enough signal to say anything meaningful - needs
/// at least one of the two anchor factors (pressure or temp trend).
pub fn quality_now(c: &ResolvedConditions) -> Option<FishingQuality> {
    let pressure_trend = c.pressure_trend.as_ref().map(|r| r.value);
    let temp_trend = c.temp_trend.as_ref().map(|r| r.value);
    if pressure_trend.is_none() && temp_trend.is_none() {
        return None;
    }

    let sky = c.sky.as_ref().map(|r| r.value);
    let wind_mph = c.wind_mph.as_ref().map(|r| r.value);
    let time_of_day = c.time_of_day.as_ref().map(|r| r.value);
    let solunar_period = c.solunar_period.as_ref().map(|r| r.value);
    let season_phase = c.season_phase.as_ref().map(|r| r.value);

    let is_daytime = matches!(time_of_day, Some(TimeOfDay::Day));
    let is_low_light = matches!(time_of_day, Some(TimeOfDay::Dawn) | Some(TimeOfDay::Dusk) | Some(TimeOfDay::Night));
    let calm_and_bright =
        is_daytime && matches!(sky, Some(Sky::Clear)) && wind_mph.is_some_and(|w| w < 5.0);
    let light_bonus = is_low_light || matches!(sky, Some(Sky::Overcast));

    let points = QualityPoints {
        pressure_trend,
        temp_trend,
        light_bonus,
        calm_and_bright,
        wind_moderate: wind_mph.is_some_and(wind_moderate),
        solunar_period,
        season_phase,
    };
    Some(bucket(score_points(&points)))
}

#[derive(Debug, Clone, Serialize)]
pub struct OutlookWindow {
    pub label: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub quality: FishingQuality,
}

/// General conditions favorability - independent of bait choice. `now` is
/// `None` when there isn't enough signal to say anything (see
/// `quality_now`); `next_48h` is empty when weather/solunar data wasn't
/// available at all (never partially populated).
#[derive(Debug, Clone, Serialize)]
pub struct FishingOutlook {
    pub now: Option<FishingQuality>,
    pub next_48h: Vec<OutlookWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Day,
    Night,
}

fn day_label(ordinal: usize) -> &'static str {
    match ordinal {
        0 => "Today",
        1 => "Tomorrow",
        _ => "In two days",
    }
}

fn night_label(ordinal: usize) -> &'static str {
    match ordinal {
        0 => "Tonight",
        1 => "Tomorrow night",
        _ => "In two nights",
    }
}

/// Builds day/night-aligned spans covering [now, now+48h), labeled relative
/// to "now" (Today/Tonight/Tomorrow/Tomorrow night). Approximates future
/// sunrise/sunset by shifting today's times by whole days - they drift only
/// ~1-2min/day, well within the precision this needs.
fn build_windows(
    now: DateTime<Utc>,
    sunrise_today: DateTime<Utc>,
    sunset_today: DateTime<Utc>,
) -> Vec<(String, Phase, DateTime<Utc>, DateTime<Utc>)> {
    let horizon = now + Duration::hours(48);

    let boundaries: Vec<(DateTime<Utc>, Phase)> = vec![
        (sunset_today - Duration::days(1), Phase::Night),
        (sunrise_today, Phase::Day),
        (sunset_today, Phase::Night),
        (sunrise_today + Duration::days(1), Phase::Day),
        (sunset_today + Duration::days(1), Phase::Night),
        (sunrise_today + Duration::days(2), Phase::Day),
        (sunset_today + Duration::days(2), Phase::Night),
    ];

    let mut day_count = 0usize;
    let mut night_count = 0usize;
    let mut windows = Vec::new();

    for pair in boundaries.windows(2) {
        let (start, phase) = pair[0];
        let (end, _) = pair[1];

        if start >= horizon {
            break;
        }
        if end <= now {
            continue;
        }

        let clipped_start = start.max(now);
        let clipped_end = end.min(horizon);

        let label = match phase {
            Phase::Day => {
                let l = day_label(day_count);
                day_count += 1;
                l
            }
            Phase::Night => {
                let l = night_label(night_count);
                night_count += 1;
                l
            }
        };

        // Drop slivers too short to be a useful UI window (a leftover
        // trailing sliver near the 48h mark is common - see the daytime
        // labeling test - but shouldn't be shown if it's only a few minutes).
        if clipped_end - clipped_start >= Duration::minutes(60) {
            windows.push((label.to_string(), phase, clipped_start, clipped_end));
        }
    }

    windows
}

fn mean_pressure(points: &[&HourlyForecastPoint]) -> Option<f32> {
    if points.is_empty() {
        return None;
    }
    Some(points.iter().map(|p| p.pressure_mb).sum::<f32>() / points.len() as f32)
}

fn majority_sky(points: &[&HourlyForecastPoint]) -> Option<Sky> {
    if points.is_empty() {
        return None;
    }
    let mut clear = 0;
    let mut partly = 0;
    let mut overcast = 0;
    for p in points {
        match p.sky {
            Sky::Clear => clear += 1,
            Sky::PartlyCloudy => partly += 1,
            Sky::Overcast => overcast += 1,
        }
    }
    if overcast >= clear && overcast >= partly {
        Some(Sky::Overcast)
    } else if clear >= partly {
        Some(Sky::Clear)
    } else {
        Some(Sky::PartlyCloudy)
    }
}

fn mean_wind(points: &[&HourlyForecastPoint]) -> Option<f32> {
    if points.is_empty() {
        return None;
    }
    Some(points.iter().map(|p| p.wind_mph).sum::<f32>() / points.len() as f32)
}

/// Checks the window against today's solunar windows plus copies shifted
/// +24h/+48h (the moon's cycle drifts ~50min/day, so this approximation's
/// error grows across the 48h horizon but stays smaller than a major
/// period's width). Major takes priority over Minor when both overlap.
fn solunar_period_for(windows: &[SolunarWindow], start: DateTime<Utc>, end: DateTime<Utc>) -> SolunarPeriod {
    let shifted = windows.iter().flat_map(|w| {
        [0, 1, 2].map(|days| SolunarWindow { kind: w.kind, center: w.center + Duration::days(days) })
    });

    let mut saw_minor = false;
    for w in shifted {
        if w.center >= start && w.center < end {
            if w.kind == SolunarPeriod::Major {
                return SolunarPeriod::Major;
            }
            saw_minor = true;
        }
    }
    if saw_minor {
        SolunarPeriod::Minor
    } else {
        SolunarPeriod::Neutral
    }
}

/// Builds the 48h outlook. `current_pressure_mb` anchors the first window's
/// pressure-trend comparison (there's no "window before window 0" otherwise).
pub fn outlook_48h(
    forecast: &[HourlyForecastPoint],
    current_pressure_mb: Option<f32>,
    sunrise_today: DateTime<Utc>,
    sunset_today: DateTime<Utc>,
    todays_solunar_windows: &[SolunarWindow],
    season_phase: Option<SeasonPhase>,
    now: DateTime<Utc>,
) -> Vec<OutlookWindow> {
    let windows = build_windows(now, sunrise_today, sunset_today);
    let mut prev_pressure = current_pressure_mb;
    let mut result = Vec::with_capacity(windows.len());

    for (label, phase, start, end) in windows {
        let points: Vec<&HourlyForecastPoint> =
            forecast.iter().filter(|p| p.time >= start && p.time < end).collect();

        let this_pressure = mean_pressure(&points).or(prev_pressure);
        let pressure_trend = match (prev_pressure, this_pressure) {
            (Some(prev), Some(this)) => {
                let delta = this - prev;
                Some(if delta <= -1.5 {
                    PressureTrend::Falling
                } else if delta >= 1.5 {
                    PressureTrend::Rising
                } else {
                    PressureTrend::Stable
                })
            }
            _ => None,
        };
        prev_pressure = this_pressure.or(prev_pressure);

        let is_daytime = phase == Phase::Day;
        let sky = majority_sky(&points);
        let wind = mean_wind(&points);
        let calm_and_bright = is_daytime && matches!(sky, Some(Sky::Clear)) && wind.is_some_and(|w| w < 5.0);
        let light_bonus = !is_daytime || matches!(sky, Some(Sky::Overcast));
        let solunar_period = solunar_period_for(todays_solunar_windows, start, end);

        let points_struct = QualityPoints {
            pressure_trend,
            temp_trend: None,
            light_bonus,
            calm_and_bright,
            wind_moderate: wind.is_some_and(wind_moderate),
            solunar_period: Some(solunar_period),
            season_phase,
        };

        result.push(OutlookWindow {
            label,
            start,
            end,
            quality: bucket(score_points(&points_struct)),
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditions::{Field, Resolved, Source};
    use chrono::NaiveDate;

    fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap().and_utc()
    }

    fn resolved<T>(value: T) -> Field<T> {
        Some(Resolved::new(value, Source::Derived))
    }

    #[test]
    fn bucket_thresholds() {
        assert_eq!(bucket(-5), FishingQuality::Poor);
        assert_eq!(bucket(-3), FishingQuality::Poor);
        assert_eq!(bucket(-2), FishingQuality::Fair);
        assert_eq!(bucket(0), FishingQuality::Fair);
        assert_eq!(bucket(1), FishingQuality::Good);
        assert_eq!(bucket(3), FishingQuality::Good);
        assert_eq!(bucket(4), FishingQuality::Excellent);
        assert_eq!(bucket(10), FishingQuality::Excellent);
    }

    #[test]
    fn quality_now_none_without_any_trend_signal() {
        let c = ResolvedConditions::default();
        assert_eq!(quality_now(&c), None);
    }

    #[test]
    fn quality_now_falling_pressure_and_major_solunar_is_excellent() {
        let mut c = ResolvedConditions::default();
        c.pressure_trend = resolved(PressureTrend::Falling);
        c.solunar_period = resolved(SolunarPeriod::Major);
        c.time_of_day = resolved(TimeOfDay::Dawn);
        assert_eq!(quality_now(&c), Some(FishingQuality::Excellent));
    }

    #[test]
    fn quality_now_rising_pressure_calm_bright_is_poor() {
        let mut c = ResolvedConditions::default();
        c.pressure_trend = resolved(PressureTrend::Rising);
        c.temp_trend = resolved(TempTrend::ColdFrontRecent);
        c.sky = resolved(Sky::Clear);
        c.time_of_day = resolved(TimeOfDay::Day);
        c.wind_mph = resolved(2.0);
        assert_eq!(quality_now(&c), Some(FishingQuality::Poor));
    }

    #[test]
    fn build_windows_labels_correctly_during_daytime() {
        let sunrise = dt(2026, 8, 10, 12, 0); // 12:00 UTC
        let sunset = dt(2026, 8, 11, 1, 0); // 01:00 UTC next day (crosses midnight)
        let now = dt(2026, 8, 10, 15, 0); // during today's daylight
        let windows = build_windows(now, sunrise, sunset);
        let labels: Vec<&str> = windows.iter().map(|(l, ..)| l.as_str()).collect();
        // Exactly 4 full-cycle windows are guaranteed; a 5th short leftover
        // sliver is expected too, since `now` rarely lands exactly on a
        // day/night boundary and the 48h horizon is a fixed offset from it.
        assert_eq!(&labels[..4], &["Today", "Tonight", "Tomorrow", "Tomorrow night"]);
        // first window starts exactly at `now`, not at the nominal sunrise
        assert_eq!(windows[0].2, now);
    }

    #[test]
    fn build_windows_labels_correctly_before_dawn() {
        let sunrise = dt(2026, 8, 10, 12, 0);
        let sunset = dt(2026, 8, 11, 1, 0);
        let now = dt(2026, 8, 10, 5, 0); // before today's sunrise
        let windows = build_windows(now, sunrise, sunset);
        let labels: Vec<&str> = windows.iter().map(|(l, ..)| l.as_str()).collect();
        assert_eq!(labels[0], "Tonight");
        assert_eq!(labels[1], "Today");
        assert_eq!(windows[0].2, now);
    }

    #[test]
    fn build_windows_cover_up_to_48h_without_gaps() {
        let sunrise = dt(2026, 8, 10, 12, 0);
        let sunset = dt(2026, 8, 11, 1, 0);
        let now = dt(2026, 8, 10, 15, 0);
        let windows = build_windows(now, sunrise, sunset);
        let horizon = now + Duration::hours(48);

        assert_eq!(windows.first().unwrap().2, now);
        for pair in windows.windows(2) {
            assert_eq!(pair[0].3, pair[1].2, "windows should be contiguous, no gaps");
        }
        assert!(windows.last().unwrap().3 <= horizon);
        assert!(horizon - windows.last().unwrap().3 < Duration::hours(24));
    }

    #[test]
    fn solunar_period_for_detects_major_overlap() {
        let windows = vec![SolunarWindow { kind: SolunarPeriod::Major, center: dt(2026, 8, 10, 14, 0) }];
        let period = solunar_period_for(&windows, dt(2026, 8, 10, 13, 30), dt(2026, 8, 10, 15, 0));
        assert_eq!(period, SolunarPeriod::Major);
    }

    #[test]
    fn solunar_period_for_shifted_day_plus_one() {
        let windows = vec![SolunarWindow { kind: SolunarPeriod::Minor, center: dt(2026, 8, 10, 9, 0) }];
        // no window on day 0 near this range, but the +1 day shift (9:00 on the 11th) should match
        let period = solunar_period_for(&windows, dt(2026, 8, 11, 8, 30), dt(2026, 8, 11, 9, 30));
        assert_eq!(period, SolunarPeriod::Minor);
    }

    #[test]
    fn outlook_48h_produces_windows_with_pressure_trend_from_forecast() {
        let sunrise = dt(2026, 8, 10, 12, 0);
        let sunset = dt(2026, 8, 11, 1, 0);
        let now = dt(2026, 8, 10, 15, 0);

        // sharply falling pressure through "Today"'s remaining hours
        let forecast: Vec<HourlyForecastPoint> = (0..10)
            .map(|h| HourlyForecastPoint {
                time: now + Duration::hours(h),
                pressure_mb: 1015.0 - (h as f32) * 1.0,
                sky: Sky::PartlyCloudy,
                wind_mph: 8.0,
            })
            .collect();

        let outlook = outlook_48h(&forecast, Some(1015.0), sunrise, sunset, &[], None, now);
        assert!(!outlook.is_empty());
        assert_eq!(outlook[0].label, "Today");
        // falling pressure across "Today" should not land on Poor
        assert_ne!(outlook[0].quality, FishingQuality::Poor);
    }
}
