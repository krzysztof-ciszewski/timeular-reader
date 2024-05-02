use std::{error::Error, pin::Pin, sync::Arc};

use btleplug::api::Peripheral;
use btleplug::api::{Central, ValueNotification};
use btleplug::platform::{Adapter, PeripheralId};
use chrono::Local;
use futures::{Stream, StreamExt};
use log::debug;

use crate::tracker::config::Handler;
use crate::handler::hackaru::create_handler;

use super::config;

pub async fn read_tracker(
    id: PeripheralId,
    adapter: Arc<Adapter>,
    setup: bool,
) -> Result<(), Box<dyn Error>> {
    let tracker = adapter.peripheral(&id).await.unwrap();

    tracker.connect().await?;
    println!("Connected");

    if setup {
        setup_tracker_config(&tracker).await;
    }

    read_orientation(&tracker).await?;

    Ok(())
}

async fn setup_tracker_config(tracker: &impl Peripheral) {
    println!("Entering setup mode");
    println!("Flip the device to a side you want to set up");
    let mut notification_stream = get_notification_stream(tracker).await;
    let mut config = config::get_timeular_config();

    while let Some(data) = notification_stream.next().await {
        let side = data.value[0];

        let mut label = String::new();

        println!("Side {}, current label: {}", &side, config.get_side(&side).label);
        println!("Please label side {}, q to finish setup", side);
        std::io::stdin().read_line(&mut label).unwrap();
        label = label.trim().to_string();

        if label.eq("q") {
            break;
        }

        config.set_side(side, label);
        println!("Label saved, flip to new side to continue");
    }

    config::update_timeular_config(&config);
    println!("Config updated!");
}

async fn get_notification_stream(tracker: &impl Peripheral) -> Pin<Box<dyn Stream<Item = ValueNotification> + Send>> {
    tracker.discover_services().await.unwrap();

    let chars = tracker.characteristics();
    let orientation_char = chars
        .iter()
        .find(|c| c.uuid.to_string().as_str() == config::ORIENTATION_CHARACTERISTIC_UUID)
        .unwrap();

    tracker.subscribe(&orientation_char).await.unwrap();

    return tracker.notifications().await.unwrap();
}

async fn read_orientation(tracker: &impl Peripheral) -> Result<(), Box<dyn Error>> {
    let mut notification_stream = get_notification_stream(tracker).await;

    let config = config::get_timeular_config();
    let handler = create_handler().await;

    let mut prev_side: Option<u8> = None;
    let mut start_date = Local::now();

    while let Some(data) = notification_stream.next().await {
        let side = data.value[0];
        debug!("current side: {}, previous side: {:?}", side, prev_side);

        if prev_side.is_some() && !prev_side.unwrap().eq(&side) {
            let end_date = Local::now();
            handler
                .handle(
                    config.get_side(&prev_side.unwrap()),
                    &(start_date, end_date),
                )
                .await;
        }

        if !config.is_trackable(&side) {
            prev_side = None;
            continue;
        }

        start_date = Local::now();
        prev_side = Some(side);
    }

    return Ok(());
}
