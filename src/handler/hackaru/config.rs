use anyhow::Result;
use reqwest_cookie_store::CookieStore;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::tracker::side_project::{project_for_side, SideProject};

const CONFIG_KEY: &str = "hackaru";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HackaruConfig {
    pub hackaru_url: String,
    pub activities_rel_url: String,
    pub email: String,
    pub project_id: u64,
    pub cookies: String,
    pub password: String,
    #[serde(default)]
    pub side_projects: Vec<SideProject<u64>>,
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
            side_projects: Vec::new(),
        }
    }
}
impl<'de> Config<'de> for HackaruConfig {}

impl HackaruConfig {
    pub fn project_id_for_side(&self, side_num: u8) -> u64 {
        *project_for_side(&self.side_projects, side_num, &self.project_id)
    }

    pub fn get_cookie_store(&self) -> Result<CookieStore> {
        let cookies_str = self.cookies.as_str();
        if cookies_str.is_empty() {
            return Ok(CookieStore::default());
        }

        let mut buf: &[u8] = cookies_str.as_bytes();
        CookieStore::load_json(&mut buf)
            .map_err(|error| anyhow::anyhow!("failed to parse saved Hackaru cookies: {error}"))
    }
}

pub fn create_config() -> Result<HackaruConfig> {
    crate::config::get_config::<HackaruConfig>(CONFIG_KEY)
}

pub fn update_config(config: &HackaruConfig) -> Result<()> {
    crate::config::update_config(CONFIG_KEY, config)
}
