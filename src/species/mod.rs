mod largemouth_bass;

pub struct SpeciesProfile {
    pub id: &'static str,
    pub display_name: &'static str,
    pub spawn_temp_range_f: (f32, f32),
    pub rules_toml: &'static str,
}

static ALL: [SpeciesProfile; 1] = [largemouth_bass::PROFILE];

pub fn all() -> &'static [SpeciesProfile] {
    &ALL
}

pub fn find(id: &str) -> Option<&'static SpeciesProfile> {
    all().iter().find(|s| s.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleEngine;

    #[test]
    fn largemouth_bass_rules_load_and_cover_the_major_axes() {
        let profile = find("largemouth_bass").expect("largemouth_bass registered");
        let engine = RuleEngine::load(profile.rules_toml).expect("rules parse");

        let rule_count = engine.rules().len();
        assert!(
            (15..=25).contains(&rule_count),
            "expected 15-25 rules, got {rule_count}"
        );

        let axes = [
            "season:",
            "light:",
            "wind:",
            "clarity:",
            "water_body:",
            "pressure_trend:",
            "temp_trend:",
        ];
        for axis in axes {
            let covered = engine
                .rules()
                .iter()
                .any(|r| r.requires.iter().any(|tag| tag.starts_with(axis)));
            assert!(covered, "no rule references axis {axis}");
        }
    }

    #[test]
    fn find_unknown_species_returns_none() {
        assert!(find("striped_bass").is_none());
    }
}
