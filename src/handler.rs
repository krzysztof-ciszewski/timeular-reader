use serde_derive::{Deserialize, Serialize};
use strum::EnumIter;

pub mod hackaru;
pub mod toggl;
pub mod traggo;

#[derive(Serialize, Deserialize, EnumIter, Debug)]
pub enum Handlers {
    Hackaru = 1,
    Toggl = 2,
    Traggo = 3,
}
impl TryFrom<u8> for Handlers {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            x if x == Handlers::Hackaru as u8 => Ok(Handlers::Hackaru),
            x if x == Handlers::Toggl as u8 => Ok(Handlers::Toggl),
            x if x == Handlers::Traggo as u8 => Ok(Handlers::Traggo),
            _ => Err(()),
        }
    }
}

impl TryFrom<&String> for Handlers {
    type Error = ();

    fn try_from(v: &String) -> Result<Self, Self::Error> {
        match v {
            x if x.to_lowercase() == format!("{:?}", Handlers::Hackaru).to_lowercase() => {
                Ok(Handlers::Hackaru)
            }
            x if x.to_lowercase() == format!("{:?}", Handlers::Toggl).to_lowercase() => {
                Ok(Handlers::Toggl)
            }
            x if x.to_lowercase() == format!("{:?}", Handlers::Traggo).to_lowercase() => {
                Ok(Handlers::Traggo)
            }
            _ => Err(()),
        }
    }
}
