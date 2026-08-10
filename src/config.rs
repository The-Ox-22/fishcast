use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;

// Empty for now - fields land here once fishcast has something to configure
// (API keys, etc). Wiring is in place so that's a one-line addition later.
#[derive(Debug, Deserialize)]
pub struct Config {}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        Figment::new()
            .merge(Toml::file("config.toml"))
            .merge(Env::prefixed("FISHCAST_"))
            .extract()
            .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))
    }
}
