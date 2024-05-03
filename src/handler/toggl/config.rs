use serde::{Deserialize, Serialize};

use crate::config::Config;

const CONFIG_KEY: &str = "toggl";

#[derive(Serialize)]
pub struct Context {
    pub workspace_id: u64,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct TogglConfig {
    pub base_url: String,
    pub time_entries_uri: String,
    pub email: String,
    pub password: String,
    pub project_id: u64,
    pub workspace_id: u64,
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
        }
    }
}
impl<'de> Config<'de> for TogglConfig {}

pub fn create_config() -> TogglConfig {
    crate::config::get_config::<TogglConfig>(CONFIG_KEY)
}

pub fn update_config(config: &TogglConfig) {
    crate::config::update_config(CONFIG_KEY, config);
}
