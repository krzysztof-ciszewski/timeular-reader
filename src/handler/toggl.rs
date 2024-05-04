use async_trait::async_trait;
use chrono::{DateTime, Local, SecondsFormat};
use log::debug;
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use rpassword::prompt_password;
use simplelog::info;
use tinytemplate::TinyTemplate;

use crate::handler::toggl::config::Context;
use crate::{
    handler::toggl::config::update_config,
    tracker::config::{Handler, Side},
};

use self::config::{create_config, TogglConfig};

pub mod config;

pub struct Toggl {
    client: Client,
    config: TogglConfig,
}
impl Toggl {
    fn get_time_entries_uri(&self) -> String {
        let mut tt = TinyTemplate::new();
        tt.add_template("url", self.config.time_entries_uri.trim_matches('/'))
            .unwrap();
        let context = Context {
            workspace_id: self.config.workspace_id,
        };
        let time_entries_url = tt.render("url", &context).unwrap();

        return time_entries_url;
    }
}

#[async_trait]
impl Handler for Toggl {
    async fn handle(&self, side: &Side, duration: &(DateTime<Local>, DateTime<Local>)) {
        let body = format!(
            r#"{{
            "created_with": "timeular_reader",
            "project_id": {project_id},
            "start": "{start}",
            "stop": "{stop}",
            "workspace_id": {workspace_id},
            "description": "{label}"
        }}"#,
            project_id = self.config.project_id,
            start = duration.0.to_rfc3339_opts(SecondsFormat::Secs, true),
            stop = duration.1.to_rfc3339_opts(SecondsFormat::Secs, true),
            workspace_id = self.config.workspace_id,
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
            .basic_auth(&self.config.email, Some(&self.config.password))
            .header(CONTENT_TYPE, "application/json")
            .body(body);

        debug!(
            "request {}",
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
            .unwrap()
        );

        let res = request_builder.send().await.unwrap();

        debug!("Response: {}", res.text().await.unwrap());
    }
}

pub async fn create_handler(setup: bool) -> Toggl {
    let mut config = create_config();
    let client = Client::builder().build().unwrap();
    update_vendor_config(&mut config, setup);

    return Toggl { client, config };
}

fn update_vendor_config(config: &mut TogglConfig, setup: bool) {
    if setup || config.workspace_id == 0 {
        let mut workspace_id = String::new();
        let mut message =
            String::from_utf8("Provide your Toggl workspace id".as_bytes().to_vec()).unwrap();
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
            .expect("Please provide workspace_id");
        workspace_id = workspace_id.trim().to_string();

        if !workspace_id.is_empty() {
            config.workspace_id = workspace_id.parse::<u64>().unwrap();
            update_config(&config);
        }
    }

    if setup || config.project_id == 0 {
        let mut project_id = String::new();
        let mut message =
            String::from_utf8("Provide your Toggl project id".as_bytes().to_vec()).unwrap();
        if config.project_id != 0 {
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
            config.project_id = project_id.parse::<u64>().unwrap();
            update_config(&config);
        }
    }

    if setup || config.email.is_empty() {
        let mut email = String::new();
        let mut message =
            String::from_utf8("Provide your Toggl email".as_bytes().to_vec()).unwrap();
        if !config.email.is_empty() {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.email).as_str(),
            );
        }
        log::info!("{message}");

        std::io::stdin()
            .read_line(&mut email)
            .expect("Please provide email");

        email = email.trim().to_string();

        if !email.is_empty() {
            config.email = email;
            update_config(&config);
        }
    }

    if setup || config.password.is_empty() {
        let mut message =
            String::from_utf8("Provide your Toggl password".as_bytes().to_vec()).unwrap();
        if !config.password.is_empty() {
            message.push_str("\nleave blank to use current value");
        }
        let password: String = (*prompt_password(message).unwrap().trim()).to_string();

        if !password.is_empty() {
            config.password = password;
            update_config(&config);
        }
    }
}
