use std::{collections::HashMap, fs};

use serde::{Deserialize, Serialize};
use toml::Value;

const CONFIG_PATH: &str = "config.toml";

pub trait Config<'de>: Serialize + Deserialize<'de> + Default {}

pub fn create_config<'de, T: Config<'de>>(key: &str) -> T {
    if fs::metadata(CONFIG_PATH).is_err() {
        return initialize_default_config(key);
    }

    let config: Value = toml::from_str(&fs::read_to_string(CONFIG_PATH).unwrap()).unwrap();

    config[key].to_owned().try_into::<T>().unwrap()
}

pub fn update_config<'de, T: Config<'de>>(key: &str, config: &T) {
    let mut whole_config: Value =
        toml::from_str(&fs::read_to_string(CONFIG_PATH).unwrap()).unwrap();

    whole_config[key] = Value::try_from(config).unwrap();

    save_config_file(key, &whole_config);
}

fn initialize_default_config<'de, T: Config<'de>>(key: &str) -> T {
    let def_config = T::default();

    let config = HashMap::from([(key, &def_config)]);

    save_config_file(key, &config);

    def_config
}

fn save_config_file<T: ?Sized + Serialize>(key: &str, contents: &T) {
    fs::write(CONFIG_PATH, toml::to_string(&contents).unwrap()).unwrap();
}
