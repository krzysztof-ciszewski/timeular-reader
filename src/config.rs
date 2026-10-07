use log::debug;
use std::{env, fs, path::PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use simplelog::info;
use toml::{value::Table, Value};

const CONFIG_FILENAME: &str = "config.toml";

pub trait Config<'de>: Serialize + Deserialize<'de> + Default {}

pub fn get_config<'de, T: Config<'de>>(key: &str) -> Result<T> {
    let path = get_config_path()?;
    ensure_file_exists(&path)?;

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let config: Value = toml::from_str(&contents)
        .with_context(|| format!("failed to parse config file {}", path.display()))?;

    if let Some(value) = config.get(key) {
        value
            .to_owned()
            .try_into::<T>()
            .with_context(|| format!("invalid {key} configuration in {}", path.display()))
    } else {
        initialize_default_config_key::<T>(key)
    }
}

pub fn update_config<'de, T: Config<'de>>(key: &str, config: &T) -> Result<()> {
    let path = get_config_path()?;
    ensure_file_exists(&path)?;

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let mut whole_config: Table = toml::from_str(&contents)
        .with_context(|| format!("failed to parse config file {}", path.display()))?;

    whole_config.insert(
        key.to_string(),
        Value::try_from(config).context("failed to serialize configuration")?,
    );

    save_config_file(&path, &whole_config)?;
    info!("Config updated");
    Ok(())
}

pub(crate) fn get_config_path() -> Result<PathBuf> {
    let executable = env::current_exe().context("failed to determine executable path")?;
    let directory = executable
        .parent()
        .context("executable path has no parent directory")?;
    let path = directory.join(CONFIG_FILENAME);
    debug!("config path: \"{}\"", path.display());
    Ok(path)
}

fn initialize_default_config_key<'de, T: Config<'de>>(key: &str) -> Result<T> {
    let def_config = T::default();

    update_config(key, &def_config)?;

    Ok(def_config)
}

fn save_config_file<T: ?Sized + Serialize>(path: &PathBuf, contents: &T) -> Result<()> {
    let serialized = toml::to_string(contents).context("failed to serialize config file")?;
    fs::write(path, serialized)
        .with_context(|| format!("failed to write config file {}", path.display()))
}

fn ensure_file_exists(path: &PathBuf) -> Result<()> {
    match fs::metadata(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::write(path, "")
            .with_context(|| format!("failed to create config file {}", path.display())),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect config file {}", path.display()))
        }
    }
}
