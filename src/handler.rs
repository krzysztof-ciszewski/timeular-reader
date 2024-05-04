use crate::handler::clockify::Clockify;
use crate::handler::hackaru::Hackaru;
use crate::handler::toggl::Toggl;
use crate::handler::traggo::Traggo;
use crate::tracker::config::{Handler, Side};
use chrono::{DateTime, Local};
use serde_derive::{Deserialize, Serialize};
use strum::EnumIter;

pub mod clockify;
pub mod hackaru;
pub mod toggl;
pub mod traggo;

#[derive(Serialize, Deserialize, EnumIter, Debug)]
pub enum Handlers {
    Toggl = 1,
    Clockify = 2,
    Traggo = 3,
    Hackaru = 4,
}

#[derive(Debug, Default)]
pub enum CreateHandler {
    #[default]
    None,
    Toggl(Toggl),
    Clockify(Clockify),
    Traggo(Traggo),
    Hackaru(Hackaru),
}

impl TryFrom<u8> for Handlers {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            x if x == Handlers::Toggl as u8 => Ok(Handlers::Toggl),
            x if x == Handlers::Clockify as u8 => Ok(Handlers::Clockify),
            x if x == Handlers::Traggo as u8 => Ok(Handlers::Traggo),
            x if x == Handlers::Hackaru as u8 => Ok(Handlers::Hackaru),
            _ => Err(()),
        }
    }
}

impl TryFrom<&String> for Handlers {
    type Error = ();

    fn try_from(v: &String) -> Result<Self, Self::Error> {
        match v {
            x if x.to_lowercase() == format!("{:?}", Handlers::Toggl).to_lowercase() => {
                Ok(Handlers::Toggl)
            }
            x if x.to_lowercase() == format!("{:?}", Handlers::Clockify).to_lowercase() => {
                Ok(Handlers::Clockify)
            }
            x if x.to_lowercase() == format!("{:?}", Handlers::Traggo).to_lowercase() => {
                Ok(Handlers::Traggo)
            }
            x if x.to_lowercase() == format!("{:?}", Handlers::Hackaru).to_lowercase() => {
                Ok(Handlers::Hackaru)
            }
            _ => Err(()),
        }
    }
}

pub async fn get_create_handler(setup: bool, config_handler: &String) -> CreateHandler {
    let handler = Handlers::try_from(config_handler).unwrap();

    return match handler {
        Handlers::Toggl => CreateHandler::Toggl(toggl::create_handler(setup).await),
        Handlers::Clockify => CreateHandler::Clockify(clockify::create_handler(setup).await),
        Handlers::Traggo => CreateHandler::Traggo(traggo::create_handler(setup).await),
        Handlers::Hackaru => CreateHandler::Hackaru(hackaru::create_handler(setup).await),
    };
}

pub async fn handle(
    create_handler: &CreateHandler,
    side: &Side,
    duration: &(DateTime<Local>, DateTime<Local>),
) {
    match create_handler {
        CreateHandler::Toggl(h) => {
            h.handle(side, duration).await;
        }
        CreateHandler::Clockify(h) => {
            h.handle(side, duration).await;
        }
        CreateHandler::Traggo(h) => {
            h.handle(side, duration).await;
        }
        CreateHandler::Hackaru(h) => {
            h.handle(side, duration).await;
        }
        CreateHandler::None => {
            panic!("CreateHandler should never be none")
        }
    }
}
