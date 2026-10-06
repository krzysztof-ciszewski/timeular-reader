extern crate core;

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager, PeripheralId};
use clap::Parser;
use futures::stream::StreamExt;
use log::{debug, error, warn, LevelFilter};
use simplelog::{info, ColorChoice, ConfigBuilder, TermLogger, TerminalMode};

use crate::tracker::reader;

pub mod config;
pub mod handler;
pub mod tracker;

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

    while let Some(event) = events.next().await {
        match event {
            CentralEvent::DeviceDiscovered(id) => {
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
                spawn_reader(id, &adapter, setup);
            }
            CentralEvent::DeviceDisconnected(id) => {
                let per = match adapter.peripheral(&id).await {
                    Ok(per) => per,
                    Err(error) => {
                        warn!("Failed to inspect disconnected Bluetooth device: {error}");
                        continue;
                    }
                };
                let name = match get_name(&per).await {
                    Ok(name) => name,
                    Err(error) => {
                        warn!("Failed to read disconnected Bluetooth device name: {error:#}");
                        continue;
                    }
                };

                if !name.to_lowercase().contains("timeular") {
                    continue;
                }

                info!("Tracker disconnected");
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

fn should_setup(requested: bool, config_path: &Path) -> std::io::Result<bool> {
    Ok(requested || !config_path.try_exists()?)
}

fn spawn_reader(id: PeripheralId, adapter: &Arc<Adapter>, setup: bool) {
    info!("Connecting to tracker...");

    let adapter = adapter.clone();

    tokio::spawn(async move {
        if let Err(error) = reader::read_tracker(id, adapter, setup).await {
            error!("Tracker reader stopped: {error:#}");
        }
    });
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
    use super::should_setup;
    use std::{env, fs, time::SystemTime};

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
