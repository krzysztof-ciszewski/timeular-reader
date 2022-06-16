use std::{error::Error, sync::Arc};

use btleplug::api::{Central, Peripheral};
use btleplug::platform::{Adapter, PeripheralId};
use chrono::Local;
use futures::StreamExt;
use log::debug;

use super::config;

pub async fn read_tracker(id: PeripheralId, adapter: Arc<Adapter>) -> Result<(), Box<dyn Error>> {
    let tracker = adapter.peripheral(&id).await.unwrap();

    tracker.connect().await?;
    tracker.discover_services().await?;

    let chars = tracker.characteristics();
    let orientation_char = chars
        .iter()
        .find(|c| c.uuid.to_string().as_str() == config::ORIENTATION_CHARACTERISTIC_UUID)
        .unwrap();

    tracker.subscribe(&orientation_char).await?;

    let mut prev_side: Option<u8> = None;
    let mut notification_stream = tracker.notifications().await.unwrap();
    let trackable_sides: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let config = config::create_timeular_config().await;
    let mut start_date = Local::now();

    while let Some(data) = notification_stream.next().await {
        let side = data.value[0];
        debug!("current side: {}, previous side: {:?}", side, prev_side);

        if prev_side.is_some() && !prev_side.unwrap().eq(&side) {
            let end_date = Local::now();
            config
                .handler
                .handle(
                    config.get_side(&prev_side.unwrap()),
                    &(start_date, end_date),
                )
                .await;
        }

        if !trackable_sides.contains(&side) {
            prev_side = None;
            continue;
        }

        start_date = Local::now();
        prev_side = Some(side);
    }

    Ok(())
}
