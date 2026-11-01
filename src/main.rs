extern crate core;

use std::sync::Arc;
use std::{collections::HashSet, hash::Hash, path::Path};

use anyhow::{Context as _, Result};
use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager, PeripheralId};
use clap::Parser;
use futures::stream::StreamExt;
use log::{debug, error, warn, LevelFilter};
use simplelog::{info, ColorChoice, ConfigBuilder, TermLogger, TerminalMode};
use tokio::{sync::watch, task::JoinHandle};

use crate::tracker::reader;

pub mod config;
pub mod handler;
#[cfg(test)]
mod test_support;
pub mod tracker;

enum DeviceEvent<Id> {
    Discovered(Id),
    TrackerIdentified(Id),
    Disconnected(Id),
}

enum DeviceEventAction<Id> {
    Inspect(Id),
    StartReader(Id),
    Stop,
    Ignore,
}

struct TrackedDevices<Id> {
    ids: HashSet<Id>,
}

impl<Id: Eq + Hash + Clone> TrackedDevices<Id> {
    fn new() -> Self {
        Self {
            ids: HashSet::new(),
        }
    }

    fn contains(&self, id: &Id) -> bool {
        self.ids.contains(id)
    }

    fn register(&mut self, id: Id) -> bool {
        self.ids.insert(id)
    }

    fn handle_event(&mut self, event: DeviceEvent<Id>) -> DeviceEventAction<Id> {
        match event {
            DeviceEvent::Discovered(id) if self.contains(&id) => DeviceEventAction::Ignore,
            DeviceEvent::Discovered(id) => DeviceEventAction::Inspect(id),
            DeviceEvent::TrackerIdentified(id) if self.register(id.clone()) => {
                DeviceEventAction::StartReader(id)
            }
            DeviceEvent::TrackerIdentified(_) => DeviceEventAction::Ignore,
            DeviceEvent::Disconnected(id) if self.ids.remove(&id) => DeviceEventAction::Stop,
            DeviceEvent::Disconnected(_) => DeviceEventAction::Ignore,
        }
    }
}

#[derive(Parser, Debug)]
#[clap(about, long_about = None)]
struct CliArgs {
    /// Run setup even if config.toml already exists.
    #[clap(short, long, action)]
    setup: bool,
    #[clap(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    #[clap(short, long, action)]
    quiet: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args = CliArgs::parse();

    create_logger(&cli_args.verbose, cli_args.quiet)?;

    let config_path = config::get_config_path()?;
    let setup = should_setup(cli_args.setup, &config_path)
        .with_context(|| format!("failed to check config file {}", config_path.display()))?;
    debug!("{}", setup);
    let adapter = Arc::new(get_adapter().await?);
    let mut events = adapter.events().await?;

    info!("Looking for Timeular Tracker");

    adapter.start_scan(ScanFilter::default()).await?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut readers: Vec<JoinHandle<()>> = Vec::new();
    let mut tracked_devices = TrackedDevices::new();
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    let mut shutdown_error = None;

    loop {
        tokio::select! {
            signal_result = &mut ctrl_c => {
                match signal_result {
                    Ok(()) => info!("Shutdown requested"),
                    Err(error) => {
                        shutdown_error = Some(anyhow::Error::new(error).context("failed to listen for Ctrl+C"));
                    }
                }
                break;
            }
            event = events.next() => {
                let Some(event) = event else {
                    break;
                };
                let action = match event {
                    CentralEvent::DeviceDiscovered(id) => {
                        tracked_devices.handle_event(DeviceEvent::Discovered(id))
                    }
                    CentralEvent::DeviceDisconnected(id) => {
                        tracked_devices.handle_event(DeviceEvent::Disconnected(id))
                    }
                    _ => continue,
                };
                match action {
                    DeviceEventAction::Inspect(id) => {
                        let per = match adapter.peripheral(&id).await {
                            Ok(per) => per,
                            Err(error) => {
                                warn!("Failed to inspect discovered Bluetooth device: {error}");
                                continue;
                            }
                        };
                        let name = match get_name(&per).await {
                            Ok(per) => per,
                            Err(error) => {
                                warn!("Failed to read discovered Bluetooth device name: {error:#}");
                                continue;
                            }
                        };
                        if !name.to_lowercase().contains("timeular") {
                            continue;
                        }
                        if let DeviceEventAction::StartReader(id) =
                            tracked_devices.handle_event(DeviceEvent::TrackerIdentified(id))
                        {
                            readers.push(spawn_reader(id, &adapter, setup, shutdown_rx.clone()));
                        }
                    }
                    DeviceEventAction::StartReader(_) => {
                        unreachable!("reader starts are only requested after tracker identification")
                    }
                    DeviceEventAction::Stop => {
                        info!("Tracker disconnected");
                        break;
                    }
                    DeviceEventAction::Ignore => {}
                }
            }
        }
    }

    shutdown_tx.send_replace(true);
    for reader in readers {
        if let Err(error) = reader.await {
            error!("Tracker reader task failed: {error}");
        }
    }

    match shutdown_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn should_setup(requested: bool, config_path: &Path) -> std::io::Result<bool> {
    Ok(requested || !config_path.try_exists()?)
}

fn spawn_reader(
    id: PeripheralId,
    adapter: &Arc<Adapter>,
    setup: bool,
    shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    info!("Connecting to tracker...");

    let adapter = adapter.clone();

    tokio::spawn(async move {
        if let Err(error) = reader::read_tracker(id, adapter, setup, shutdown).await {
            error!("Tracker reader stopped: {error:#}");
        }
    })
}

async fn get_adapter() -> Result<Adapter> {
    let adapters = Manager::new()
        .await
        .context("failed to initialize Bluetooth manager")?
        .adapters()
        .await
        .context("failed to enumerate Bluetooth adapters")?;
    adapters.into_iter().next().context(
        "no Bluetooth adapter found; make sure Bluetooth is enabled and an adapter is available",
    )
}

async fn get_name(per: &impl Peripheral) -> Result<String> {
    let properties = per
        .properties()
        .await
        .context("failed to read Bluetooth device properties")?
        .context("Bluetooth device has no properties")?;
    properties
        .local_name
        .context("Bluetooth device has no local name")
}

fn create_logger(verbosity: &u8, quiet: bool) -> Result<()> {
    let mut config_builder = ConfigBuilder::default();
    let mut level_filter = LevelFilter::Info;

    match verbosity {
        0 => {}
        1 => {
            config_builder.add_filter_allow(String::from("timeular_reader"));
            config_builder.add_filter_allow(String::from("reqwest"));
            level_filter = LevelFilter::Debug;
        }
        _ => {
            level_filter = LevelFilter::Trace;
        }
    }
    if quiet {
        level_filter = LevelFilter::Off;
    }

    TermLogger::init(
        level_filter,
        config_builder.build(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .context("failed to initialize application logger")
}

#[cfg(test)]
mod tests {
    use super::{should_setup, DeviceEvent, DeviceEventAction, TrackedDevices};
    use std::{env, fs, time::SystemTime};

    #[test]
    fn known_disconnect_stops_tracking_and_repeated_disconnect_is_ignored() {
        let mut devices = TrackedDevices::new();
        assert!(devices.register(42));

        assert!(matches!(
            devices.handle_event(DeviceEvent::Disconnected(42)),
            DeviceEventAction::Stop
        ));
        assert!(matches!(
            devices.handle_event(DeviceEvent::Disconnected(42)),
            DeviceEventAction::Ignore
        ));
    }

    #[test]
    fn unrelated_disconnect_is_ignored() {
        let mut devices = TrackedDevices::new();
        assert!(devices.register(42));

        assert!(matches!(
            devices.handle_event(DeviceEvent::Disconnected(7)),
            DeviceEventAction::Ignore
        ));
        assert!(devices.contains(&42));
    }

    #[test]
    fn repeated_discovery_starts_only_one_reader_per_device() {
        let mut devices = TrackedDevices::new();
        let mut readers_started = 0;

        for id in [42, 42, 7, 42, 7] {
            if let DeviceEventAction::Inspect(id) =
                devices.handle_event(DeviceEvent::Discovered(id))
            {
                if matches!(
                    devices.handle_event(DeviceEvent::TrackerIdentified(id)),
                    DeviceEventAction::StartReader(_)
                ) {
                    readers_started += 1;
                }
            }
        }

        assert_eq!(readers_started, 2);
    }

    #[test]
    fn disconnect_handling_does_not_request_device_inspection() {
        let mut devices = TrackedDevices::new();
        assert!(devices.register(42));
        let mut peripheral_lookups = 0;
        let mut name_lookups = 0;

        let action = devices.handle_event(DeviceEvent::Disconnected(42));
        if matches!(action, DeviceEventAction::Inspect(_)) {
            peripheral_lookups += 1;
            name_lookups += 1;
        }

        assert!(matches!(action, DeviceEventAction::Stop));
        assert_eq!(peripheral_lookups, 0);
        assert_eq!(name_lookups, 0);
    }

    #[test]
    fn setup_defaults_to_missing_config_and_can_be_forced() {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let config_path = env::temp_dir().join(format!(
            "timeular-reader-config-{}-{unique}.toml",
            std::process::id()
        ));

        assert!(should_setup(false, &config_path).unwrap());
        assert!(should_setup(true, &config_path).unwrap());
        assert!(!config_path.try_exists().unwrap());

        fs::write(&config_path, "").unwrap();
        let automatic = should_setup(false, &config_path);
        let explicit = should_setup(true, &config_path);
        fs::remove_file(&config_path).unwrap();

        assert!(!automatic.unwrap());
        assert!(explicit.unwrap());
    }
}
