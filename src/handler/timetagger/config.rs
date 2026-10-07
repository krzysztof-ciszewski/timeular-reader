use crate::config::Config;
use anyhow::Result;
use serde::{Deserialize, Serialize};

const CONFIG_KEY: &str = "timetagger";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TimetaggerConfig {
    pub timetagger_url: String,
    pub api_key: String,
}

impl<'de> Config<'de> for TimetaggerConfig {}

pub fn create_config() -> Result<TimetaggerConfig> {
    crate::config::get_config::<TimetaggerConfig>(CONFIG_KEY)
}

pub fn update_config(config: &TimetaggerConfig) -> Result<()> {
    crate::config::update_config(CONFIG_KEY, config)
}

#[cfg(test)]
mod tests {
    use super::TimetaggerConfig;

    #[test]
    fn default_config_leaves_credentials_unset() {
        let config = TimetaggerConfig::default();
        assert!(config.api_key.is_empty());
        assert!(config.timetagger_url.is_empty());
    }
}
