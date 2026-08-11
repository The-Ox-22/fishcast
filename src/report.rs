use std::collections::HashMap;

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::conditions::{self, ConditionOverrides, ResolvedConditions, SeasonPhase};
use crate::location::{self, LocationInput, ResolvedLocation};
use crate::outlook::FishingOutlook;
use crate::rules::{RuleEngine, StructureSuggestion, Suggestion};
use crate::{species, tags};

fn default_species() -> String {
    "largemouth_bass".to_string()
}

#[derive(Debug, Deserialize)]
pub struct SuggestRequest {
    #[serde(default = "default_species")]
    pub species: String,
    pub location: LocationInput,
    /// Parsed for validation/round-trip but intentionally unused -
    /// conditions always resolve as of actual request time. Forward-looking
    /// requests are explicitly out of v1 scope (docs/design.md SS6); the
    /// field exists now so the API shape doesn't need to change later.
    #[serde(default)]
    #[allow(dead_code)]
    pub at: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    pub conditions: ConditionOverrides,
}

#[derive(Debug, Serialize)]
pub struct SuggestResponse {
    pub location: ResolvedLocation,
    pub resolved_conditions: ResolvedConditions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_phase: Option<SeasonPhase>,
    pub suggestions: Vec<Suggestion>,
    pub target_structure: Vec<StructureSuggestion>,
    /// General conditions favorability, independent of bait choice - see
    /// src/outlook.rs.
    pub fishing_outlook: FishingOutlook,
}

#[derive(Debug)]
pub enum ReportError {
    UnknownSpecies(String),
    LocationResolution(anyhow::Error),
}

impl std::fmt::Display for ReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReportError::UnknownSpecies(id) => write!(f, "unknown species '{id}'"),
            ReportError::LocationResolution(e) => write!(f, "could not resolve location: {e:#}"),
        }
    }
}

impl std::error::Error for ReportError {}

pub async fn suggest(
    client: &reqwest::Client,
    engines: &HashMap<&'static str, RuleEngine>,
    gauge_radius_mi: f64,
    rule_top_n: usize,
    req: SuggestRequest,
) -> Result<SuggestResponse, ReportError> {
    let profile = species::find(&req.species).ok_or_else(|| ReportError::UnknownSpecies(req.species.clone()))?;
    let engine = engines
        .get(profile.id)
        .expect("a RuleEngine is registered for every species in species::all()");

    let resolved_location = location::resolve(client, &req.location)
        .await
        .map_err(ReportError::LocationResolution)?;

    let (resolved_conditions, fishing_outlook) = conditions::resolve(
        client,
        resolved_location.lat,
        resolved_location.lon,
        profile,
        gauge_radius_mi,
        &req.conditions,
    )
    .await;

    let tag_set = tags::derive_tags(&resolved_conditions);
    let (suggestions, target_structure) = engine.suggest(&tag_set, rule_top_n);
    let season_phase = resolved_conditions.season_phase.as_ref().map(|r| r.value);

    Ok(SuggestResponse {
        location: resolved_location,
        resolved_conditions,
        season_phase,
        suggestions,
        target_structure,
        fishing_outlook,
    })
}
