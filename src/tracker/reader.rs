use std::{pin::Pin, sync::Arc, time::Duration};

use crate::handler::{get_handler, Handlers};
use anyhow::{Context as _, Result};
use btleplug::api::Peripheral;
use btleplug::api::{Central, ValueNotification};
use btleplug::platform::{Adapter, PeripheralId};
use chrono::{Local, TimeDelta};
use futures::{Stream, StreamExt};
use log::{debug, error, warn};
use simplelog::info;
use strum::IntoEnumIterator;
use tokio::sync::{mpsc, watch};

use crate::tracker::config::Handler;
use crate::tracker::state_machine::Tracker;

use super::config;

const SUBMISSION_QUEUE_CAPACITY: usize = 100;
const ENTRY_SUBMISSION_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(15);

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
    let h = get_handler(setup, &config).await?;

    info!("Flip the device to the side you want to track");
    let orientations = notification_stream.map(|data| data.value.first().copied());
    process_orientations(orientations, &config.sides, h, &mut shutdown).await
}

async fn process_orientations<S>(
    orientations: S,
    sides: &[config::Side],
    handler: Arc<dyn Handler>,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<()>
where
    S: Stream<Item = Option<u8>>,
{
    let mut orientations = Box::pin(orientations);
    let mut tracker_state = Tracker::default();
    let (submission_transmitter, mut submission_receiver) =
        mpsc::channel(SUBMISSION_QUEUE_CAPACITY);
    let submission_worker = tokio::spawn(async move {
        while let Some(entry) = submission_receiver.recv().await {
            if let Err(error) =
                record_entry_with_timeout(handler.as_ref(), &entry, ENTRY_SUBMISSION_TIMEOUT).await
            {
                error!("failed to record time for {}: {error:#}", entry.side.label);
            }
        }
    });

    let mut final_entries = Vec::new();
    let tracking_result: Result<()> = async {
        loop {
            let side_num = tokio::select! {
                biased;
                _ = wait_for_shutdown(shutdown) => break,
                orientation = orientations.next() => match orientation {
                    Some(side_num) => side_num,
                    None => break,
                },
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
                tokio::select! {
                    biased;
                    _ = wait_for_shutdown(shutdown) => {
                        final_entries.push(entry);
                        break;
                    },
                    queued = queue_entry(&submission_transmitter, &entry) => queued?,
                }
            }
        }
        Ok(())
    }
    .await;

    if let Some(entry) = tracker_state.finish(Local::now()) {
        final_entries.push(entry);
    }

    finish_submissions(
        submission_transmitter,
        submission_worker,
        final_entries,
        SHUTDOWN_GRACE_PERIOD,
    )
    .await?;

    tracking_result
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    while !*shutdown.borrow() {
        if shutdown.changed().await.is_err() {
            break;
        }
    }
}

async fn finish_submissions(
    submissions: mpsc::Sender<config::TimeEntry>,
    mut worker: tokio::task::JoinHandle<()>,
    final_entries: Vec<config::TimeEntry>,
    grace_period: Duration,
) -> Result<()> {
    let result = tokio::time::timeout(grace_period, async {
        for entry in final_entries {
            queue_entry(&submissions, &entry).await?;
        }
        drop(submissions);
        (&mut worker)
            .await
            .context("time-entry submission worker failed")
    })
    .await;

    match result {
        Ok(Ok(())) => Ok(()),
        failure => {
            worker.abort();
            if !worker.is_finished() {
                match worker.await {
                    Ok(()) => {}
                    Err(error) if error.is_cancelled() => {}
                    Err(error) => error!("time-entry submission worker failed: {error}"),
                }
            }
            error!(
                "Submission drain did not complete; remaining queued, final, or in-flight \
                 entries may be lost and will not be replayed. Hackaru may have an unstopped activity."
            );
            match failure {
                Ok(Err(error)) => Err(error),
                Err(_) => anyhow::bail!(
                    "time-entry submission shutdown exceeded the {grace_period:?} grace period"
                ),
                Ok(Ok(())) => unreachable!(),
            }
        }
    }
}

async fn queue_entry(
    submissions: &mpsc::Sender<crate::tracker::config::TimeEntry>,
    entry: &crate::tracker::config::TimeEntry,
) -> Result<()> {
    submissions
        .send(entry.clone())
        .await
        .context("failed to queue time entry for submission")?;
    log_time_spent(entry.end - entry.start, &entry.side.label);
    Ok(())
}

async fn record_entry(
    handler: &dyn Handler,
    entry: &crate::tracker::config::TimeEntry,
) -> Result<()> {
    handler
        .handle(entry)
        .await
        .with_context(|| format!("failed to record time for {}", entry.side.label))
}

async fn record_entry_with_timeout(
    handler: &dyn Handler,
    entry: &crate::tracker::config::TimeEntry,
    timeout: Duration,
) -> Result<()> {
    tokio::time::timeout(timeout, record_entry(handler, entry))
        .await
        .map_err(|_| anyhow::anyhow!("handler did not finish within {timeout:?}"))?
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
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use std::time::Duration;

    use async_trait::async_trait;
    use chrono::TimeDelta;
    use futures::stream;
    use tokio::sync::{mpsc, oneshot, watch, Notify};

    use crate::tracker::config::{Handler, Side, TimeEntry};

    use super::{
        finish_submissions, format_time_spent, process_orientations, queue_entry,
        record_entry_with_timeout, wait_for_shutdown,
    };

    #[derive(Default)]
    struct RecordingHandler {
        entries: Mutex<Vec<TimeEntry>>,
    }

    struct BlockingHandler {
        calls: AtomicUsize,
        entries: Mutex<Vec<TimeEntry>>,
        started: Mutex<Option<oneshot::Sender<()>>>,
        release: Notify,
    }

    struct FailingHandler {
        calls: AtomicUsize,
        successful_entries: Mutex<Vec<TimeEntry>>,
    }

    struct HangingHandler;

    #[async_trait]
    impl Handler for RecordingHandler {
        async fn handle(&self, entry: &TimeEntry) -> anyhow::Result<()> {
            self.entries.lock().unwrap().push(entry.clone());
            Ok(())
        }
    }

    #[async_trait]
    impl Handler for BlockingHandler {
        async fn handle(&self, entry: &TimeEntry) -> anyhow::Result<()> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                if let Some(started) = self.started.lock().unwrap().take() {
                    let _ = started.send(());
                }
                self.release.notified().await;
            }
            self.entries.lock().unwrap().push(entry.clone());
            Ok(())
        }
    }

    #[async_trait]
    impl Handler for FailingHandler {
        async fn handle(&self, entry: &TimeEntry) -> anyhow::Result<()> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                anyhow::bail!("submission failed");
            }
            self.successful_entries.lock().unwrap().push(entry.clone());
            Ok(())
        }
    }

    #[async_trait]
    impl Handler for HangingHandler {
        async fn handle(&self, _entry: &TimeEntry) -> anyhow::Result<()> {
            std::future::pending().await
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
        let handler = Arc::new(RecordingHandler::default());
        let (_shutdown_transmitter, mut shutdown_receiver) = watch::channel(false);

        process_orientations(
            stream::iter([Some(1)]),
            &sides(),
            handler.clone(),
            &mut shutdown_receiver,
        )
        .await
        .unwrap();

        let entries = handler.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].side.label, "Work");
        assert!(entries[0].end >= entries[0].start);
    }

    #[tokio::test]
    async fn repeated_shutdown_flushes_the_active_entry_once() {
        let handler = Arc::new(RecordingHandler::default());
        let (shutdown_transmitter, mut shutdown_receiver) = watch::channel(false);
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
            process_orientations(orientations, &sides(), task_handler, &mut shutdown_receiver).await
        });

        ready_rx.await.unwrap();
        shutdown_transmitter.send_replace(true);
        shutdown_transmitter.send_replace(true);
        task.await.unwrap().unwrap();

        let entries = handler.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].side.label, "Work");
        assert!(entries[0].end >= entries[0].start);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_takes_priority_over_continuous_orientation_notifications() {
        let handler = Arc::new(RecordingHandler::default());
        let (shutdown_transmitter, mut shutdown_receiver) = watch::channel(false);
        let (active_transmitter, active_receiver) = oneshot::channel();
        let orientations = stream::unfold(
            (0, Some(active_transmitter)),
            |(state, active_transmitter)| async move {
                if state == 0 {
                    if let Some(active_transmitter) = active_transmitter {
                        let _ = active_transmitter.send(());
                    }
                } else {
                    tokio::task::yield_now().await;
                }
                Some((Some(1), (state + 1, None)))
            },
        );
        tokio::spawn(async move {
            active_receiver.await.unwrap();
            shutdown_transmitter.send_replace(true);
        });

        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            process_orientations(
                orientations,
                &sides(),
                handler.clone(),
                &mut shutdown_receiver,
            ),
        )
        .await
        .expect("shutdown should not be starved by orientation notifications")
        .unwrap();

        let entries = handler.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].side.label, "Work");
    }

    #[tokio::test]
    async fn stream_end_without_an_active_entry_does_not_call_handler() {
        let handler = Arc::new(RecordingHandler::default());
        let (_shutdown_transmitter, mut shutdown_receiver) = watch::channel(false);

        process_orientations(
            stream::empty(),
            &sides(),
            handler.clone(),
            &mut shutdown_receiver,
        )
        .await
        .unwrap();

        assert!(handler.entries.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn queue_entry_waits_for_capacity_before_reporting_success() {
        let (sender, mut receiver) = mpsc::channel(1);
        let first = crate::test_support::time_entry(1, "First");
        let second = crate::test_support::time_entry(1, "Second");
        sender.send(first.clone()).await.unwrap();

        let queued_entry = second.clone();
        let enqueue_sender = sender.clone();
        let mut enqueue =
            tokio::spawn(async move { queue_entry(&enqueue_sender, &queued_entry).await });
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut enqueue)
                .await
                .is_err(),
            "enqueue should wait while the bounded queue is full"
        );
        assert_eq!(receiver.recv().await.unwrap(), first);
        enqueue.await.unwrap().unwrap();
        assert_eq!(receiver.recv().await.unwrap(), second);
    }

    #[tokio::test]
    async fn handler_timeout_returns_control_to_submission_worker() {
        let error = record_entry_with_timeout(
            &HangingHandler,
            &crate::test_support::time_entry(1, "Work"),
            Duration::from_millis(10),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("handler did not finish"));
    }

    #[tokio::test]
    async fn shutdown_interrupts_an_enqueue_blocked_by_a_full_queue() {
        let (sender, _receiver) = mpsc::channel(1);
        let entry = crate::test_support::time_entry(1, "Work");
        sender.send(entry.clone()).await.unwrap();
        let (shutdown_transmitter, mut shutdown_receiver) = watch::channel(false);
        let pending = entry.clone();
        let task = tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = wait_for_shutdown(&mut shutdown_receiver) => pending,
                result = queue_entry(&sender, &pending) => {
                    result.unwrap();
                    panic!("the full queue should not accept another entry");
                },
            }
        });

        shutdown_transmitter.send_replace(true);
        let unsent = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(unsent, entry);
    }

    #[tokio::test]
    async fn shutdown_grace_includes_waiting_to_enqueue_the_final_entry() {
        let (sender, receiver) = mpsc::channel(1);
        let entry = crate::test_support::time_entry(1, "Work");
        sender.send(entry.clone()).await.unwrap();
        let marker = Arc::new(());
        let task_marker = marker.clone();
        let (started_transmitter, started_receiver) = oneshot::channel();
        let worker = tokio::spawn(async move {
            let _receiver = receiver;
            let _marker = task_marker;
            started_transmitter.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        started_receiver.await.unwrap();

        let error = tokio::time::timeout(
            Duration::from_secs(1),
            finish_submissions(sender, worker, vec![entry], Duration::from_millis(20)),
        )
        .await
        .unwrap()
        .unwrap_err();

        assert!(error.to_string().contains("grace period"));
        assert_eq!(Arc::strong_count(&marker), 1, "worker must be cancelled");
    }

    #[tokio::test]
    async fn shutdown_grace_is_shared_by_all_queued_entries() {
        let (sender, mut receiver) = mpsc::channel(3);
        for _ in 0..3 {
            sender
                .send(crate::test_support::time_entry(1, "Work"))
                .await
                .unwrap();
        }
        let marker = Arc::new(());
        let task_marker = marker.clone();
        let worker = tokio::spawn(async move {
            let _marker = task_marker;
            while receiver.recv().await.is_some() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });

        let error = tokio::time::timeout(
            Duration::from_secs(1),
            finish_submissions(sender, worker, Vec::new(), Duration::from_millis(20)),
        )
        .await
        .unwrap()
        .unwrap_err();

        assert!(error.to_string().contains("grace period"));
        assert_eq!(Arc::strong_count(&marker), 1, "worker must be cancelled");
    }

    #[tokio::test]
    async fn pending_submission_does_not_block_orientation_processing() {
        let (shutdown_transmitter, mut shutdown_receiver) = watch::channel(false);
        let (started_tx, started_rx) = oneshot::channel();
        let (flips_tx, flips_rx) = oneshot::channel();
        let orientations = stream::unfold((0, Some(flips_tx)), |(state, flips_tx)| async move {
            match state {
                0..=2 => Some((Some(if state == 1 { 2 } else { 1 }), (state + 1, flips_tx))),
                _ => {
                    if let Some(flips_tx) = flips_tx {
                        let _ = flips_tx.send(());
                    }
                    std::future::pending().await
                }
            }
        });
        let handler = Arc::new(BlockingHandler {
            calls: AtomicUsize::new(0),
            entries: Mutex::new(Vec::new()),
            started: Mutex::new(Some(started_tx)),
            release: Notify::new(),
        });
        let task_handler = handler.clone();
        let task = tokio::spawn(async move {
            process_orientations(orientations, &sides(), task_handler, &mut shutdown_receiver).await
        });

        started_rx.await.unwrap();
        flips_rx.await.unwrap();
        shutdown_transmitter.send_replace(true);
        handler.release.notify_one();
        task.await.unwrap().unwrap();

        assert_eq!(handler.entries.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_failed_submission_does_not_stop_later_entries() {
        let handler = Arc::new(FailingHandler {
            calls: AtomicUsize::new(0),
            successful_entries: Mutex::new(Vec::new()),
        });
        let (_shutdown_transmitter, mut shutdown_receiver) = watch::channel(false);

        process_orientations(
            stream::iter([Some(1), Some(2), Some(1)]),
            &[
                Side {
                    side_num: 1,
                    label: "Work".into(),
                    configurable: true,
                },
                Side {
                    side_num: 2,
                    label: "Break".into(),
                    configurable: true,
                },
            ],
            handler.clone(),
            &mut shutdown_receiver,
        )
        .await
        .unwrap();

        assert_eq!(handler.calls.load(Ordering::SeqCst), 3);
        assert_eq!(handler.successful_entries.lock().unwrap().len(), 2);
    }
}
