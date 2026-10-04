use crate::config::Config;
use serde::{Deserialize, Serialize};

const CONFIG_KEY: &str = "timetagger";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TimetaggerConfig {
    pub timetagger_url: String,
    pub api_key: String,
}

impl<'de> Config<'de> for TimetaggerConfig {}

pub fn create_config() -> TimetaggerConfig {
    crate::config::get_config::<TimetaggerConfig>(CONFIG_KEY)
}

pub fn update_config(config: &TimetaggerConfig) {
    crate::config::update_config(CONFIG_KEY, config);
}
