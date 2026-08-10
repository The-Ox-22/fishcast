use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    /// How far from the caller's location a USGS water gauge is still
    /// trusted as representative. See docs/design.md SS4.3.
    #[serde(default = "default_usgs_gauge_radius_mi")]
    pub usgs_gauge_radius_mi: f64,
    /// How many top-ranked rules contribute to a /suggest response.
    #[serde(default = "default_rule_top_n")]
    pub rule_top_n: usize,
}

fn default_usgs_gauge_radius_mi() -> f64 {
    25.0
}

fn default_rule_top_n() -> usize {
    4
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        Figment::new()
            .merge(Toml::file("config.toml"))
            .merge(Env::prefixed("FISHCAST_"))
            .extract()
            .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))
    }
}
