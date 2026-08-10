use std::collections::HashSet;

use crate::conditions::{PressureTrend, ResolvedConditions, SeasonPhase, Sky, SolunarPeriod, TempTrend, TimeOfDay, WaterBodyType, WaterClarity, WaterLevelTrend};

pub type TagSet = HashSet<String>;

pub fn derive_tags(c: &ResolvedConditions) -> TagSet {
    let mut tags = TagSet::new();
    for tag in [
        season_tag(c),
        light_tag(c),
        wind_tag(c),
        clarity_tag(c),
        water_body_tag(c),
        pressure_trend_tag(c),
        temp_trend_tag(c),
        solunar_tag(c),
        water_level_trend_tag(c),
    ]
    .into_iter()
    .flatten()
    {
        tags.insert(tag);
    }
    tags
}

fn season_tag(c: &ResolvedConditions) -> Option<String> {
    let v = c.season_phase.as_ref()?.value;
    let s = match v {
        SeasonPhase::Winter => "winter",
        SeasonPhase::PreSpawn => "pre_spawn",
        SeasonPhase::Spawn => "spawn",
        SeasonPhase::PostSpawn => "post_spawn",
        SeasonPhase::Summer => "summer",
        SeasonPhase::Fall => "fall",
    };
    Some(format!("season:{s}"))
}

fn light_tag(c: &ResolvedConditions) -> Option<String> {
    let sky = c.sky.as_ref()?.value;
    let tod = c.time_of_day.as_ref()?.value;
    let level = match (sky, tod) {
        (Sky::Overcast, _) => "overcast",
        (_, TimeOfDay::Dawn | TimeOfDay::Dusk | TimeOfDay::Night) => "low",
        _ => "bright",
    };
    Some(format!("light:{level}"))
}

fn wind_tag(c: &ResolvedConditions) -> Option<String> {
    let mph = c.wind_mph.as_ref()?.value;
    let level = if mph < 5.0 {
        "calm"
    } else if mph < 10.0 {
        "light"
    } else if mph < 18.0 {
        "moderate"
    } else {
        "windy"
    };
    Some(format!("wind:{level}"))
}

fn clarity_tag(c: &ResolvedConditions) -> Option<String> {
    let v = c.water_clarity.as_ref()?.value;
    let s = match v {
        WaterClarity::Clear => "clear",
        WaterClarity::Stained => "stained",
        WaterClarity::Muddy => "muddy",
    };
    Some(format!("clarity:{s}"))
}

fn water_body_tag(c: &ResolvedConditions) -> Option<String> {
    let v = c.water_body_type.as_ref()?.value;
    let s = match v {
        WaterBodyType::Pond => "pond",
        WaterBodyType::NaturalLake => "natural_lake",
        WaterBodyType::Reservoir => "reservoir",
        WaterBodyType::River => "river",
    };
    Some(format!("water_body:{s}"))
}

fn pressure_trend_tag(c: &ResolvedConditions) -> Option<String> {
    let v = c.pressure_trend.as_ref()?.value;
    let s = match v {
        PressureTrend::Falling => "falling",
        PressureTrend::Rising => "rising",
        PressureTrend::Stable => "stable",
    };
    Some(format!("pressure_trend:{s}"))
}

fn temp_trend_tag(c: &ResolvedConditions) -> Option<String> {
    let v = c.temp_trend.as_ref()?.value;
    let s = match v {
        TempTrend::Stable => "stable",
        TempTrend::Warming => "warming",
        TempTrend::Cooling => "cooling",
        TempTrend::ColdFrontRecent => "cold_front_recent",
        TempTrend::RecoveringFromFront => "recovering_from_front",
    };
    Some(format!("temp_trend:{s}"))
}

fn solunar_tag(c: &ResolvedConditions) -> Option<String> {
    let v = c.solunar_period.as_ref()?.value;
    let s = match v {
        SolunarPeriod::Major => "major",
        SolunarPeriod::Minor => "minor",
        SolunarPeriod::Neutral => "neutral",
    };
    Some(format!("solunar:{s}"))
}

fn water_level_trend_tag(c: &ResolvedConditions) -> Option<String> {
    let v = c.water_level_trend.as_ref()?.value;
    let s = match v {
        WaterLevelTrend::Rising => "rising",
        WaterLevelTrend::Falling => "falling",
        WaterLevelTrend::Stable => "stable",
    };
    Some(format!("water_level_trend:{s}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditions::{Resolved, Source};

    #[test]
    fn derives_expected_tags_from_full_conditions() {
        let mut c = ResolvedConditions::default();
        c.season_phase = Some(Resolved::new(SeasonPhase::Summer, Source::Derived));
        c.sky = Some(Resolved::new(Sky::Overcast, Source::Fetched));
        c.time_of_day = Some(Resolved::new(TimeOfDay::Day, Source::Derived));
        c.wind_mph = Some(Resolved::new(12.0, Source::Fetched));
        c.water_clarity = Some(Resolved::new(WaterClarity::Stained, Source::Provided));
        c.water_body_type = Some(Resolved::new(WaterBodyType::Reservoir, Source::Provided));
        c.pressure_trend = Some(Resolved::new(PressureTrend::Falling, Source::Derived));
        c.temp_trend = Some(Resolved::new(TempTrend::Warming, Source::Derived));
        c.solunar_period = Some(Resolved::new(SolunarPeriod::Major, Source::Derived));

        let tags = derive_tags(&c);
        assert_eq!(
            tags,
            TagSet::from([
                "season:summer".to_string(),
                "light:overcast".to_string(),
                "wind:moderate".to_string(),
                "clarity:stained".to_string(),
                "water_body:reservoir".to_string(),
                "pressure_trend:falling".to_string(),
                "temp_trend:warming".to_string(),
                "solunar:major".to_string(),
            ])
        );
    }

    #[test]
    fn unknown_fields_produce_no_tags() {
        let c = ResolvedConditions::default();
        assert!(derive_tags(&c).is_empty());
    }

    #[test]
    fn light_tag_requires_both_sky_and_time_of_day() {
        let mut c = ResolvedConditions::default();
        c.sky = Some(Resolved::new(Sky::Clear, Source::Fetched));
        // time_of_day missing
        assert!(derive_tags(&c).iter().all(|t| !t.starts_with("light:")));
    }
}
