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

#[cfg(test)]
mod tests {
    use super::ExampleConfig;
    use crate::tracker::side_project::SideProject;

    #[test]
    fn defaults_and_side_projects_are_supported() {
        let mut config = ExampleConfig::default();
        assert_eq!(config.base_url, "https://api.example.com");
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
    fn legacy_config_defaults_new_optional_fields() {
        let config: ExampleConfig = toml::from_str(
            r#"
base_url = "https://example.test"
api_key = ""
"#,
        )
        .unwrap();

        assert_eq!(config.project_id_for_side(1), "");
        assert!(config.side_projects.is_empty());
    }
}
