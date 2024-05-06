use async_trait::async_trait;
use chrono::{DateTime, Local, SecondsFormat};
use log::debug;
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use rpassword::prompt_password;
use simplelog::info;
use std::collections::HashMap;
use tinytemplate::TinyTemplate;

use crate::{
    handler::clockify::config::update_config,
    tracker::config::{Handler, Side},
};

use self::config::{create_config, ClockifyConfig};

pub mod config;

#[derive(Debug, Default)]
pub struct Clockify {
    client: Client,
    config: ClockifyConfig,
}

impl Clockify {
    fn get_time_entries_uri(&self) -> String {
        let mut tt = TinyTemplate::new();
        tt.add_template("url", self.config.time_entries_uri.trim_matches('/'))
            .unwrap();
        let mut context = HashMap::new();
        context.insert("workspace_id", &self.config.workspace_id);

        return tt.render("url", &context).unwrap();
    }
}

#[async_trait]
impl Handler for Clockify {
    async fn handle(&self, side: &Side, duration: &(DateTime<Local>, DateTime<Local>)) {
        let body = format!(
            r#"{{
            "projectId": "{project_id}",
            "start": "{start}",
            "end": "{end}",
            "description": "{label}"
        }}"#,
            project_id = self.config.project_id,
            start = duration.0.to_rfc3339_opts(SecondsFormat::Secs, true),
            end = duration.1.to_rfc3339_opts(SecondsFormat::Secs, true),
            label = side.label
        );

        let time_entries_url = self.get_time_entries_uri();

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

        debug!(
            "request {}\nheaders {:?}",
            String::from_utf8(
                request_builder
                    .try_clone()
                    .unwrap()
                    .build()
                    .unwrap()
                    .body()
                    .unwrap()
                    .as_bytes()
                    .unwrap()
                    .to_vec()
            )
            .unwrap(),
            request_builder
                .try_clone()
                .unwrap()
                .build()
                .unwrap()
                .headers()
        );

        let res = request_builder.send().await.unwrap();

        debug!("Response: {}", res.text().await.unwrap());
    }
}

pub async fn create_handler(setup: bool) -> Clockify {
    let mut config = create_config();
    let client = Client::builder().build().unwrap();
    update_vendor_config(&mut config, setup);

    return Clockify { client, config };
}

fn update_vendor_config(config: &mut ClockifyConfig, setup: bool) {
    if setup || config.workspace_id.is_empty() {
        let mut workspace_id = String::new();
        let mut message =
            String::from_utf8("Provide your Clockify workspace id".as_bytes().to_vec()).unwrap();
        if !config.workspace_id.is_empty() {
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
            .expect("Please provide workspace_id");
        workspace_id = workspace_id.trim().to_string();

        if !workspace_id.is_empty() {
            config.workspace_id = workspace_id;
            update_config(&config);
        }
    }

    if setup || config.project_id.is_empty() {
        let mut project_id = String::new();
        let mut message =
            String::from_utf8("Provide your Clockify project id".as_bytes().to_vec()).unwrap();
        if !config.project_id.is_empty() {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.project_id).as_str(),
            );
        }
        log::info!("{message}");

        std::io::stdin()
            .read_line(&mut project_id)
            .expect("Please provide project_id");

        project_id = project_id.trim().to_string();

        if !project_id.is_empty() {
            config.project_id = project_id;
            update_config(&config);
        }
    }

    if setup || config.api_key.is_empty() {
        let mut message =
            String::from_utf8("Provide your Clockify Api Key".as_bytes().to_vec()).unwrap();
        if !config.api_key.is_empty() {
            message.push_str("\nleave blank to use current value");
        }
        let api_key: String = (*prompt_password(message).unwrap().trim()).to_string();

        if !api_key.is_empty() {
            config.api_key = api_key;
            update_config(&config);
        }
    }
}
