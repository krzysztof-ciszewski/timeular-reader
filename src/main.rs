pub mod config;
pub mod handler;
pub mod tracker;

extern crate core;

use std::error::Error;
use std::sync::Arc;

use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager};
use futures::stream::StreamExt;
use log::{debug, LevelFilter};
use simplelog::{ColorChoice, ConfigBuilder, TermLogger, TerminalMode};

use crate::tracker::reader;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    create_logger();
    let adapter = Arc::new(get_adapter().await);
    let mut events = adapter.events().await?;

    debug!("start scan");

    adapter.start_scan(ScanFilter::default()).await?;

    while let Some(event) = events.next().await {
        match event {
            CentralEvent::DeviceDiscovered(id) => {
                let per = match adapter.peripheral(&id).await {
                    Ok(per) => per,
                    Err(_e) => continue,
                };
                let name = match get_name(&per).await {
                    Ok(per) => per,
                    Err(_e) => continue,
                };
                if !name.to_lowercase().contains("timeular") {
                    continue;
                }
                debug!("found timeular");
                let adapter = adapter.clone();
                tokio::spawn(async move {
                    reader::read_tracker(id, adapter).await;
                });
            }
            CentralEvent::DeviceDisconnected(id) => {
                debug!("DISCONNECT {:?}", id);
                debug!("thread {}", std::thread::current().name().unwrap());
            }
            _ => {}
        }
    }

    Ok(())
}

async fn get_adapter() -> Adapter {
    Manager::new()
        .await
        .unwrap()
        .adapters()
        .await
        .unwrap()
        .into_iter()
        .nth(0)
        .expect("Bluetooth manager not found. Make sure bluetooth is turned on.")
}

async fn get_name(per: &impl Peripheral) -> Result<String, &str> {
    let res = match per.properties().await {
        Ok(res) => res,
        Err(_e) => {
            return Err("err");
        }
    };
    let per_props = match res {
        Some(per_props) => per_props,
        None => {
            return Err("no props");
        }
    };

    match per_props.local_name {
        Some(local_name) => Ok(local_name),
        None => Err("no name"),
    }
}

fn create_logger() {
    TermLogger::init(
        LevelFilter::Debug,
        ConfigBuilder::default()
            .add_filter_allow(String::from("timeular_reader"))
            .add_filter_allow(String::from("reqwest"))
            .build(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .unwrap();
}
