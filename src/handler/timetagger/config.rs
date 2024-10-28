use crate::config::Config;
use serde::{Deserialize, Serialize};

const CONFIG_KEY: &str = "timetagger";
pub const DEFAULT_RECORDS_API_URL: &str = "https://timetagger.app/api/v2/records";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct TimetaggerConfig {
    pub timetagger_url: String,
    pub api_key: String,
}

impl Default for TimetaggerConfig {
    fn default() -> Self {
        TimetaggerConfig {
            timetagger_url: String::from(DEFAULT_RECORDS_API_URL),
            api_key: String::new(),
        }
    }
}

impl<'de> Config<'de> for TimetaggerConfig {}

pub fn create_config() -> TimetaggerConfig {
    crate::config::get_config::<TimetaggerConfig>(CONFIG_KEY)
}

pub fn update_config(config: &TimetaggerConfig) {
    crate::config::update_config(CONFIG_KEY, config);
}
