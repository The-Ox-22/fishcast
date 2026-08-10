use serde::{Deserialize, Serialize};

use crate::tags::TagSet;

#[derive(Debug, Deserialize)]
pub struct RuleFile {
    #[serde(rename = "rule")]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
pub struct Rule {
    pub id: String,
    pub requires: Vec<String>,
    pub weight: u32,
    pub baits: Vec<BaitOption>,
    pub retrieve: String,
    #[serde(default)]
    pub target_structure: Vec<String>,
    pub why: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BaitOption {
    pub category: String,
    #[serde(default)]
    pub rig: Option<String>,
    #[serde(default)]
    pub variant: Option<String>,
    pub colors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

fn confidence_for_weight(weight: u32) -> Confidence {
    if weight >= 8 {
        Confidence::High
    } else if weight >= 4 {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub bait_category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rig: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    pub colors: Vec<String>,
    pub retrieve: String,
    pub confidence: Confidence,
    pub why: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructureSuggestion {
    pub feature: String,
    pub why: Vec<String>,
}

pub struct RuleEngine {
    rules: Vec<Rule>,
}

impl RuleEngine {
    pub fn load(toml_str: &str) -> anyhow::Result<Self> {
        let file: RuleFile = toml::from_str(toml_str)?;
        Ok(Self { rules: file.rules })
    }

    // Not called from the request-handling path (only `suggest` is), but
    // real pub API for introspecting a loaded ruleset - exercised by the
    // species coverage test and a natural fit for a future rules-listing
    // endpoint.
    #[allow(dead_code)]
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// A rule fires iff every tag in its `requires` is present in `tags`.
    /// Firing rules are ranked by weight desc (ties broken by rule id, for
    /// determinism), truncated to `top_n`, then merged: a bait_category
    /// seen in more than one contributing rule keeps the highest-weight
    /// rule's colors/retrieve, with `why` concatenated across all
    /// contributors. Same merge logic applies to target_structure features.
    pub fn suggest(&self, tags: &TagSet, top_n: usize) -> (Vec<Suggestion>, Vec<StructureSuggestion>) {
        let mut matched: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|r| r.requires.iter().all(|tag| tags.contains(tag)))
            .collect();
        matched.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.id.cmp(&b.id)));
        matched.truncate(top_n);

        let mut suggestions: Vec<Suggestion> = Vec::new();
        for rule in &matched {
            let confidence = confidence_for_weight(rule.weight);
            for bait in &rule.baits {
                if let Some(existing) = suggestions.iter_mut().find(|s| s.bait_category == bait.category) {
                    existing.why.push(rule.why.clone());
                } else {
                    suggestions.push(Suggestion {
                        bait_category: bait.category.clone(),
                        rig: bait.rig.clone(),
                        variant: bait.variant.clone(),
                        colors: bait.colors.clone(),
                        retrieve: rule.retrieve.clone(),
                        confidence,
                        why: vec![rule.why.clone()],
                    });
                }
            }
        }

        let mut structures: Vec<StructureSuggestion> = Vec::new();
        for rule in &matched {
            for feature in &rule.target_structure {
                if let Some(existing) = structures.iter_mut().find(|s| &s.feature == feature) {
                    existing.why.push(rule.why.clone());
                } else {
                    structures.push(StructureSuggestion {
                        feature: feature.clone(),
                        why: vec![rule.why.clone()],
                    });
                }
            }
        }

        (suggestions, structures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
[[rule]]
id = "post_frontal_tough_bite"
requires = ["light:bright", "temp_trend:cold_front_recent"]
weight = 10
baits = [
  { category = "finesse_worm", rig = "shaky_head", colors = ["green_pumpkin", "watermelon"] },
  { category = "jig", variant = "football", colors = ["brown_orange", "green_pumpkin"] },
]
retrieve = "slow, deadstick pauses"
target_structure = ["deeper cover fish pulled to after the front", "shaded docks"]
why = "Post-frontal bright skies plus a recent cold snap - fish are tight to cover and slow"

[[rule]]
id = "stained_water_wind_reaction"
requires = ["clarity:stained", "wind:moderate"]
weight = 8
baits = [
  { category = "spinnerbait", colors = ["chartreuse_white"] },
  { category = "chatterbait", colors = ["chartreuse_white", "black_blue"] },
]
retrieve = "steady, moderate speed"
target_structure = ["wind-blown banks", "vegetation edges"]
why = "Wind plus stained water hide the bait's flaws - reaction baits shine"

[[rule]]
id = "low_light_topwater"
requires = ["light:low"]
weight = 6
baits = [{ category = "topwater_walker", colors = ["bone", "black"] }]
retrieve = "walk-the-dog, steady cadence"
target_structure = ["points", "flats near spawning bays"]
why = "Low light puts fish shallow and aggressive"

[[rule]]
id = "windy_reaction_general"
requires = ["wind:moderate"]
weight = 8
baits = [{ category = "chatterbait", colors = ["white"] }]
retrieve = "steady"
target_structure = ["wind-blown points"]
why = "Wind alone favors reaction baits"
"#;

    #[test]
    fn loads_fixture_toml() {
        let engine = RuleEngine::load(FIXTURE).unwrap();
        assert_eq!(engine.rules().len(), 4);
    }

    #[test]
    fn matches_only_rules_whose_requires_is_a_subset_of_tags() {
        let engine = RuleEngine::load(FIXTURE).unwrap();
        let tags: TagSet = ["clarity:stained".into(), "wind:moderate".into()].into();
        let (suggestions, structures) = engine.suggest(&tags, 10);

        // stained_water_wind_reaction (requires both) and windy_reaction_general
        // (requires just wind) should fire; post_frontal and low_light should not.
        let categories: Vec<&str> = suggestions.iter().map(|s| s.bait_category.as_str()).collect();
        assert!(categories.contains(&"spinnerbait"));
        assert!(categories.contains(&"chatterbait"));
        assert!(!categories.contains(&"finesse_worm"));
        assert!(!categories.contains(&"topwater_walker"));

        let features: Vec<&str> = structures.iter().map(|s| s.feature.as_str()).collect();
        assert!(features.contains(&"wind-blown banks"));
        assert!(features.contains(&"wind-blown points"));
    }

    #[test]
    fn weight_desc_then_id_tie_break_ordering() {
        let engine = RuleEngine::load(FIXTURE).unwrap();
        // both stained_water_wind_reaction and windy_reaction_general have weight 8 and
        // both fire on this tag set; chatterbait is contributed by both (merge test),
        // spinnerbait only by the higher-alphabetical-id rule.
        let tags: TagSet = ["clarity:stained".into(), "wind:moderate".into()].into();
        let (suggestions, _) = engine.suggest(&tags, 10);
        let chatterbait = suggestions.iter().find(|s| s.bait_category == "chatterbait").unwrap();
        // "stained_water_wind_reaction" < "windy_reaction_general" lexicographically,
        // so it wins the merge and its colors should be first.
        assert_eq!(chatterbait.colors, vec!["chartreuse_white", "black_blue"]);
        assert_eq!(chatterbait.why.len(), 2);
    }

    #[test]
    fn top_n_truncation() {
        let engine = RuleEngine::load(FIXTURE).unwrap();
        let tags: TagSet = [
            "clarity:stained".into(),
            "wind:moderate".into(),
            "light:bright".into(),
            "temp_trend:cold_front_recent".into(),
            "light:low".into(), // contradictory with light:bright but fine for this synthetic test
        ]
        .into();
        let (suggestions, _) = engine.suggest(&tags, 1);
        // only the single highest-weight rule's baits should appear
        assert_eq!(suggestions.len(), 2); // post_frontal_tough_bite (weight 10) contributes 2 baits
    }

    #[test]
    fn no_matching_rules_returns_empty() {
        let engine = RuleEngine::load(FIXTURE).unwrap();
        let tags: TagSet = ["season:winter".into()].into();
        let (suggestions, structures) = engine.suggest(&tags, 10);
        assert!(suggestions.is_empty());
        assert!(structures.is_empty());
    }
}
