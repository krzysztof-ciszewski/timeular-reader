use async_trait::async_trait;
use chrono::{DateTime, Local};
use log::debug;
use serde::{Deserialize, Serialize};

use crate::config::Config;

pub const ORIENTATION_CHARACTERISTIC_UUID: &str = "c7e70012-c847-11e6-8175-8c89a55d403c";
const CONFIG_KEY: &str = "timeular";

#[derive(Debug, Serialize, Deserialize)]
pub struct TimeularConfig {
    pub sides: [Side; 8],
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Side {
    pub side_num: u8,
    pub label: String,
}

#[async_trait]
pub trait Handler: Sync {
    async fn handle(self: &Self, side: &Side, duration: &(DateTime<Local>, DateTime<Local>)) {
        debug!("handler\n side: {:?}\n duration {:?}", side, duration)
    }
}
pub struct CallbackHandler {
    callback: fn(side: &Side, duration: &(DateTime<Local>, DateTime<Local>)),
}

#[async_trait]
impl Handler for CallbackHandler {
    async fn handle(self: &Self, side: &Side, duration: &(DateTime<Local>, DateTime<Local>)) {
        (self.callback)(side, duration);
    }
}

impl Default for TimeularConfig {
    fn default() -> Self {
        TimeularConfig {
            sides: [
                Side {
                    side_num: 1,
                    label: String::new(),
                },
                Side {
                    side_num: 2,
                    label: String::new(),
                },
                Side {
                    side_num: 3,
                    label: String::new(),
                },
                Side {
                    side_num: 4,
                    label: String::new(),
                },
                Side {
                    side_num: 5,
                    label: String::new(),
                },
                Side {
                    side_num: 6,
                    label: String::new(),
                },
                Side {
                    side_num: 7,
                    label: String::new(),
                },
                Side {
                    side_num: 8,
                    label: String::new(),
                },
            ],
        }
    }
}

impl<'de> Config<'de> for TimeularConfig {}

impl TimeularConfig {
    pub(crate) fn get_side(&self, side_num: &u8) -> &Side {
        self.find_side(side_num).unwrap()
    }

    pub fn is_trackable(&self, side_num: &u8) -> bool {
        self.find_side(side_num).is_some()
    }

    fn find_side(&self, side_num: &u8) -> Option<&Side> {
        self.sides.iter().find(|e| e.side_num.eq(side_num))
    }
}

pub fn get_timeular_config() -> TimeularConfig {
    crate::config::get_config::<TimeularConfig>(CONFIG_KEY)
}
