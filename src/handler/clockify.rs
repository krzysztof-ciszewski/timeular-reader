use crate::tracker::config::{Handler, Side};
use async_trait::async_trait;
use chrono::{DateTime, Local};

pub struct ClockifyConfig {}
#[derive(Debug, Default)]
pub struct Clockify {}

#[async_trait]
impl Handler for Clockify {
    async fn handle(self: &Self, _side: &Side, _duration: &(DateTime<Local>, DateTime<Local>)) {
        todo!()
    }
}
pub async fn create_handler(_setup: bool) -> Clockify {
    return Clockify {};
}
