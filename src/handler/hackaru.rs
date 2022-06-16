use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Local};
use cookie_store::CookieStore;
use reqwest::Client;
use reqwest_cookie_store::CookieStoreMutex;
use rpassword::prompt_password;
use serde_derive::Deserialize;
use serde_derive::Serialize;

use crate::tracker::config::{Handler, Side};

pub struct Hackaru {
    client: reqwest::Client,
    url: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LoginRequest {
    user: UserRequest,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct UserRequest {
    email: String,
    password: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ActivityStart {
    activity: ActivityStartData,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ActivityStartData {
    description: String,
    project_id: u64,
    started_at: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ActivityEnd {
    activity: ActivityEndData,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ActivityEndData {
    id: u64,
    stopped_at: String,
}
#[derive(Deserialize)]
struct ActivityResponse {
    id: u64,
}

#[async_trait]
impl Handler for Hackaru {
    async fn handle(self: &Self, side: &Side, duration: &(DateTime<Local>, DateTime<Local>)) {
        let activity_start = ActivityStart {
            activity: ActivityStartData {
                description: side.label.clone(),
                project_id: 5867,
                started_at: duration.0.to_rfc3339(),
            },
        };

        let response = self
            .client
            .post(format!("{}/v1/activities", self.url))
            .header("x-requested-with", "XMLHttpRequest")
            .json(&activity_start)
            .send()
            .await
            .expect("error")
            .json::<ActivityResponse>()
            .await
            .unwrap();

        let activity_end = ActivityEnd {
            activity: ActivityEndData {
                id: response.id,
                stopped_at: duration.1.to_rfc3339(),
            },
        };

        self.client
            .post(format!("{}/v1/activities/{}", self.url, response.id))
            .header("x-requested-with", "XMLHttpRequest")
            .json(&activity_end)
            .send()
            .await
            .expect("error");
    }
}

pub async fn create_handler() -> Hackaru {
    let base_url = "https://api.hackaru.app";
    let cookie_store = create_cookie_store();
    let client = create_client(&cookie_store);

    if !has_cookies(&cookie_store) {
        auth_client(&client, base_url).await;

        save_cookies(&cookie_store);
    }

    Hackaru {
        client: client,
        url: String::from(base_url),
    }
}

fn has_cookies(cookie_store: &Arc<CookieStoreMutex>) -> bool {
    let store = cookie_store.lock().unwrap();
    store.iter_unexpired().count() > 0
}

fn save_cookies(cookie_store: &Arc<CookieStoreMutex>) {
    let store = cookie_store.lock().unwrap();

    let mut writer = std::fs::File::create("cookies.json")
        .map(std::io::BufWriter::new)
        .unwrap();

    store.save_json(&mut writer).unwrap();
}

fn create_client(cookie_store: &Arc<CookieStoreMutex>) -> Client {
    Client::builder()
        .cookie_store(true)
        .cookie_provider(Arc::clone(&cookie_store))
        .build()
        .unwrap()
}

async fn auth_client(client: &Client, base_url: &str) {
    println!("Type your hackaru email");

    let mut email = String::new();

    std::io::stdin()
        .read_line(&mut email)
        .expect("Please provide email");

    let email = email.as_str().trim();

    let password = prompt_password("Type your hackaru password: ").unwrap();
    let password = password.as_str().trim();

    let login = LoginRequest {
        user: UserRequest {
            email: String::from(email),
            password: String::from(password),
        },
    };

    client
        .post(format!("{}/auth/auth_tokens", base_url))
        .json(&login)
        .header("Content-Type", "application/json")
        .header("X-Requested-With", "XMLHttpRequest")
        .send()
        .await
        .unwrap();
}

fn create_cookie_store() -> Arc<CookieStoreMutex> {
    let cookie_store = {
        let file = File::open("cookies.json").map(BufReader::new).unwrap();
        CookieStore::load_json(file).unwrap()
    };
    let cookie_store = CookieStoreMutex::new(cookie_store);
    std::sync::Arc::new(cookie_store)
}
