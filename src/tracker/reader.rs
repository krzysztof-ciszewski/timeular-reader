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
use tokio::sync::watch;

use crate::tracker::config::Handler;
use crate::tracker::state_machine::Tracker;

use super::config;

pub async fn read_tracker(
    id: PeripheralId,
    adapter: Arc<Adapter>,
    setup: bool,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let tracker = adapter
        .peripheral(&id)
        .await
        .context("failed to access discovered tracker")?;

    tracker.connect().await?;
    info!("Connected");

    if setup {
        setup_tracker_config(&tracker).await?;
    }

    read_orientation(&tracker, setup, shutdown).await?;

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

async fn read_orientation(
    tracker: &impl Peripheral,
    setup: bool,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let notification_stream = get_notification_stream(tracker).await?;

    let config = config::get_timeular_config()?;

    debug!("Handler is: {}", config.handler);
    let h: Box<dyn Handler> = get_handler(setup, &config).await?;

    info!("Flip the device to the side you want to track");
    let orientations = notification_stream.map(|data| data.value.first().copied());
    process_orientations(orientations, &config.sides, h.as_ref(), &mut shutdown).await
}

async fn process_orientations<S>(
    orientations: S,
    sides: &[config::Side],
    handler: &dyn Handler,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<()>
where
    S: Stream<Item = Option<u8>>,
{
    let mut orientations = Box::pin(orientations);
    let mut tracker_state = Tracker::default();

    loop {
        let side_num = tokio::select! {
            orientation = orientations.next() => match orientation {
                Some(side_num) => side_num,
                None => break,
            },
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
                continue;
            }
        };

        let Some(side_num) = side_num else {
            warn!("Ignoring empty tracker orientation notification");
            continue;
        };
        let side = sides.iter().find(|entry| entry.side_num == side_num);

        if side.is_none() {
            warn!("Tracker reported unconfigured side {side_num}");
        }

        if let Some(side) = side.filter(|side| side.is_trackable()) {
            info!("Currently tracking {}", side.label);
        }

        debug!("current side: {:?}", side);

        if let Some(entry) = tracker_state.on_side(side, Local::now()) {
            record_entry(handler, &entry).await?;
        }
    }

    if let Some(entry) = tracker_state.finish(Local::now()) {
        record_entry(handler, &entry).await?;
    }

    Ok(())
}

async fn record_entry(
    handler: &dyn Handler,
    entry: &crate::tracker::config::TimeEntry,
) -> Result<()> {
    log_time_spent(entry.end - entry.start, &entry.side.label);
    handler
        .handle(entry)
        .await
        .with_context(|| format!("failed to record time for {}", entry.side.label))
}

fn log_time_spent(duration: TimeDelta, label: &String) {
    info!("You spent {} on {}", format_time_spent(duration), label);
}

fn format_time_spent(duration: TimeDelta) -> String {
    format!(
        "{}h {}m {}s",
        duration.num_hours(),
        duration.num_minutes() % 60,
        duration.num_seconds() % 60
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::TimeDelta;
    use futures::stream;
    use tokio::sync::{oneshot, watch};

    use crate::tracker::config::{Handler, Side, TimeEntry};

    use super::{format_time_spent, process_orientations};

    #[derive(Default)]
    struct RecordingHandler {
        entries: Mutex<Vec<TimeEntry>>,
    }

    #[async_trait]
    impl Handler for RecordingHandler {
        async fn handle(&self, entry: &TimeEntry) -> anyhow::Result<()> {
            self.entries.lock().unwrap().push(entry.clone());
            Ok(())
        }
    }

    #[test]
    fn formats_duration_components_without_carrying_totals() {
        assert_eq!(format_time_spent(TimeDelta::zero()), "0h 0m 0s");
        assert_eq!(format_time_spent(TimeDelta::seconds(59)), "0h 0m 59s");
        assert_eq!(format_time_spent(TimeDelta::seconds(3_661)), "1h 1m 1s");
        assert_eq!(format_time_spent(TimeDelta::seconds(90_061)), "25h 1m 1s");
    }

    fn sides() -> Vec<Side> {
        vec![Side {
            side_num: 1,
            label: "Work".into(),
            configurable: true,
        }]
    }

    #[tokio::test]
    async fn stream_end_flushes_the_active_entry_once() {
        let handler = RecordingHandler::default();
        let (_shutdown_tx, mut shutdown_rx) = watch::channel(false);

        process_orientations(
            stream::iter([Some(1)]),
            &sides(),
            &handler,
            &mut shutdown_rx,
        )
        .await
        .unwrap();

        let entries = handler.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].side.label, "Work");
        assert!(entries[0].end >= entries[0].start);
    }

    #[tokio::test]
    async fn shutdown_flushes_the_active_entry_once() {
        let handler = Arc::new(RecordingHandler::default());
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let (ready_tx, ready_rx) = oneshot::channel();
        let orientations = stream::unfold((0, Some(ready_tx)), |(state, ready_tx)| async move {
            if state == 0 {
                Some((Some(1), (1, ready_tx)))
            } else {
                if let Some(ready_tx) = ready_tx {
                    let _ = ready_tx.send(());
                }
                std::future::pending().await
            }
        });
        let task_handler = handler.clone();
        let task = tokio::spawn(async move {
            process_orientations(
                orientations,
                &sides(),
                task_handler.as_ref(),
                &mut shutdown_rx,
            )
            .await
        });

        ready_rx.await.unwrap();
        shutdown_tx.send_replace(true);
        task.await.unwrap().unwrap();

        let entries = handler.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].side.label, "Work");
        assert!(entries[0].end >= entries[0].start);
    }

    #[tokio::test]
    async fn stream_end_without_an_active_entry_does_not_call_handler() {
        let handler = RecordingHandler::default();
        let (_shutdown_tx, mut shutdown_rx) = watch::channel(false);

        process_orientations(stream::empty(), &sides(), &handler, &mut shutdown_rx)
            .await
            .unwrap();

        assert!(handler.entries.lock().unwrap().is_empty());
    }
}
