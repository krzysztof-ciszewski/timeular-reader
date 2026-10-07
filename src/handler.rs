use crate::tracker::config::{Handler, TimeularConfig};
use anyhow::{bail, Result as AppResult};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

pub mod clockify;
pub mod example;
pub mod hackaru;
pub mod timetagger;
pub mod toggl;
pub mod traggo;

#[derive(Serialize, Deserialize, EnumIter, Debug)]
pub enum Handlers {
    Toggl = 1,
    Clockify = 2,
    Traggo = 3,
    Hackaru = 4,
    Example = 5,
    Timetagger = 6,
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
            x if x == Handlers::Timetagger as u8 => Ok(Handlers::Timetagger),
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
            "timetagger" => Ok(Handlers::Timetagger),
            _ => Err(()),
        }
    }
}

pub async fn get_handler(setup: bool, config: &TimeularConfig) -> AppResult<Box<dyn Handler>> {
    match config.handler.as_str() {
        "toggl" => Ok(Box::new(toggl::create_handler(setup, &config.sides).await?)),
        "hackaru" => Ok(Box::new(
            hackaru::create_handler(setup, &config.sides).await?,
        )),
        "clockify" => Ok(Box::new(
            clockify::create_handler(setup, &config.sides).await?,
        )),
        "traggo" => Ok(Box::new(traggo::create_handler(setup).await)),
        "example" => Ok(Box::new(
            example::create_handler(setup, &config.sides).await?,
        )),
        "timetagger" => Ok(Box::new(timetagger::create_handler(setup).await?)),
        handler => bail!("unknown time-tracking handler: {handler}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{get_handler, Handlers};
    use crate::tracker::config::TimeularConfig;

    #[test]
    fn handler_names_and_ids_round_trip_for_every_registered_handler() {
        let handlers = [
            ("toggl", 1),
            ("clockify", 2),
            ("traggo", 3),
            ("hackaru", 4),
            ("example", 5),
            ("timetagger", 6),
        ];

        for (name, id) in handlers {
            let from_name = Handlers::try_from(&name.to_string()).unwrap();
            let from_id = Handlers::try_from(id).unwrap();
            assert!(std::mem::discriminant(&from_name) == std::mem::discriminant(&from_id));
        }

        assert!(Handlers::try_from(0).is_err());
        assert!(Handlers::try_from(7).is_err());
        assert!(Handlers::try_from(&"unknown".to_string()).is_err());
    }

    #[tokio::test]
    async fn handler_factory_rejects_unknown_names() {
        let config = TimeularConfig {
            sides: Vec::new(),
            handler: "unknown".into(),
        };

        let error = get_handler(false, &config).await.err().unwrap();
        assert!(error.to_string().contains("unknown time-tracking handler"));
    }
}
