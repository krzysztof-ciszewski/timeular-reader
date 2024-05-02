use std::fs;

use serde::{Deserialize, Serialize};
use toml::{Value, value::Table};

const CONFIG_PATH: &str = "config.toml";
pub trait Config<'de>: Serialize + Deserialize<'de> + Default {}

pub fn get_config<'de, T: Config<'de>>(key: &str) -> T {
    ensure_file_exists();

    let config: Value = toml::from_str(&fs::read_to_string(CONFIG_PATH).unwrap()).unwrap();
    let value = config.get(&key);

    if value.is_none() {
        return initialize_default_config_key::<T>(key);
    }

    value.unwrap().to_owned().try_into::<T>().unwrap()
}

pub fn update_config<'de, T: Config<'de>>(key: &str, config: &T) {
    ensure_file_exists();

    let mut whole_config: Table =
        toml::from_str(&fs::read_to_string(CONFIG_PATH).unwrap()).unwrap();

    whole_config.insert(key.to_string(), Value::try_from(config).unwrap());

    save_config_file(&whole_config);
}

fn initialize_default_config_key<'de, T: Config<'de>>(key: &str) -> T {
    let def_config = T::default();

    update_config(key, &def_config);

    def_config
}

fn save_config_file<T: ?Sized + Serialize>(contents: &T) {
    fs::write(CONFIG_PATH, toml::to_string(&contents).unwrap()).unwrap();
}

fn ensure_file_exists() {
    if fs::metadata(CONFIG_PATH).is_err() {
        fs::write(CONFIG_PATH, "").unwrap();
    }
}
