use anyhow::{Context as _, Result};
use async_trait::async_trait;
use chrono::SecondsFormat;
use log::debug;
use reqwest::Client;
use serde::Serialize;
use simplelog::info;
use tinytemplate::TinyTemplate;

use crate::handler::toggl::config::Context;
use crate::tracker::side_project::prompt_side_projects;
use crate::{
    handler::toggl::config::update_config,
    tracker::config::{Handler, Side, TimeEntry},
};

use self::config::{create_config, TogglConfig};

pub mod config;

#[derive(Serialize)]
struct TimeEntryRequest<'a> {
    created_with: &'static str,
    project_id: u64,
    start: String,
    stop: String,
    workspace_id: u64,
    description: &'a str,
}

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
    async fn handle(&self, entry: &TimeEntry) -> Result<()> {
        let body = TimeEntryRequest {
            created_with: "timeular_reader",
            project_id: self.config.project_id_for_side(entry.side.side_num),
            start: entry.start.to_rfc3339_opts(SecondsFormat::Secs, true),
            stop: entry.end.to_rfc3339_opts(SecondsFormat::Secs, true),
            workspace_id: self.config.workspace_id,
            description: &entry.side.label,
        };

        let time_entries_url = self.get_time_entries_uri()?;

        let request_builder = self
            .client
            .post(format!(
                "{}/{}",
                self.config.base_url.trim_end_matches('/'),
                time_entries_url,
            ))
            .basic_auth(&self.config.email, Some(&self.config.password))
            .json(&body);

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
    update_vendor_config(&mut config, setup, sides).await?;

    Ok(Toggl { client, config })
}

async fn update_vendor_config(config: &mut TogglConfig, setup: bool, sides: &[Side]) -> Result<()> {
    if setup || config.workspace_id == 0 {
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

        let workspace_id = crate::prompt::read_line()
            .await
            .context("failed to read Toggl workspace ID")?;
        let workspace_id = workspace_id.trim().to_string();

        if !workspace_id.is_empty() {
            config.workspace_id = workspace_id
                .parse::<u64>()
                .context("Toggl workspace ID must be a number")?;
            update_config(config)?;
        }
    }

    if setup || config.project_id == 0 {
        let mut message = String::from("Provide your Toggl project id");
        if config.project_id != 0 {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.project_id).as_str(),
            );
        }
        log::info!("{message}");

        let project_id = crate::prompt::read_line()
            .await
            .context("failed to read Toggl project ID")?;

        let project_id = project_id.trim().to_string();

        if !project_id.is_empty() {
            config.project_id = project_id
                .parse::<u64>()
                .context("Toggl project ID must be a number")?;
            update_config(config)?;
        }
    }

    if setup && prompt_side_projects("Toggl", sides, &mut config.side_projects).await? {
        update_config(config)?;
    }

    if setup || config.email.is_empty() {
        let mut message = String::from("Provide your Toggl email");
        if !config.email.is_empty() {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.email).as_str(),
            );
        }
        log::info!("{message}");

        let email = crate::prompt::read_line()
            .await
            .context("failed to read Toggl email")?;

        let email = email.trim().to_string();

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
        let password = crate::prompt::read_password(message)
            .await
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

#[cfg(test)]
mod tests {
    use super::{config::TogglConfig, Toggl};
    use crate::{
        test_support::{request_body, time_entry, MockServer},
        tracker::config::Handler,
    };
    use reqwest::Client;
    use serde_json::json;

    #[tokio::test]
    async fn posts_a_valid_json_entry_with_side_project_and_basic_auth() {
        let server = MockServer::start(1, |_, _| (200, "{}".into()));
        let config = TogglConfig {
            base_url: server.url().to_string(),
            time_entries_uri: "/workspaces/{workspace_id}/time_entries".into(),
            workspace_id: 123,
            project_id: 1,
            email: "user".into(),
            password: "pass".into(),
            side_projects: vec![crate::tracker::side_project::SideProject {
                side_num: 2,
                project_id: 99,
            }],
        };

        let entry = time_entry(2, "Quote: \" slash: \\ newline:\n");
        let handler = Toggl {
            client: Client::new(),
            config,
        };
        handler.handle(&entry).await.unwrap();

        let requests = server.finish();
        let request = &requests[0];
        assert!(request.starts_with("POST /workspaces/123/time_entries HTTP/1.1"));
        assert!(request.contains("authorization: Basic dXNlcjpwYXNz"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(request_body(request)).unwrap(),
            json!({
                "created_with": "timeular_reader",
                "project_id": 99,
                "start": entry.start.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "stop": entry.end.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "workspace_id": 123,
                "description": entry.side.label
            })
        );
    }

    #[tokio::test]
    async fn reports_http_rejection() {
        let server = MockServer::start(1, |_, _| (500, "{}".into()));
        let config = TogglConfig {
            base_url: server.url().to_string(),
            workspace_id: 1,
            ..TogglConfig::default()
        };

        let handler = Toggl {
            client: Client::new(),
            config,
        };
        let error = handler.handle(&time_entry(1, "Work")).await.unwrap_err();

        assert!(format!("{error:#}").contains("Toggl rejected the time entry"));
        server.finish();
    }
}
