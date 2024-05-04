use serde_derive::{Deserialize, Serialize};
use crate::config::Config;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExampleConfig {
    pub base_url: String,
    pub api_key: String,
}

impl Default for ExampleConfig {
    fn default() -> Self {
        ExampleConfig {
            base_url: String::from("https://api.example.com"),
            api_key: String::new(),
        }
    }
}

impl<'de> Config<'de> for ExampleConfig {}

const CONFIG_KEY: &str = "example";

pub fn create_config() -> ExampleConfig {
    crate::config::get_config::<ExampleConfig>(CONFIG_KEY)
}

pub fn update_config(config: &ExampleConfig) {
    crate::config::update_config(CONFIG_KEY, config);
}
