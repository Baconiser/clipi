use std::fs;
use std::path::{Path, PathBuf};

use directories::BaseDirs;
use serde::{Deserialize, Serialize};

const MIN_ENTRIES: u32 = 10;
const MAX_ENTRIES: u32 = 10_000;
const MIN_DB_MB: u32 = 8;
const MAX_DB_MB: u32 = 512;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_max_entries")]
    pub max_entries: u32,
    #[serde(default = "default_max_db_mb")]
    pub max_db_mb: u32,
    #[serde(default)]
    pub db_path: PathBuf,
}

fn default_max_entries() -> u32 {
    500
}

fn default_max_db_mb() -> u32 {
    80
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_entries: default_max_entries(),
            max_db_mb: default_max_db_mb(),
            db_path: default_db_path(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = config_file_path();
        let mut settings = if path.exists() {
            match fs::read_to_string(&path) {
                Ok(raw) => toml::from_str(&raw).unwrap_or_default(),
                Err(_) => Settings::default(),
            }
        } else {
            Settings::default()
        };
        if settings.db_path.as_os_str().is_empty() {
            settings.db_path = default_db_path();
        }
        settings.sanitize();
        if !path.exists() {
            let _ = settings.save();
        }
        settings
    }

    pub fn save(&self) -> Result<(), String> {
        let mut copy = self.clone();
        copy.sanitize();
        let path = config_file_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let body = toml::to_string_pretty(&copy).map_err(|e| e.to_string())?;
        fs::write(path, body).map_err(|e| e.to_string())
    }

    pub fn sanitize(&mut self) {
        self.max_entries = self.max_entries.clamp(MIN_ENTRIES, MAX_ENTRIES);
        self.max_db_mb = self.max_db_mb.clamp(MIN_DB_MB, MAX_DB_MB);
    }
}

pub fn config_file_path() -> PathBuf {
    app_dir().join("config.toml")
}

pub fn default_db_path() -> PathBuf {
    app_dir().join("clipi.db")
}

pub fn app_dir() -> PathBuf {
    if let Some(base) = BaseDirs::new() {
        #[cfg(target_os = "macos")]
        {
            return base.home_dir().join("Library/Application Support/clipi");
        }
        #[cfg(target_os = "windows")]
        {
            return base.config_dir().join("clipi");
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            return base.config_dir().join("clipi");
        }
    }
    PathBuf::from("clipi-data")
}

pub fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(())
}
