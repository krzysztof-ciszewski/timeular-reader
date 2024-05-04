pub mod config;
pub mod http_data;

use async_trait::async_trait;
use chrono::{DateTime, Local};
use http_data::*;
use log::{debug, info};
use reqwest::Client;
use reqwest_cookie_store::CookieStoreMutex;
use rpassword::prompt_password;
use std::string::String;
use std::sync::Arc;

use crate::{
    handler::hackaru::config::update_config,
    tracker::config::{Handler, Side},
};

use self::config::{create_config, HackaruConfig};
pub struct Hackaru {
    client: Client,
    config: HackaruConfig,
}

#[async_trait]
impl Handler for Hackaru {
    async fn handle(&self, side: &Side, duration: &(DateTime<Local>, DateTime<Local>)) {
        let activity_start = ActivityStartRequest {
            activity: ActivityStartData {
                description: side.label.clone(),
                project_id: self.config.project_id,
                started_at: duration.0.to_rfc3339(),
            },
        };

        let response = self
            .client
            .post(format!(
                "{}/{}",
                self.config.hackaru_url.trim_end_matches('/'),
                self.config.activities_rel_url.trim_matches('/')
            ))
            .header("x-requested-with", "XMLHttpRequest")
            .json(&activity_start)
            .send()
            .await
            .unwrap()
            .json::<ActivityResponse>()
            .await
            .unwrap();

        let activity_end = ActivityEndRequest {
            activity: ActivityEndData {
                id: response.id,
                stopped_at: duration.1.to_rfc3339(),
            },
        };

        self.client
            .put(format!(
                "{}/{}/{}",
                self.config.hackaru_url.trim_end_matches('/'),
                self.config.activities_rel_url.trim_matches('/'),
                response.id
            ))
            .header("x-requested-with", "XMLHttpRequest")
            .json(&activity_end)
            .send()
            .await
            .expect("error");
    }
}

pub async fn create_handler(setup: bool) -> Hackaru {
    let mut config = create_config();
    let cookie_store = create_cookie_store(&config);
    let client = create_client(&cookie_store);
    setup_vendor_config(setup, &mut config).await;

    if !has_cookies(&cookie_store) {
        auth(&client, &config).await;
        save_cookies(&cookie_store, &mut config);
        update_config(&config);
    }

    return Hackaru { client, config };
}

fn has_cookies(cookie_store: &Arc<CookieStoreMutex>) -> bool {
    let store = cookie_store.lock().unwrap();
    store.iter_unexpired().count() > 0
}

fn save_cookies(cookie_store: &Arc<CookieStoreMutex>, config: &mut HackaruConfig) {
    let cookie_store = cookie_store.lock().unwrap();

    let mut json = Vec::new();
    cookie_store.save_json(&mut json).unwrap();

    config.cookies = String::from_utf8(json).unwrap();
    update_config(&config);
}

fn create_client(cookie_store: &Arc<CookieStoreMutex>) -> Client {
    Client::builder()
        .cookie_store(true)
        .cookie_provider(Arc::clone(&cookie_store))
        .build()
        .unwrap()
}

async fn setup_vendor_config(setup: bool, config: &mut HackaruConfig) {
    if setup || config.hackaru_url.is_empty() {
        let mut hackaru_url = String::new();
        let mut message =
            String::from_utf8("Provide your hackaru url".as_bytes().to_vec()).unwrap();
        if config.project_id != 0 {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.project_id).as_str(),
            );
        }
        info!("{message}");

        std::io::stdin()
            .read_line(&mut hackaru_url)
            .expect("Please provide url");

        hackaru_url = hackaru_url.trim().to_string();

        if !hackaru_url.is_empty() {
            config.hackaru_url = hackaru_url;
            update_config(&config);
        }
    }

    if setup || config.project_id == 0 {
        let mut project_id = String::new();
        let mut message =
            String::from_utf8("Provide your hackaru project id".as_bytes().to_vec()).unwrap();
        if config.project_id != 0 {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.project_id).as_str(),
            );
        }
        info!("{message}");

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
            String::from_utf8("Provide your hackaru email".as_bytes().to_vec()).unwrap();
        if !config.email.is_empty() {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.email).as_str(),
            );
        }
        info!("{message}");

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
            String::from_utf8("Provide your hackaru password".as_bytes().to_vec()).unwrap();
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

async fn auth(client: &Client, config: &HackaruConfig) {
    let login = LoginRequest {
        user: UserRequest {
            email: config.email.clone(),
            password: config.password.clone(),
        },
    };

    let res = client
        .post(format!(
            "{}/auth/auth_tokens",
            config.hackaru_url.trim_end_matches('/')
        ))
        .json(&login)
        .header("Content-Type", "application/json")
        .header("X-Requested-With", "XMLHttpRequest")
        .send()
        .await
        .unwrap();

    debug!("{:?}", res.text().await);
}

fn create_cookie_store(config: &HackaruConfig) -> Arc<CookieStoreMutex> {
    let cookie_store = config.get_cookie_store();
    Arc::new(CookieStoreMutex::new(cookie_store))
}
