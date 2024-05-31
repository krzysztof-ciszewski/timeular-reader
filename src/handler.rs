use crate::tracker::config::{Handler, TimeularConfig};
use serde_derive::{Deserialize, Serialize};
use strum::EnumIter;

pub mod clockify;
pub mod example;
pub mod hackaru;
pub mod toggl;
pub mod traggo;

#[derive(Serialize, Deserialize, EnumIter, Debug)]
pub enum Handlers {
    Toggl = 1,
    Clockify = 2,
    Traggo = 3,
    Hackaru = 4,
    Example = 5,
}
impl TryFrom<u8> for Handlers {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            x if x == Handlers::Toggl as u8 => Ok(Handlers::Toggl),
            x if x == Handlers::Clockify as u8 => Ok(Handlers::Clockify),
            x if x == Handlers::Traggo as u8 => Ok(Handlers::Traggo),
            x if x == Handlers::Hackaru as u8 => Ok(Handlers::Hackaru),
            x if x == Handlers::Example as u8 => Ok(Handlers::Example),
            _ => Err(()),
        }
    }
}

impl TryFrom<&String> for Handlers {
    type Error = ();

    fn try_from(v: &String) -> Result<Self, Self::Error> {
        match v.as_str() {
            "toggl" => Ok(Handlers::Toggl),
            "clockify" => Ok(Handlers::Clockify),
            "traggo" => Ok(Handlers::Traggo),
            "hackaru" => Ok(Handlers::Hackaru),
            "example" => Ok(Handlers::Example),
            _ => Err(()),
        }
    }
}

pub async fn get_handler(setup: bool, config: &TimeularConfig) -> Box<dyn Handler> {
    match config.handler.as_str() {
        "toggl" => Box::new(toggl::create_handler(setup).await),
        "hackaru" => Box::new(hackaru::create_handler(setup).await),
        "clockify" => Box::new(clockify::create_handler(setup).await),
        "traggo" => Box::new(traggo::create_handler(setup).await),
        "example" => Box::new(example::create_handler(setup).await),
        _ => Box::new(example::create_handler(setup).await),
    }
}
