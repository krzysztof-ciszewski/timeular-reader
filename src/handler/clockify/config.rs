use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::tracker::side_project::{project_for_side, SideProject};

const CONFIG_KEY: &str = "clockify";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClockifyConfig {
    pub base_url: String,
    pub time_entries_uri: String,
    pub api_key: String,
    pub project_id: String,
    pub workspace_id: String,
    #[serde(default)]
    pub side_projects: Vec<SideProject<String>>,
}

impl ClockifyConfig {
    pub fn project_id_for_side(&self, side_num: u8) -> &str {
        project_for_side(&self.side_projects, side_num, &self.project_id).as_str()
    }
}

impl Default for ClockifyConfig {
    fn default() -> Self {
        ClockifyConfig {
            base_url: String::from("https://app.clockify.me"),
            time_entries_uri: String::from("/api/v1/workspaces/{workspace_id}/time-entries"),
            api_key: String::new(),
            project_id: String::new(),
            workspace_id: String::new(),
            side_projects: Vec::new(),
        }
    }
}
impl<'de> Config<'de> for ClockifyConfig {}

pub fn create_config() -> ClockifyConfig {
    crate::config::get_config::<ClockifyConfig>(CONFIG_KEY)
}

pub fn update_config(config: &ClockifyConfig) {
    crate::config::update_config(CONFIG_KEY, config);
}
