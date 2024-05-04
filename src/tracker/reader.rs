use std::{error::Error, pin::Pin, sync::Arc};

use crate::handler::{get_create_handler, handle, Handlers};
use btleplug::api::Peripheral;
use btleplug::api::{Central, ValueNotification};
use btleplug::platform::{Adapter, PeripheralId};
use chrono::Local;
use futures::{Stream, StreamExt};
use log::debug;
use simplelog::info;
use strum::IntoEnumIterator;

use crate::tracker::config::Side;

use super::config;

pub async fn read_tracker(
    id: PeripheralId,
    adapter: Arc<Adapter>,
    setup: bool,
) -> Result<(), Box<dyn Error>> {
    let tracker = adapter.peripheral(&id).await.unwrap();

    tracker.connect().await?;
    info!("Connected");

    if setup {
        setup_tracker_config(&tracker).await;
    }

    read_orientation(&tracker, setup).await?;

    return Ok(());
}

async fn setup_tracker_config(tracker: &impl Peripheral) {
    info!("Entering setup mode");

    let mut config = config::get_timeular_config();

    if !config.handler.is_empty() {
        info!("Currently used handler: {}", config.handler);
    }

    let handler = get_handler_enum();
    if handler.is_some() {
        config.handler = format!("{:?}", handler.unwrap()).to_string().to_lowercase();
    }

    info!("Flip the device to a side you want to set up");
    let mut notification_stream = get_notification_stream(tracker).await;

    while let Some(data) = notification_stream.next().await {
        let side = data.value[0];

        let mut label = String::new();

        info!(
            "Side {}, current label: {}",
            &side,
            config.get_side(&side).label
        );
        info!("Please label side {}, q to finish setup", side);
        std::io::stdin().read_line(&mut label).unwrap();
        label = label.trim().to_string();

        if label.eq("q") {
            break;
        }

        config.set_side(side, label);
        info!("Label saved, flip to new side to continue");
    }

    config::update_timeular_config(&config);
}

fn get_handler_enum() -> Option<Handlers> {
    let mut message = String::from_utf8(
        "Select handler, leave blank to skip\nAvailable options:"
            .as_bytes()
            .to_vec(),
    )
    .unwrap();

    let mut i: u8 = 1;
    for h in Handlers::iter() {
        message.push_str(format!("\n{}: {:?}", i, h).to_lowercase().as_str());
        i += 1;
    }
    info!("{message}\nType the number:");

    let mut handler = String::new();
    std::io::stdin().read_line(&mut handler).unwrap();
    handler = handler.trim().to_string();
    if handler.is_empty() {
        return None;
    }

    let idx = handler.parse::<u8>().unwrap();

    Some(Handlers::try_from(idx).unwrap())
}

async fn get_notification_stream(
    tracker: &impl Peripheral,
) -> Pin<Box<dyn Stream<Item = ValueNotification> + Send>> {
    tracker.discover_services().await.unwrap();

    let chars = tracker.characteristics();
    let orientation_char = chars
        .iter()
        .find(|c| c.uuid.to_string().as_str() == config::ORIENTATION_CHARACTERISTIC_UUID)
        .unwrap();

    tracker.subscribe(&orientation_char).await.unwrap();

    return tracker.notifications().await.unwrap();
}

async fn read_orientation(tracker: &impl Peripheral, setup: bool) -> Result<(), Box<dyn Error>> {
    let mut notification_stream = get_notification_stream(tracker).await;

    let config = config::get_timeular_config();

    let create_handler = get_create_handler(setup, &config.handler).await;

    let mut prev_side: Option<&Side> = None;
    let mut start_date = Local::now();

    info!("Flip the device to the side you want to track");
    while let Some(data) = notification_stream.next().await {
        let side = config.get_side(&data.value[0]);
        info!(
            "Currently tracking side number {} label {}",
            side.side_num, side.label
        );
        debug!("current side: {}, previous side: {:?}", side, prev_side);

        if prev_side.is_some() && prev_side.unwrap() != side {
            let end_date = Local::now();
            let duration = end_date - start_date;

            info!(
                "You spent {}h {}m {}s on {}",
                duration.num_hours(),
                duration.num_minutes(),
                duration.num_seconds(),
                prev_side.unwrap().label
            );

            handle(&create_handler, side, &(start_date, end_date)).await;
        }

        if !config.is_trackable(&side.side_num) {
            prev_side = None;
            continue;
        }

        start_date = Local::now();
        prev_side = Some(side);
    }

    return Ok(());
}
