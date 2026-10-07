use crate::config::Config;
use crate::tracker::side_project::{project_for_side, SideProject};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExampleConfig {
    pub base_url: String,
    pub api_key: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub side_projects: Vec<SideProject<String>>,
}

impl Default for ExampleConfig {
    fn default() -> Self {
        ExampleConfig {
            base_url: String::from("https://api.example.com"),
            api_key: String::new(),
            project_id: String::new(),
            side_projects: Vec::new(),
        }
    }
}

impl<'de> Config<'de> for ExampleConfig {}

impl ExampleConfig {
    pub fn project_id_for_side(&self, side_num: u8) -> &str {
        project_for_side(&self.side_projects, side_num, &self.project_id).as_str()
    }
}

const CONFIG_KEY: &str = "example";

pub fn create_config() -> Result<ExampleConfig> {
    crate::config::get_config::<ExampleConfig>(CONFIG_KEY)
}

pub fn update_config(config: &ExampleConfig) -> Result<()> {
    crate::config::update_config(CONFIG_KEY, config)
}
