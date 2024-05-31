extern crate core;

use std::error::Error;
use std::sync::Arc;

use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager, PeripheralId};
use clap::Parser;
use futures::stream::StreamExt;
use log::{debug, LevelFilter};
use simplelog::{info, ColorChoice, ConfigBuilder, TermLogger, TerminalMode};

use crate::tracker::reader;

pub mod config;
pub mod handler;
pub mod tracker;

#[derive(Parser, Debug)]
#[clap(about, long_about = None)]
struct CliArgs {
    #[clap(short, long, action)]
    setup: bool,
    #[clap(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    #[clap(short, long, action)]
    quiet: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_args = CliArgs::parse();

    create_logger(&cli_args.verbose, cli_args.quiet);

    debug!("{}", cli_args.setup);
    let adapter = Arc::new(get_adapter().await);
    let mut events = adapter.events().await?;

    info!("Looking for Timeular Tracker");

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
                spawn_reader(id, &adapter, cli_args.setup);
            }
            CentralEvent::DeviceDisconnected(id) => {
                let per = match adapter.peripheral(&id).await {
                    Ok(per) => per,
                    Err(_e) => continue,
                };
                let name = match get_name(&per).await {
                    Ok(name) => name,
                    Err(_e) => continue,
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

fn spawn_reader(id: PeripheralId, adapter: &Arc<Adapter>, setup: bool) {
    info!("Connecting to tracker...");

    let adapter = adapter.clone();

    tokio::spawn(async move {
        reader::read_tracker(id, adapter, setup).await.unwrap();
    });
}

async fn get_adapter() -> Adapter {
    Manager::new()
        .await
        .unwrap()
        .adapters()
        .await
        .unwrap()
        .into_iter()
        .next()
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

fn create_logger(verbosity: &u8, quiet: bool) {
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
    .unwrap();
}
