use anyhow::{Context as _, Result};
use async_trait::async_trait;
use chrono::SecondsFormat;
use log::debug;
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use rpassword::prompt_password;
use simplelog::info;
use std::collections::HashMap;
use tinytemplate::TinyTemplate;

use crate::{
    handler::clockify::config::update_config,
    tracker::config::{Handler, Side, TimeEntry},
    tracker::side_project::prompt_side_projects,
};

use self::config::{create_config, ClockifyConfig};

pub mod config;

#[derive(Debug, Default)]
pub struct Clockify {
    client: Client,
    config: ClockifyConfig,
}

impl Clockify {
    fn get_time_entries_uri(&self) -> Result<String> {
        let mut tt = TinyTemplate::new();
        tt.add_template("url", self.config.time_entries_uri.trim_matches('/'))
            .context("invalid Clockify time-entry URL template")?;
        let mut context = HashMap::new();
        context.insert("workspace_id", &self.config.workspace_id);

        tt.render("url", &context)
            .context("failed to render Clockify time-entry URL")
    }
}

#[async_trait]
impl Handler for Clockify {
    async fn handle(&self, entry: &TimeEntry) -> Result<()> {
        let body = format!(
            r#"{{
            "projectId": "{project_id}",
            "start": "{start}",
            "end": "{end}",
            "description": "{label}"
        }}"#,
            project_id = self.config.project_id_for_side(entry.side.side_num),
            start = entry.start.to_rfc3339_opts(SecondsFormat::Secs, true),
            end = entry.end.to_rfc3339_opts(SecondsFormat::Secs, true),
            label = entry.side.label
        );

        let time_entries_url = self.get_time_entries_uri()?;

        let request_builder = self
            .client
            .post(format!(
                "{}/{}",
                self.config.base_url.trim_end_matches('/'),
                time_entries_url,
            ))
            .header(CONTENT_TYPE, "application/json")
            .header("x-api-key", &self.config.api_key)
            .body(body);

        let res = request_builder
            .send()
            .await
            .context("failed to send time entry to Clockify")?
            .error_for_status()
            .context("Clockify rejected the time entry")?;

        debug!(
            "Clockify response: {}",
            res.text()
                .await
                .context("failed to read Clockify response body")?
        );
        Ok(())
    }
}

pub async fn create_handler(setup: bool, sides: &[Side]) -> Result<Clockify> {
    let mut config = create_config()?;
    let client = Client::builder()
        .build()
        .context("failed to create Clockify HTTP client")?;
    update_vendor_config(&mut config, setup, sides)?;

    Ok(Clockify { client, config })
}

fn update_vendor_config(config: &mut ClockifyConfig, setup: bool, sides: &[Side]) -> Result<()> {
    if setup || config.workspace_id.is_empty() {
        let mut workspace_id = String::new();
        let mut message = String::from("Provide your Clockify workspace id");
        if !config.workspace_id.is_empty() {
            message.push_str(
                format!(
                    "\nCurrently \"{}\", leave blank to skip",
                    config.workspace_id
                )
                .as_str(),
            );
        }
        info!("{message}");

        std::io::stdin()
            .read_line(&mut workspace_id)
            .context("failed to read Clockify workspace ID")?;
        workspace_id = workspace_id.trim().to_string();

        if !workspace_id.is_empty() {
            config.workspace_id = workspace_id;
            update_config(config)?;
        }
    }

    if setup || config.project_id.is_empty() {
        let mut project_id = String::new();
        let mut message = String::from("Provide your Clockify project id");
        if !config.project_id.is_empty() {
            message.push_str(
                format!("\nCurrently \"{}\", leave blank to skip", config.project_id).as_str(),
            );
        }
        log::info!("{message}");

        std::io::stdin()
            .read_line(&mut project_id)
            .context("failed to read Clockify project ID")?;

        project_id = project_id.trim().to_string();

        if !project_id.is_empty() {
            config.project_id = project_id;
            update_config(config)?;
        }
    }

    if setup && prompt_side_projects("Clockify", sides, &mut config.side_projects)? {
        update_config(config)?;
    }

    if setup || config.api_key.is_empty() {
        let mut message = String::from("Provide your Clockify API key");
        if !config.api_key.is_empty() {
            message.push_str("\nleave blank to use current value");
        }
        let api_key = prompt_password(message)
            .context("failed to read Clockify API key")?
            .trim()
            .to_string();

        if !api_key.is_empty() {
            config.api_key = api_key;
            update_config(config)?;
        }
    }
    Ok(())
}
