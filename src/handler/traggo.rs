use anyhow::{bail, Context as _, Result};
use async_trait::async_trait;
use reqwest::Client;
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use simplelog::info;
use std::io;

use crate::tracker::config::{Handler, Side, TimeEntry};

use self::config::{create_config, update_config, TraggoConfig};

pub mod config;

const CREATE_TIME_SPAN: &str = r#"
mutation CreateTimeSpan($start: Time!, $end: Time!, $tags: [InputTimeSpanTag!], $note: String!) {
    createTimeSpan(start: $start, end: $end, tags: $tags, note: $note) {
        id
    }
}
"#;

#[derive(Serialize)]
struct GraphQlRequest<'a> {
    query: &'static str,
    variables: GraphQlVariables<'a>,
}

#[derive(Serialize)]
struct GraphQlVariables<'a> {
    start: String,
    end: String,
    tags: Vec<InputTimeSpanTag<'a>>,
    note: &'static str,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct InputTimeSpanTag<'a> {
    key: &'a str,
    value: &'a str,
}

#[derive(Deserialize)]
struct GraphQlResponse {
    #[serde(default)]
    data: Option<CreateTimeSpanData>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

#[derive(Deserialize)]
struct CreateTimeSpanData {
    #[serde(rename = "createTimeSpan")]
    create_time_span: Option<CreateTimeSpanResult>,
}

#[derive(Deserialize)]
struct CreateTimeSpanResult {
    id: i32,
}

#[derive(Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Debug)]
pub struct Traggo {
    client: Client,
    config: TraggoConfig,
}

#[async_trait]
impl Handler for Traggo {
    async fn handle(&self, entry: &TimeEntry) -> Result<()> {
        let tags = parse_tags(&entry.side.label)
            .with_context(|| format!("invalid Traggo tags for side {}", entry.side.side_num))?;

        let request = GraphQlRequest {
            query: CREATE_TIME_SPAN,
            variables: GraphQlVariables {
                start: entry.start.to_rfc3339(),
                end: entry.end.to_rfc3339(),
                tags,
                note: "",
            },
        };
        let response_body = super::http::send_with_retry(
            "Traggo time-entry request",
            super::http::ResponseBodyPolicy::ReadOnce,
            || {
                self.client
                    .post(&self.config.graphql_url)
                    .header(
                        reqwest::header::AUTHORIZATION,
                        format!("traggo {}", self.config.api_token),
                    )
                    .json(&request)
            },
        )
        .await
        .context("failed to submit time entry to Traggo")?;

        let response: GraphQlResponse = serde_json::from_slice(&response_body)
            .context("failed to parse Traggo GraphQL response")?;
        if !response.errors.is_empty() {
            let messages = response
                .errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; ");
            bail!("Traggo rejected the time entry: {messages}");
        }
        let result = response
            .data
            .and_then(|data| data.create_time_span)
            .context("Traggo response did not contain a created time span")?;
        log::debug!("Traggo accepted time span {}", result.id);
        Ok(())
    }
}

pub(crate) fn parse_tags(label: &str) -> Result<Vec<InputTimeSpanTag<'_>>> {
    let mut tags = Vec::<InputTimeSpanTag<'_>>::new();
    for pair in label.split(',').map(str::trim) {
        let (key, value) = pair
            .split_once(':')
            .context("each Traggo tag must have the form key:value, separated by commas")?;
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            bail!("Traggo tag keys and values must not be empty");
        }
        tags.push(InputTimeSpanTag { key, value });
    }
    Ok(tags)
}

fn validate_sides(sides: &[Side]) -> Result<()> {
    for side in sides.iter().filter(|side| side.is_trackable()) {
        parse_tags(&side.label)
            .with_context(|| format!("invalid Traggo tags for side {}", side.side_num))?;
    }
    Ok(())
}

pub async fn create_handler(setup: bool, sides: &[Side]) -> Result<Traggo> {
    validate_sides(sides)?;
    let mut config = create_config()?;
    update_vendor_config(&mut config, setup)?;
    validate_config(&config)?;

    Ok(Traggo {
        client: Client::builder()
            .build()
            .context("failed to create Traggo HTTP client")?,
        config,
    })
}

fn validate_config(config: &TraggoConfig) -> Result<()> {
    if config.graphql_url.trim().is_empty() {
        bail!("Traggo GraphQL URL must not be empty");
    }
    if config.api_token.trim().is_empty() {
        bail!("Traggo device token must not be empty");
    }
    Ok(())
}

fn update_vendor_config(config: &mut TraggoConfig, setup: bool) -> Result<()> {
    if setup || config.graphql_url.is_empty() {
        let message = if config.graphql_url.is_empty() {
            "Provide your Traggo GraphQL URL (for example, http://localhost:3030/graphql)"
                .to_string()
        } else {
            format!(
                "Provide your Traggo GraphQL URL (currently {}, leave blank to keep it)",
                config.graphql_url
            )
        };
        info!("{message}");

        let mut graphql_url = String::new();
        io::stdin()
            .read_line(&mut graphql_url)
            .context("failed to read Traggo GraphQL URL")?;
        let graphql_url = graphql_url.trim();
        if !graphql_url.is_empty() {
            config.graphql_url = graphql_url.to_string();
            update_config(config)?;
        }
    }

    if setup || config.api_token.is_empty() {
        let message = if config.api_token.is_empty() {
            "Provide your Traggo device token".to_string()
        } else {
            "Provide your Traggo device token (leave blank to keep the current value)".to_string()
        };
        let api_token = prompt_password(message)
            .context("failed to read Traggo device token")?
            .trim()
            .to_string();
        if !api_token.is_empty() {
            config.api_token = api_token;
            update_config(config)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_tags, validate_sides, InputTimeSpanTag, Traggo};
    use crate::{
        test_support::{request_body, time_entry, MockServer},
        tracker::config::Handler,
    };
    use reqwest::Client;
    use serde_json::json;

    #[test]
    fn parses_multiple_tags_with_spaces_and_colons_in_values() {
        assert_eq!(
            parse_tags(" project : Client A , type:Meeting, link:https://example.com:3030 ")
                .unwrap(),
            vec![
                InputTimeSpanTag {
                    key: "project",
                    value: "Client A",
                },
                InputTimeSpanTag {
                    key: "type",
                    value: "Meeting",
                },
                InputTimeSpanTag {
                    key: "link",
                    value: "https://example.com:3030",
                },
            ]
        );
    }

    #[test]
    fn accepts_repeated_keys_and_trims_whitespace_around_each_tag() {
        for label in [
            "project:Client,project:Personal",
            "project:Client, project :Personal",
            " \tproject : Client \t, \tproject : Personal \n",
        ] {
            assert_eq!(
                parse_tags(label).unwrap(),
                vec![
                    InputTimeSpanTag {
                        key: "project",
                        value: "Client",
                    },
                    InputTimeSpanTag {
                        key: "project",
                        value: "Personal",
                    },
                ]
            );
        }
    }

    #[test]
    fn rejects_invalid_tags_without_returning_a_partial_list() {
        for label in [
            "",
            " ",
            "Work",
            ":Work",
            "project:",
            "project: ",
            "project:Client, Meeting",
            "project:Client,",
            "project:Client,,type:Meeting",
        ] {
            assert!(parse_tags(label).is_err(), "accepted invalid tags: {label}");
        }
    }

    #[test]
    fn validates_trackable_side_labels_and_skips_disabled_sides() {
        let sides = [
            time_entry(1, "project:Client, type:Meeting").side,
            time_entry(2, "").side,
            time_entry(3, "activity:Reading").side,
        ];
        validate_sides(&sides).unwrap();

        let error = validate_sides(&[
            sides[0].clone(),
            time_entry(3, "activity:Reading, broken").side,
        ])
        .unwrap_err();
        assert!(format!("{error:#}").contains("invalid Traggo tags for side 3"));
    }

    #[tokio::test]
    async fn creates_a_tagged_time_span_with_the_configured_token() {
        let server = MockServer::start(1, |_, _| {
            (200, r#"{"data":{"createTimeSpan":{"id":42}}}"#.into())
        });
        let entry = time_entry(1, "activity:Quote: \" slash: \\ newline:\nline 2");
        let handler = Traggo {
            client: Client::new(),
            config: super::config::TraggoConfig {
                graphql_url: format!("{}/graphql", server.url()),
                api_token: "device-secret".into(),
            },
        };

        handler.handle(&entry).await.unwrap();

        let requests = server.finish();
        let request = &requests[0];
        assert!(request.starts_with("POST /graphql HTTP/1.1"));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: traggo device-secret"));
        let body: serde_json::Value = serde_json::from_str(request_body(request)).unwrap();
        assert_eq!(
            body,
            json!({
                "query": super::CREATE_TIME_SPAN,
                "variables": {
                    "start": entry.start.to_rfc3339(),
                    "end": entry.end.to_rfc3339(),
                    "tags": [{"key": "activity", "value": "Quote: \" slash: \\ newline:\nline 2"}],
                    "note": ""
                }
            })
        );
    }

    #[tokio::test]
    async fn creates_time_spans_with_different_tags_for_each_side() {
        let server = MockServer::start(2, |_, _| {
            (200, r#"{"data":{"createTimeSpan":{"id":42}}}"#.into())
        });
        let handler = Traggo {
            client: Client::new(),
            config: super::config::TraggoConfig {
                graphql_url: format!("{}/graphql", server.url()),
                api_token: "device-secret".into(),
            },
        };
        handler
            .handle(&time_entry(
                1,
                " project:Client A , project : Personal , type:Meeting ",
            ))
            .await
            .unwrap();
        handler
            .handle(&time_entry(2, "activity:Reading, book:Rust: A Guide"))
            .await
            .unwrap();

        let requests = server.finish();
        let first: serde_json::Value = serde_json::from_str(request_body(&requests[0])).unwrap();
        let second: serde_json::Value = serde_json::from_str(request_body(&requests[1])).unwrap();
        assert_eq!(
            first["variables"]["tags"],
            json!([
                {"key": "project", "value": "Client A"},
                {"key": "project", "value": "Personal"},
                {"key": "type", "value": "Meeting"},
            ])
        );
        assert_eq!(
            second["variables"]["tags"],
            json!([
                {"key": "activity", "value": "Reading"},
                {"key": "book", "value": "Rust: A Guide"},
            ])
        );
    }

    #[tokio::test]
    async fn retries_with_the_same_multiple_tags() {
        let server = MockServer::start(2, |index, _| {
            if index == 0 {
                (503, "{}".into())
            } else {
                (200, r#"{"data":{"createTimeSpan":{"id":42}}}"#.into())
            }
        });
        let handler = Traggo {
            client: Client::new(),
            config: super::config::TraggoConfig {
                graphql_url: format!("{}/graphql", server.url()),
                api_token: "device-secret".into(),
            },
        };

        handler
            .handle(&time_entry(1, "project:Client, type:Meeting"))
            .await
            .unwrap();

        let requests = server.finish();
        let first: serde_json::Value = serde_json::from_str(request_body(&requests[0])).unwrap();
        let second: serde_json::Value = serde_json::from_str(request_body(&requests[1])).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first["variables"]["tags"],
            json!([
                {"key": "project", "value": "Client"},
                {"key": "type", "value": "Meeting"},
            ])
        );
    }

    #[tokio::test]
    async fn reports_graphql_errors_even_when_http_succeeds() {
        let server = MockServer::start(1, |_, _| {
            (
                200,
                r#"{"data":{"createTimeSpan":null},"errors":[{"message":"not authorized"}]}"#
                    .into(),
            )
        });
        let handler = Traggo {
            client: Client::new(),
            config: super::config::TraggoConfig {
                graphql_url: format!("{}/graphql", server.url()),
                ..super::config::TraggoConfig::default()
            },
        };

        let error = handler
            .handle(&time_entry(1, "activity:Work"))
            .await
            .unwrap_err();

        assert!(format!("{error:#}").contains("Traggo rejected the time entry: not authorized"));
        server.finish();
    }

    #[tokio::test]
    async fn reports_a_response_without_a_created_time_span() {
        let server = MockServer::start(1, |_, _| (200, r#"{"data":{}}"#.into()));
        let handler = Traggo {
            client: Client::new(),
            config: super::config::TraggoConfig {
                graphql_url: format!("{}/graphql", server.url()),
                ..super::config::TraggoConfig::default()
            },
        };

        let error = handler
            .handle(&time_entry(1, "activity:Work"))
            .await
            .unwrap_err();

        assert!(
            format!("{error:#}").contains("Traggo response did not contain a created time span")
        );
        server.finish();
    }

    #[tokio::test]
    async fn rejects_malformed_side_tags_before_sending_a_request() {
        let handler = Traggo {
            client: Client::new(),
            config: super::config::TraggoConfig::default(),
        };

        let error = handler.handle(&time_entry(1, "Work")).await.unwrap_err();

        assert!(format!("{error:#}").contains("invalid Traggo tags for side 1"));
        assert!(format!("{error:#}").contains("key:value"));
    }

    #[test]
    fn validates_required_connection_settings() {
        let error = super::validate_config(&super::config::TraggoConfig::default()).unwrap_err();
        assert_eq!(error.to_string(), "Traggo GraphQL URL must not be empty");

        let error = super::validate_config(&super::config::TraggoConfig {
            graphql_url: "https://traggo.example/graphql".into(),
            ..super::config::TraggoConfig::default()
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "Traggo device token must not be empty");
    }
}
