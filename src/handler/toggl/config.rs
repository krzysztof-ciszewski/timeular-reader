use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::tracker::side_project::{project_for_side, SideProject};

const CONFIG_KEY: &str = "toggl";

#[derive(Serialize)]
pub struct Context {
    pub workspace_id: u64,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TogglConfig {
    pub base_url: String,
    pub time_entries_uri: String,
    pub email: String,
    pub password: String,
    pub project_id: u64,
    pub workspace_id: u64,
    #[serde(default)]
    pub side_projects: Vec<SideProject<u64>>,
}

impl TogglConfig {
    pub fn project_id_for_side(&self, side_num: u8) -> u64 {
        *project_for_side(&self.side_projects, side_num, &self.project_id)
    }
}

impl Default for TogglConfig {
    fn default() -> Self {
        TogglConfig {
            base_url: String::from("https://api.track.toggl.com"),
            time_entries_uri: String::from("api/v9/workspaces/{workspace_id}/time_entries"),
            email: String::new(),
            password: String::new(),
            project_id: 0,
            workspace_id: 0,
            side_projects: Vec::new(),
        }
    }
}
impl<'de> Config<'de> for TogglConfig {}

pub fn create_config() -> Result<TogglConfig> {
    crate::config::get_config::<TogglConfig>(CONFIG_KEY)
}

pub fn update_config(config: &TogglConfig) -> Result<()> {
    crate::config::update_config(CONFIG_KEY, config)
}
