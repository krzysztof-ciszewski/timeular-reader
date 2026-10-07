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
    get_config_from_path(&path, key)
}

fn get_config_from_path<'de, T: Config<'de>>(path: &PathBuf, key: &str) -> Result<T> {
    ensure_file_exists(path)?;

    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let config: Value = toml::from_str(&contents)
        .with_context(|| format!("failed to parse config file {}", path.display()))?;

    if let Some(value) = config.get(key) {
        value
            .to_owned()
            .try_into::<T>()
            .with_context(|| format!("invalid {key} configuration in {}", path.display()))
    } else {
        initialize_default_config_key::<T>(path, key)
    }
}

pub fn update_config<'de, T: Config<'de>>(key: &str, config: &T) -> Result<()> {
    let path = get_config_path()?;
    update_config_at_path(&path, key, config)
}

fn update_config_at_path<T: Serialize>(path: &PathBuf, key: &str, config: &T) -> Result<()> {
    ensure_file_exists(path)?;

    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let mut whole_config: Table = toml::from_str(&contents)
        .with_context(|| format!("failed to parse config file {}", path.display()))?;

    whole_config.insert(
        key.to_string(),
        Value::try_from(config).context("failed to serialize configuration")?,
    );

    save_config_file(path, &whole_config)?;
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

fn initialize_default_config_key<'de, T: Config<'de>>(path: &PathBuf, key: &str) -> Result<T> {
    let def_config = T::default();

    update_config_at_path(path, key, &def_config)?;

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

#[cfg(test)]
mod tests {
    use super::{get_config_from_path, update_config_at_path, Config};
    use serde::{Deserialize, Serialize};
    use std::{env, fs, path::PathBuf, time::SystemTime};

    #[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
    struct TestConfig {
        name: String,
        count: u32,
    }

    impl<'de> Config<'de> for TestConfig {}

    fn temporary_config_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!(
            "timeular-reader-config-{}-{unique}.toml",
            std::process::id()
        ))
    }

    #[test]
    fn initializes_missing_config_and_round_trips_updates() {
        let path = temporary_config_path();

        let initial = get_config_from_path::<TestConfig>(&path, "service").unwrap();
        assert_eq!(initial, TestConfig::default());
        let written: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["service"]["name"].as_str(), Some(""));
        assert_eq!(written["service"]["count"].as_integer(), Some(0));

        let updated = TestConfig {
            name: "work".into(),
            count: 7,
        };
        update_config_at_path(&path, "service", &updated).unwrap();
        assert_eq!(
            get_config_from_path::<TestConfig>(&path, "service").unwrap(),
            updated
        );

        let other: TestConfig = TestConfig {
            name: "untouched".into(),
            count: 3,
        };
        update_config_at_path(&path, "other", &other).unwrap();
        update_config_at_path(&path, "service", &updated).unwrap();
        assert_eq!(
            get_config_from_path::<TestConfig>(&path, "other").unwrap(),
            other
        );

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reports_malformed_configuration() {
        let path = temporary_config_path();
        fs::write(&path, "[service\nname = \"broken\"").unwrap();

        let error = get_config_from_path::<TestConfig>(&path, "service").unwrap_err();
        assert!(format!("{error:#}").contains("failed to parse config file"));

        fs::remove_file(path).unwrap();
    }
}
