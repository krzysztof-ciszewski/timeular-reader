use async_trait::async_trait;
use chrono::{DateTime, Local};
use log::debug;

use crate::handler::hackaru::create_handler;

pub const ORIENTATION_CHARACTERISTIC_UUID: &str = "c7e70012-c847-11e6-8175-8c89a55d403c";

pub struct TimeularConfig {
    pub sides: [Side; 8],
    pub handler: Box<dyn Handler + Send + Sync>,
}

#[derive(Debug)]
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

impl TimeularConfig {
    pub(crate) fn get_side(&self, side_num: &u8) -> &Side {
        self.sides.iter().find(|e| e.side_num.eq(side_num)).unwrap()
    }
}

pub async fn create_timeular_config() -> TimeularConfig {
    TimeularConfig {
        sides: [
            Side {
                side_num: 1,
                label: "1".to_string(),
            },
            Side {
                side_num: 2,
                label: "2".to_string(),
            },
            Side {
                side_num: 3,
                label: "3".to_string(),
            },
            Side {
                side_num: 4,
                label: "4".to_string(),
            },
            Side {
                side_num: 5,
                label: "5".to_string(),
            },
            Side {
                side_num: 6,
                label: "6".to_string(),
            },
            Side {
                side_num: 7,
                label: "7".to_string(),
            },
            Side {
                side_num: 8,
                label: "8".to_string(),
            },
        ],
        handler: Box::new(create_handler().await),
    }
}
