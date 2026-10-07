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

#[cfg(test)]
mod tests {
    use super::HackaruConfig;
    use crate::tracker::side_project::SideProject;

    #[test]
    fn cookie_store_defaults_empty_and_rejects_invalid_saved_data() {
        let config = HackaruConfig::default();
        assert_eq!(
            config.get_cookie_store().unwrap().iter_unexpired().count(),
            0
        );

        let invalid = HackaruConfig {
            cookies: "not json".into(),
            ..config
        };
        assert!(invalid
            .get_cookie_store()
            .unwrap_err()
            .to_string()
            .contains("failed to parse saved Hackaru cookies"));
    }

    #[test]
    fn project_id_for_side_uses_default_when_unassigned() {
        let config = HackaruConfig {
            project_id: 5,
            side_projects: vec![SideProject {
                side_num: 2,
                project_id: 10,
            }],
            ..HackaruConfig::default()
        };

        assert_eq!(config.project_id_for_side(1), 5);
        assert_eq!(config.project_id_for_side(2), 10);
    }
}
