extern crate core;

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager, PeripheralId};
use futures::stream::StreamExt;
use log::{debug, LevelFilter};
use simplelog::{ColorChoice, Config, ConfigBuilder, TerminalMode, TermLogger};
use tokio::time::Instant;

const ORIENTATION_CHARACTERISTIC_UUID: &str = "c7e70012-c847-11e6-8175-8c89a55d403c";

struct TimeularConfig {
    sides: [Side; 8],
}

impl TimeularConfig {
}

struct Side {
    side_num: u8,
    label: String,
    action_url: String,
}

impl TimeularConfig {
    pub(crate) fn get_side(&self, side_num: &u8) -> &Side {
        self.sides
            .iter()
            .find(|e| e.side_num.eq(side_num))
            .unwrap()
    }
}

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
                    read_tracker(id, adapter).await;
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

async fn read_tracker(id: PeripheralId, adapter: Arc<Adapter>) -> Result<(), Box<dyn Error>> {
    let tracker = adapter.peripheral(&id).await.unwrap();

    tracker.connect().await?;
    tracker.discover_services().await?;

    let chars = tracker.characteristics();
    let orientation_char = chars
        .iter()
        .find(|c| c.uuid.to_string().as_str() == ORIENTATION_CHARACTERISTIC_UUID)
        .unwrap();

    tracker.subscribe(&orientation_char).await?;

    let mut prev_side: Option<u8> = None;
    let mut notification_stream = tracker.notifications().await.unwrap();
    let mut now = Instant::now();
    let trackable_sides: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let config = create_timeular_config();

    while let Some(data) = notification_stream.next().await {
        let side = data.value[0];

        debug!("current side: {}, previous side: {}",side,prev_side.unwrap_or_else(|| 255));

        if prev_side.is_some() && !prev_side.unwrap().eq(&side) {
            let duration = now.elapsed().as_secs();
            debug!("side {} was on for {:?}s",prev_side.unwrap_or_default(),duration);
            send_side_action(config.get_side(&prev_side.unwrap()), &duration).await;
        }

        if !trackable_sides.contains(&side) {
            prev_side = None;
            continue;
        }

        now = Instant::now();
        prev_side = Some(side);
    }

    Ok(())
}

async fn send_side_action(side: &Side, duration: &u64) {
    debug!("action");
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

fn create_timeular_config() -> TimeularConfig {
    TimeularConfig {
        sides: [
            Side {
                side_num: 1,
                label: "".to_string(),
                action_url: "".to_string(),
            },
            Side {
                side_num: 2,
                label: "".to_string(),
                action_url: "".to_string(),
            },
            Side {
                side_num: 3,
                label: "".to_string(),
                action_url: "".to_string(),
            },
            Side {
                side_num: 4,
                label: "".to_string(),
                action_url: "".to_string(),
            },
            Side {
                side_num: 5,
                label: "".to_string(),
                action_url: "".to_string(),
            },
            Side {
                side_num: 6,
                label: "".to_string(),
                action_url: "".to_string(),
            },
            Side {
                side_num: 7,
                label: "".to_string(),
                action_url: "".to_string(),
            },
            Side {
                side_num: 8,
                label: "".to_string(),
                action_url: "".to_string(),
            },
        ],
    }
}

fn create_logger() {
    TermLogger::init(
        LevelFilter::Debug,
        ConfigBuilder::default()
            .add_filter_allow(String::from("timeular_cli"))
            .build(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .unwrap();
}
