use std::error::Error;

use cookie_store::Cookie;
use serde::{Deserialize, Serialize};

use crate::config::Config;

const CONFIG_KEY: &str = "hackaru";

#[derive(Serialize, Deserialize, Clone)]
pub struct HackaruConfig {
    pub hackaru_url: String,
    pub activities_rel_url: String,
    pub email: String,
    pub project_id: u64,
    pub cookies: String,
}

impl Default for HackaruConfig {
    fn default() -> Self {
        HackaruConfig {
            hackaru_url: String::from("https://api.hackaru.app/"),
            activities_rel_url: String::from("v1/activities"),
            email: String::new(),
            project_id: 1,
            cookies: String::new(),
        }
    }
}
impl<'de> Config<'de> for HackaruConfig {}

impl HackaruConfig {
    pub fn get_cookies(&self) -> Vec<Result<Cookie<'static>, Box<dyn Error>>> {
        let cookies_str = self.cookies.as_str();
        if cookies_str.is_empty() {
            return vec![];
        }

        let cookies: Vec<Cookie> = serde_json::from_str(cookies_str).unwrap();
        cookies.into_iter().by_ref().map(Ok).collect()
    }
}

pub fn create_config() -> HackaruConfig {
    crate::config::get_config::<HackaruConfig>(CONFIG_KEY)
}

pub fn update_config(config: &HackaruConfig) {
    crate::config::update_config(CONFIG_KEY, config);
}
