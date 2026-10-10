use anyhow::{Context as _, Result};
use async_trait::async_trait;
use chrono::SecondsFormat;
use log::debug;
use reqwest::Client;
use serde::Serialize;
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

#[derive(Serialize)]
struct TimeEntryRequest<'a> {
    #[serde(rename = "projectId", skip_serializing_if = "Option::is_none")]
    project_id: Option<&'a str>,
    start: String,
    end: String,
    description: &'a str,
}

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
        let project_id = self.config.project_id_for_side(entry.side.side_num);
        let body = TimeEntryRequest {
            project_id: (!project_id.is_empty()).then_some(project_id),
            start: entry.start.to_rfc3339_opts(SecondsFormat::Secs, true),
            end: entry.end.to_rfc3339_opts(SecondsFormat::Secs, true),
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
            .header("x-api-key", &self.config.api_key)
            .json(&body);

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
    update_vendor_config(&mut config, setup, sides).await?;

    Ok(Clockify { client, config })
}

async fn update_vendor_config(
    config: &mut ClockifyConfig,
    setup: bool,
    sides: &[Side],
) -> Result<()> {
    if setup || config.workspace_id.is_empty() {
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

        let workspace_id = crate::prompt::read_line()
            .await
            .context("failed to read Clockify workspace ID")?;
        let workspace_id = workspace_id.trim().to_string();

        if !workspace_id.is_empty() {
            config.workspace_id = workspace_id;
            update_config(config)?;
        }
    }

    if setup || config.project_id.is_empty() {
        let mut message = String::from("Provide your Clockify project id");
        if !config.project_id.is_empty() {
            message.push_str(
                format!("\nCurrently \"{}\", leave blank to skip", config.project_id).as_str(),
            );
        }
        log::info!("{message}");

        let project_id = crate::prompt::read_line()
            .await
            .context("failed to read Clockify project ID")?;

        let project_id = project_id.trim().to_string();

        if !project_id.is_empty() {
            config.project_id = project_id;
            update_config(config)?;
        }
    }

    if setup && prompt_side_projects("Clockify", sides, &mut config.side_projects).await? {
        update_config(config)?;
    }

    if setup || config.api_key.is_empty() {
        let mut message = String::from("Provide your Clockify API key");
        if !config.api_key.is_empty() {
            message.push_str("\nleave blank to use current value");
        }
        let api_key = crate::prompt::read_password(message)
            .await
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

#[cfg(test)]
mod tests {
    use super::{config::ClockifyConfig, Clockify};
    use crate::{
        test_support::{request_body, time_entry, MockServer},
        tracker::config::Handler,
    };
    use reqwest::Client;
    use serde_json::json;

    #[tokio::test]
    async fn posts_escaped_description_and_omits_an_empty_project_id() {
        let server = MockServer::start(1, |_, _| (201, "{}".into()));
        let config = ClockifyConfig {
            base_url: server.url().to_string(),
            time_entries_uri: "/workspaces/{workspace_id}/time-entries".into(),
            workspace_id: "workspace-123".into(),
            api_key: "secret".into(),
            ..ClockifyConfig::default()
        };

        let entry = time_entry(1, "Quote: \" slash: \\ newline:\n");
        let handler = Clockify {
            client: Client::new(),
            config,
        };
        handler.handle(&entry).await.unwrap();

        let requests = server.finish();
        let request = &requests[0];
        assert!(request.starts_with("POST /workspaces/workspace-123/time-entries HTTP/1.1"));
        assert!(request.to_ascii_lowercase().contains("x-api-key: secret"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(request_body(request)).unwrap(),
            json!({
                "start": entry.start.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "end": entry.end.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "description": entry.side.label
            })
        );
    }

    #[tokio::test]
    async fn reports_http_rejection() {
        let server = MockServer::start(1, |_, _| (500, "{}".into()));
        let config = ClockifyConfig {
            base_url: server.url().to_string(),
            ..ClockifyConfig::default()
        };

        let handler = Clockify {
            client: Client::new(),
            config,
        };
        let error = handler.handle(&time_entry(1, "Work")).await.unwrap_err();

        assert!(format!("{error:#}").contains("Clockify rejected the time entry"));
        server.finish();
    }
}
