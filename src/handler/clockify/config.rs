use anyhow::Result;
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

pub fn create_config() -> Result<ClockifyConfig> {
    crate::config::get_config::<ClockifyConfig>(CONFIG_KEY)
}

pub fn update_config(config: &ClockifyConfig) -> Result<()> {
    crate::config::update_config(CONFIG_KEY, config)
}

#[cfg(test)]
mod tests {
    use super::ClockifyConfig;
    use crate::tracker::side_project::SideProject;

    #[test]
    fn defaults_and_side_projects_preserve_string_ids() {
        let mut config = ClockifyConfig::default();
        assert_eq!(config.base_url, "https://app.clockify.me");
        assert_eq!(config.project_id_for_side(1), "");

        config.project_id = "default".into();
        config.side_projects.push(SideProject {
            side_num: 2,
            project_id: "side-project".into(),
        });
        assert_eq!(config.project_id_for_side(1), "default");
        assert_eq!(config.project_id_for_side(2), "side-project");
    }

    #[test]
    fn legacy_config_defaults_missing_side_projects() {
        let config: ClockifyConfig = toml::from_str(
            r#"
base_url = "https://example.test"
time_entries_uri = "entries"
api_key = ""
project_id = "default"
workspace_id = "workspace"
"#,
        )
        .unwrap();

        assert!(config.side_projects.is_empty());
        assert_eq!(config.project_id_for_side(1), "default");
    }
}
