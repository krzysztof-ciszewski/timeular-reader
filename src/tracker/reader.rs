use std::{pin::Pin, sync::Arc};

use crate::handler::{get_handler, Handlers};
use anyhow::{Context as _, Result};
use btleplug::api::Peripheral;
use btleplug::api::{Central, ValueNotification};
use btleplug::platform::{Adapter, PeripheralId};
use chrono::{Local, TimeDelta};
use futures::{Stream, StreamExt};
use log::{debug, warn};
use simplelog::info;
use strum::IntoEnumIterator;

use crate::tracker::config::Handler;
use crate::tracker::state_machine::Tracker;

use super::config;

pub async fn read_tracker(id: PeripheralId, adapter: Arc<Adapter>, setup: bool) -> Result<()> {
    let tracker = adapter
        .peripheral(&id)
        .await
        .context("failed to access discovered tracker")?;

    tracker.connect().await?;
    info!("Connected");

    if setup {
        setup_tracker_config(&tracker).await?;
    }

    read_orientation(&tracker, setup).await?;

    Ok(())
}

async fn setup_tracker_config(tracker: &impl Peripheral) -> Result<()> {
    info!("Entering setup mode");

    let mut config = config::get_timeular_config()?;

    if !config.handler.is_empty() {
        info!("Currently used handler: {}", config.handler);
    }

    if let Some(handler) = get_handler_enum()? {
        config.handler = format!("{handler:?}").to_lowercase();
    }

    info!("Flip the device to a side you want to set up");
    let mut notification_stream = get_notification_stream(tracker).await?;

    while let Some(data) = notification_stream.next().await {
        let Some(side) = data.value.first().copied() else {
            warn!("Ignoring empty tracker orientation notification");
            continue;
        };

        let mut label = String::new();

        let Some(side_config) = config.sides.iter().find(|entry| entry.side_num == side) else {
            warn!("Ignoring unknown tracker side {side}");
            continue;
        };

        if !side_config.configurable {
            continue;
        }

        info!("Side {}, current label: {}", &side, side_config.label);
        info!("Please label side {}, q to finish setup", side);
        std::io::stdin()
            .read_line(&mut label)
            .context("failed to read tracker side label")?;
        label = label.trim().to_string();

        if label.eq("q") {
            break;
        }

        config.set_side(side, label);
        info!("Label saved, flip to new side to continue");
    }

    config::update_timeular_config(&config)?;
    Ok(())
}

fn get_handler_enum() -> Result<Option<Handlers>> {
    let mut message = String::from("Available handlers:");

    let mut i: u8 = 1;
    for h in Handlers::iter() {
        message.push_str(format!("\n{}: {:?}", i, h).to_lowercase().as_str());
        i += 1;
    }
    info!("{message}\nChoose handler [1-{i}]:");

    let mut handler = String::new();
    std::io::stdin()
        .read_line(&mut handler)
        .context("failed to read handler selection")?;
    handler = handler.trim().to_string();
    if handler.is_empty() {
        return Ok(None);
    }

    let idx = handler
        .parse::<u8>()
        .with_context(|| format!("invalid handler selection: {handler}"))?;

    Handlers::try_from(idx)
        .map(Some)
        .map_err(|_| anyhow::anyhow!("handler selection is out of range: {idx}"))
}

async fn get_notification_stream(
    tracker: &impl Peripheral,
) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>> {
    tracker
        .discover_services()
        .await
        .context("failed to discover tracker services")?;

    let chars = tracker.characteristics();
    let orientation_char = chars
        .iter()
        .find(|c| c.uuid.to_string().as_str() == config::ORIENTATION_CHARACTERISTIC_UUID)
        .context("tracker does not expose the orientation characteristic")?;

    tracker
        .subscribe(orientation_char)
        .await
        .context("failed to subscribe to tracker orientation updates")?;

    tracker
        .notifications()
        .await
        .context("failed to receive tracker notifications")
}

async fn read_orientation(tracker: &impl Peripheral, setup: bool) -> Result<()> {
    let mut notification_stream = get_notification_stream(tracker).await?;

    let config = config::get_timeular_config()?;

    debug!("Handler is: {}", config.handler);
    let h: Box<dyn Handler> = get_handler(setup, &config).await?;

    let mut tracker_state = Tracker::default();

    info!("Flip the device to the side you want to track");
    while let Some(data) = notification_stream.next().await {
        let Some(side_num) = data.value.first().copied() else {
            warn!("Ignoring empty tracker orientation notification");
            continue;
        };
        let side = config.sides.iter().find(|entry| entry.side_num == side_num);

        if side.is_none() {
            warn!("Tracker reported unconfigured side {side_num}");
        }

        if let Some(side) = side.filter(|side| side.is_trackable()) {
            info!("Currently tracking {}", side.label);
        }

        debug!("current side: {:?}", side);

        if let Some(entry) = tracker_state.on_side(side, Local::now()) {
            log_time_spent(entry.end - entry.start, &entry.side.label);
            h.handle(&entry)
                .await
                .with_context(|| format!("failed to record time for {}", entry.side.label))?;
        }
    }

    Ok(())
}

fn log_time_spent(duration: TimeDelta, label: &String) {
    let mut minutes = duration.num_minutes();
    if duration.num_minutes() > 0 && duration.num_hours() > 0 {
        minutes = duration.num_minutes() % (duration.num_hours() * 60);
    }

    let mut seconds = duration.num_seconds();
    if duration.num_seconds() > 0 && duration.num_minutes() > 0 {
        seconds = duration.num_seconds() % (duration.num_minutes() * 60);
    }

    info!(
        "You spent {}h {}m {}s on {}",
        duration.num_hours(),
        minutes,
        seconds,
        label
    );
}
