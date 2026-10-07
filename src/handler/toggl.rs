use anyhow::{Context as _, Result};
use async_trait::async_trait;
use chrono::{DateTime, Local, SecondsFormat};
use log::debug;
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use rpassword::prompt_password;
use simplelog::info;
use tinytemplate::TinyTemplate;

use crate::handler::toggl::config::Context;
use crate::tracker::side_project::prompt_side_projects;
use crate::{
    handler::toggl::config::update_config,
    tracker::config::{Handler, Side},
};

use self::config::{create_config, TogglConfig};

pub mod config;

#[derive(Debug, Default)]
pub struct Toggl {
    client: Client,
    config: TogglConfig,
}
impl Toggl {
    fn get_time_entries_uri(&self) -> Result<String> {
        let mut tt = TinyTemplate::new();
        tt.add_template("url", self.config.time_entries_uri.trim_matches('/'))
            .context("invalid Toggl time-entry URL template")?;
        let context = Context {
            workspace_id: self.config.workspace_id,
        };
        tt.render("url", &context)
            .context("failed to render Toggl time-entry URL")
    }
}

#[async_trait]
impl Handler for Toggl {
    async fn handle(
        &self,
        side: &Side,
        duration: &(DateTime<Local>, DateTime<Local>),
    ) -> Result<()> {
        let body = format!(
            r#"{{
            "created_with": "timeular_reader",
            "project_id": {project_id},
            "start": "{start}",
            "stop": "{stop}",
            "workspace_id": {workspace_id},
            "description": "{label}"
        }}"#,
            project_id = self.config.project_id_for_side(side.side_num),
            start = duration.0.to_rfc3339_opts(SecondsFormat::Secs, true),
            stop = duration.1.to_rfc3339_opts(SecondsFormat::Secs, true),
            workspace_id = self.config.workspace_id,
            label = side.label
        );

        let time_entries_url = self.get_time_entries_uri()?;

        let request_builder = self
            .client
            .post(format!(
                "{}/{}",
                self.config.base_url.trim_end_matches('/'),
                time_entries_url,
            ))
            .basic_auth(&self.config.email, Some(&self.config.password))
            .header(CONTENT_TYPE, "application/json")
            .body(body);

        debug!(
            "Sending time entry to Toggl workspace {}",
            self.config.workspace_id
        );

        let res = request_builder
            .send()
            .await
            .context("failed to send time entry to Toggl")?
            .error_for_status()
            .context("Toggl rejected the time entry")?;

        debug!(
            "Toggl response: {}",
            res.text()
                .await
                .context("failed to read Toggl response body")?
        );
        Ok(())
    }
}

pub async fn create_handler(setup: bool, sides: &[Side]) -> Result<Toggl> {
    let mut config = create_config()?;
    let client = Client::builder()
        .build()
        .context("failed to create Toggl HTTP client")?;
    update_vendor_config(&mut config, setup, sides)?;

    Ok(Toggl { client, config })
}

fn update_vendor_config(config: &mut TogglConfig, setup: bool, sides: &[Side]) -> Result<()> {
    if setup || config.workspace_id == 0 {
        let mut workspace_id = String::new();
        let mut message = String::from("Provide your Toggl workspace id");
        if config.workspace_id != 0 {
            message.push_str(
                format!(
                    "\ncurrent value {}, leave blank to skip",
                    config.workspace_id
                )
                .as_str(),
            );
        }
        info!("{message}");

        std::io::stdin()
            .read_line(&mut workspace_id)
            .context("failed to read Toggl workspace ID")?;
        workspace_id = workspace_id.trim().to_string();

        if !workspace_id.is_empty() {
            config.workspace_id = workspace_id
                .parse::<u64>()
                .context("Toggl workspace ID must be a number")?;
            update_config(config)?;
        }
    }

    if setup || config.project_id == 0 {
        let mut project_id = String::new();
        let mut message = String::from("Provide your Toggl project id");
        if config.project_id != 0 {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.project_id).as_str(),
            );
        }
        log::info!("{message}");

        std::io::stdin()
            .read_line(&mut project_id)
            .context("failed to read Toggl project ID")?;

        project_id = project_id.trim().to_string();

        if !project_id.is_empty() {
            config.project_id = project_id
                .parse::<u64>()
                .context("Toggl project ID must be a number")?;
            update_config(config)?;
        }
    }

    if setup && prompt_side_projects("Toggl", sides, &mut config.side_projects)? {
        update_config(config)?;
    }

    if setup || config.email.is_empty() {
        let mut email = String::new();
        let mut message = String::from("Provide your Toggl email");
        if !config.email.is_empty() {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.email).as_str(),
            );
        }
        log::info!("{message}");

        std::io::stdin()
            .read_line(&mut email)
            .context("failed to read Toggl email")?;

        email = email.trim().to_string();

        if !email.is_empty() {
            config.email = email;
            update_config(config)?;
        }
    }

    if setup || config.password.is_empty() {
        let mut message = String::from("Provide your Toggl password");
        if !config.password.is_empty() {
            message.push_str("\nleave blank to use current value");
        }
        let password = prompt_password(message)
            .context("failed to read Toggl password")?
            .trim()
            .to_string();

        if !password.is_empty() {
            config.password = password;
            update_config(config)?;
        }
    }
    Ok(())
}
