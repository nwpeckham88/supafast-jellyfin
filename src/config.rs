use std::{env, path::PathBuf};

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub tv_base_path: PathBuf,
    pub movies_base_path: PathBuf,
    pub database_url: Option<String>,
    pub audit_log_dir: PathBuf,
    pub port: u16,
    pub scan_ttl_minutes: u64,
    pub follow_symlinks: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, AppError> {
        Ok(Self {
            host: read_env_or_default("HOST", "127.0.0.1"),
            tv_base_path: PathBuf::from(read_env_or_default(
                "TV_BASE_PATH",
                "/mnt/nas/servarr/data/media/tv/",
            )),
            movies_base_path: PathBuf::from(read_env_or_default(
                "MOVIES_BASE_PATH",
                "/mnt/nas/servarr/data/media/movies/",
            )),
            database_url: env::var("DATABASE_URL").ok(),
            audit_log_dir: PathBuf::from(read_env_or_default(
                "AUDIT_LOG_DIR",
                "/var/log/supafast-jellyfin/",
            )),
            port: parse_u16("PORT", 3001)?,
            scan_ttl_minutes: parse_u64("SCAN_TTL_MINUTES", 60)?,
            follow_symlinks: parse_bool("FOLLOW_SYMLINKS", false)?,
        })
    }
}

fn read_env_or_default(key: &'static str, default: &'static str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn parse_u16(key: &'static str, default: u16) -> Result<u16, AppError> {
    let raw = env::var(key).unwrap_or_else(|_| default.to_string());
    raw.parse::<u16>()
        .map_err(|_| AppError::InvalidConfigValue { key, value: raw })
}

fn parse_u64(key: &'static str, default: u64) -> Result<u64, AppError> {
    let raw = env::var(key).unwrap_or_else(|_| default.to_string());
    raw.parse::<u64>()
        .map_err(|_| AppError::InvalidConfigValue { key, value: raw })
}

fn parse_bool(key: &'static str, default: bool) -> Result<bool, AppError> {
    let raw = env::var(key).unwrap_or_else(|_| default.to_string());
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(AppError::InvalidBooleanConfigValue { key, value: raw }),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::AppConfig;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_env_overrides<F>(overrides: &[(&str, Option<&str>)], f: F)
    where
        F: FnOnce(),
    {
        let _guard = env_lock().lock().expect("env lock");
        let mut previous = Vec::with_capacity(overrides.len());

        for (key, value) in overrides {
            previous.push(((*key).to_string(), std::env::var(key).ok()));
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }

        f();

        for (key, value) in previous {
            match value {
                Some(v) => std::env::set_var(&key, v),
                None => std::env::remove_var(&key),
            }
        }
    }

    #[test]
    fn loads_default_values() {
        with_env_overrides(
            &[
                ("PORT", None),
                ("HOST", None),
                ("SCAN_TTL_MINUTES", None),
                ("FOLLOW_SYMLINKS", None),
            ],
            || {
                let config = AppConfig::from_env().expect("config from env");

                assert_eq!(config.host, "127.0.0.1");
                assert_eq!(config.port, 3001);
                assert_eq!(config.scan_ttl_minutes, 60);
                assert!(!config.follow_symlinks);
            },
        );
    }

    #[test]
    fn rejects_invalid_port() {
        with_env_overrides(&[("PORT", Some("not-a-number"))], || {
            let result = AppConfig::from_env();
            assert!(result.is_err());
        });
    }

    #[test]
    fn rejects_invalid_bool() {
        with_env_overrides(&[("FOLLOW_SYMLINKS", Some("sometimes"))], || {
            let result = AppConfig::from_env();
            assert!(result.is_err());
        });
    }
}
