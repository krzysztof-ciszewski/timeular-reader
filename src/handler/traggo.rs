use crate::tracker::config::{Handler, Side};
use async_trait::async_trait;
use chrono::{DateTime, Local};

pub struct TraggoConfig {}
#[derive(Debug, Default)]
pub struct Traggo {}

#[async_trait]
impl Handler for Traggo {
    async fn handle(&self, _side: &Side, _duration: &(DateTime<Local>, DateTime<Local>)) {
        todo!()
    }
}

pub async fn create_handler(_setup: bool) -> Traggo {
    return Traggo {};
}
