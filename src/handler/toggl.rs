use async_trait::async_trait;
use chrono::{DateTime, Local, SecondsFormat};
use log::debug;
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use rpassword::prompt_password;
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
    auth: (String, String),
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
        let auth = self.auth.clone();
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
            .basic_auth(auth.0, Some(auth.1))
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

        debug!("res {}", res.text().await.unwrap());
    }
}

pub async fn create_handler() -> Toggl {
    let mut config = create_config();
    let client = Client::builder().build().unwrap();
    update_vendor_config(&mut config);

    let auth = get_auth(&mut config).await;
    if auth.0 != config.email {
        config.email = auth.0.clone();
        update_config(&config);
        debug!("config updated");
    }

    return Toggl {
        client,
        config,
        auth,
    };
}

fn update_vendor_config(config: &mut TogglConfig) {
    if config.workspace_id == 0 {
        let mut workspace_id = String::new();
        println!("Provider your toggl workspace_id");

        std::io::stdin()
            .read_line(&mut workspace_id)
            .expect("Please provide workspace_id");

        config.workspace_id = workspace_id.parse::<u64>().unwrap();
        update_config(&config);
    }

    if config.project_id == 0 {
        let mut project_id = String::new();
        println!("Provider your toggl project_id");

        std::io::stdin()
            .read_line(&mut project_id)
            .expect("Please provide project_id");

        config.project_id = project_id.parse::<u64>().unwrap();
        update_config(&config);
    }
}

async fn get_auth(config: &mut TogglConfig) -> (String, String) {
    if config.email.is_empty() {
        let mut email = String::new();
        println!("Type your toggl email");

        std::io::stdin()
            .read_line(&mut email)
            .expect("Please provide email");

        config.email = email;
        update_config(&config);
    }

    if config.password.is_empty() {
        let password: String = (*prompt_password("Type your toggl password: ")
            .unwrap()
            .trim())
        .to_string();

        config.password = password;
        update_config(&config);
    }

    return (config.email.clone(), config.password.clone());
}
