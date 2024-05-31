use log::debug;
use std::{env, fs};

use serde::{Deserialize, Serialize};
use simplelog::info;
use toml::{value::Table, Value};

const CONFIG_FILENAME: &str = "config.toml";

static mut CONFIG_PATH: String = String::new();
pub trait Config<'de>: Serialize + Deserialize<'de> + Default {}

pub fn get_config<'de, T: Config<'de>>(key: &str) -> T {
    ensure_file_exists();

    let config: Value = toml::from_str(&fs::read_to_string(get_config_path()).unwrap()).unwrap();
    let value = config.get(&key);

    if value.is_none() {
        return initialize_default_config_key::<T>(key);
    }

    value.unwrap().to_owned().try_into::<T>().unwrap()
}

pub fn update_config<'de, T: Config<'de>>(key: &str, config: &T) {
    ensure_file_exists();

    let mut whole_config: Table =
        toml::from_str(&fs::read_to_string(get_config_path()).unwrap()).unwrap();

    whole_config.insert(key.to_string(), Value::try_from(config).unwrap());

    save_config_file(&whole_config);
    info!("Config updated");
}

fn get_config_path() -> String {
    unsafe {
        if CONFIG_PATH.is_empty() {
            let mut path = env::current_exe()
                .expect("Executable should have a path")
                .parent()
                .unwrap()
                .to_path_buf();
            path.push(CONFIG_FILENAME);
            CONFIG_PATH = path.to_str().unwrap().to_string();
            debug!("config path: \"{}\"", CONFIG_PATH);
        }

        CONFIG_PATH.clone()
    }
}

fn initialize_default_config_key<'de, T: Config<'de>>(key: &str) -> T {
    let def_config = T::default();

    update_config(key, &def_config);

    def_config
}

fn save_config_file<T: ?Sized + Serialize>(contents: &T) {
    fs::write(get_config_path(), toml::to_string(&contents).unwrap()).unwrap();
}

fn ensure_file_exists() {
    if fs::metadata(get_config_path()).is_err() {
        fs::write(get_config_path(), "").unwrap();
    }
}
