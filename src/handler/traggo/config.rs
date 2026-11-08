use crate::config::Config;
use anyhow::Result;
use serde::{Deserialize, Serialize};

const CONFIG_KEY: &str = "traggo";

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct TraggoConfig {
    pub graphql_url: String,
    pub api_token: String,
}

impl<'de> Config<'de> for TraggoConfig {}

pub fn create_config() -> Result<TraggoConfig> {
    crate::config::get_config::<TraggoConfig>(CONFIG_KEY)
}

pub fn update_config(config: &TraggoConfig) -> Result<()> {
    crate::config::update_config(CONFIG_KEY, config)
}

#[cfg(test)]
mod tests {
    use super::TraggoConfig;

    #[test]
    fn default_config_leaves_connection_unset() {
        let config = TraggoConfig::default();

        assert!(config.graphql_url.is_empty());
        assert!(config.api_token.is_empty());
    }

    #[test]
    fn legacy_global_tag_key_is_not_used_or_saved() {
        let config: TraggoConfig = toml::from_str(
            r#"
graphql_url = "https://traggo.example/graphql"
api_token = "device-token"
tag_key = "activity"
"#,
        )
        .unwrap();

        let serialized = toml::to_string(&config).unwrap();
        assert!(!serialized.contains("tag_key"));
        assert_eq!(toml::from_str::<TraggoConfig>(&serialized).unwrap(), config);
    }
}
