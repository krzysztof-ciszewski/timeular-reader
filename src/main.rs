#[allow(dead_code)]
extern crate core;

use std::error::Error;
use std::sync::Arc;

use btleplug::api::{
    bleuuid::BleUuid, BDAddr, Central, CentralEvent, Manager as _, Peripheral, ScanFilter,
};
use btleplug::platform::{Adapter, Manager, PeripheralId};
use futures::stream::StreamExt;
use futures::TryFutureExt;

const ORIENTATION_SERVICE_UUID: &str = "c7e70010-c847-11e6-8175-8c89a55d403c";
const ORIENTATION_CHARACTERISTIC_UUID: &str = "c7e70012-c847-11e6-8175-8c89a55d403c";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let adapter = Arc::new(get_adapter().await);
    let mut events = adapter.events().await?;

    println!("start scan");

    adapter.start_scan(ScanFilter::default()).await?;

    while let Some(event) = events.next().await {
        match event {
            CentralEvent::DeviceDiscovered(id) => {
                println!("{:?}", id);
                let per = match adapter.peripheral(&id).await {
                    Ok(per) => per,
                    Err(_e) => continue,
                };
                let name = match get_name(&per).await {
                    Ok(per) => per,
                    Err(_e) => continue,
                };
                println!("device {}", name);
                if !name.to_lowercase().contains("timeular") {
                    continue;
                }
                println!("found timeular");
                let adapter = adapter.clone();
                tokio::spawn(async move { read_tracker(id, adapter).await; });
            }
            CentralEvent::DeviceDisconnected(id) => {
                println!("DISCONNECT {:?}", id);
                println!("thread {}", std::thread::current().name().unwrap());
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

    let mut notification_stream = tracker.notifications().await.unwrap();
    while let Some(data) = notification_stream.next().await {
        //TODO: send data to external program, add support to faces
        println!("Received data from [{:?}]: {:?}", data.uuid, data.value);
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
        .unwrap()
}

async fn get_name(per: &impl Peripheral) -> Result<String, &str> {
    let res = match per.properties().await {
        Ok(res) => res,
        Err(e) => {
            println!("Skipped error {}", e.to_string());
            return Err("error");
        }
    };
    let per_props = match res {
        Some(per_props) => per_props,
        None => {
            println!("no props");
            return Err("no props");
        }
    };

    match per_props.local_name {
        Some(local_name) => Ok(local_name),
        None => {
            Err("no name")
        }
    }
}
