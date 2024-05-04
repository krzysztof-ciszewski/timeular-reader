use reqwest_cookie_store::CookieStore;
use serde::{Deserialize, Serialize};

use crate::config::Config;

const CONFIG_KEY: &str = "hackaru";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HackaruConfig {
    pub hackaru_url: String,
    pub activities_rel_url: String,
    pub email: String,
    pub project_id: u64,
    pub cookies: String,
    pub password: String,
}

impl Default for HackaruConfig {
    fn default() -> Self {
        HackaruConfig {
            hackaru_url: String::new(),
            activities_rel_url: String::from("v1/activities"),
            email: String::new(),
            project_id: 0,
            cookies: String::new(),
            password: String::new(),
        }
    }
}
impl<'de> Config<'de> for HackaruConfig {}

impl HackaruConfig {
    pub fn get_cookie_store(&self) -> CookieStore {
        let cookies_str = self.cookies.as_str();
        if cookies_str.is_empty() {
            return CookieStore::default();
        }

        let mut buf: &[u8] = cookies_str.as_bytes();
        return CookieStore::load_json(&mut buf).unwrap();
    }
}

pub fn create_config() -> HackaruConfig {
    crate::config::get_config::<HackaruConfig>(CONFIG_KEY)
}

pub fn update_config(config: &HackaruConfig) {
    crate::config::update_config(CONFIG_KEY, config);
}
