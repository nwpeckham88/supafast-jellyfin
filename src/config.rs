use std::{
    env,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::error::AppError;
use crate::models::execute::MovieRenameMode;

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
    pub movie_rename_mode: MovieRenameMode,
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
            movie_rename_mode: parse_movie_rename_mode("MOVIE_RENAME_MODE", MovieRenameMode::Clean)?,
        })
    }

    pub fn log_startup_diagnostics(&self) {
        tracing::info!(
            host = %self.host,
            port = self.port,
            scan_ttl_minutes = self.scan_ttl_minutes,
            follow_symlinks = self.follow_symlinks,
            movie_rename_mode = ?self.movie_rename_mode,
            tv_base_path = %self.tv_base_path.display(),
            movies_base_path = %self.movies_base_path.display(),
            audit_log_dir = %self.audit_log_dir.display(),
            database_url_set = self.database_url.is_some(),
            database_url = %mask_database_url(self.database_url.as_deref()),
            host_source = %env_source("HOST"),
            port_source = %env_source("PORT"),
            tv_base_path_source = %env_source("TV_BASE_PATH"),
            movies_base_path_source = %env_source("MOVIES_BASE_PATH"),
            audit_log_dir_source = %env_source("AUDIT_LOG_DIR"),
            database_url_source = %env_source("DATABASE_URL"),
            "startup configuration diagnostics"
        );
    }

    pub fn validate_startup_paths(&self) -> Result<(), AppError> {
        validate_path_access("TV_BASE_PATH", &self.tv_base_path, false)?;
        if let Err(error) = assert_directory_writable("TV_BASE_PATH", &self.tv_base_path) {
            tracing::warn!(
                error = %error,
                path = %self.tv_base_path.display(),
                "TV_BASE_PATH is readable but not writable; execute/rollback operations may fail"
            );
        }

        validate_path_access("MOVIES_BASE_PATH", &self.movies_base_path, false)?;
        if let Err(error) = assert_directory_writable("MOVIES_BASE_PATH", &self.movies_base_path)
        {
            tracing::warn!(
                error = %error,
                path = %self.movies_base_path.display(),
                "MOVIES_BASE_PATH is readable but not writable; execute/rollback operations may fail"
            );
        }

        validate_path_access("AUDIT_LOG_DIR", &self.audit_log_dir, true)?;
        Ok(())
    }
}

fn env_source(key: &'static str) -> &'static str {
    if env::var_os(key).is_some() {
        "env"
    } else {
        "default"
    }
}

fn validate_path_access(
    key: &'static str,
    path: &Path,
    require_write: bool,
) -> Result<(), AppError> {
    let metadata = fs::metadata(path).map_err(|error| AppError::StartupValidation {
        key,
        message: format!("{} is not accessible: {error}", path.display()),
    })?;

    if !metadata.is_dir() {
        return Err(AppError::StartupValidation {
            key,
            message: format!("{} is not a directory", path.display()),
        });
    }

    fs::read_dir(path).map_err(|error| AppError::StartupValidation {
        key,
        message: format!("{} is not readable: {error}", path.display()),
    })?;

    if require_write {
        assert_directory_writable(key, path)?;
    }

    Ok(())
}

fn assert_directory_writable(key: &'static str, dir: &Path) -> Result<(), AppError> {
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let probe = dir.join(format!(".supafast_write_probe_{pid}_{nanos}"));

    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
        .map_err(|error| AppError::StartupValidation {
            key,
            message: format!("{} is not writable: {error}", dir.display()),
        })?;

    fs::remove_file(&probe).map_err(|error| AppError::StartupValidation {
        key,
        message: format!(
            "{} probe cleanup failed after write test ({}): {error}",
            dir.display(),
            probe.display()
        ),
    })?;

    Ok(())
}

fn mask_database_url(value: Option<&str>) -> String {
    match value {
        Some(raw) if !raw.trim().is_empty() => {
            if let Some((scheme, _rest)) = raw.split_once("://") {
                return format!("{scheme}://***");
            }
            "***".to_string()
        }
        _ => "not_set".to_string(),
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

fn parse_movie_rename_mode(
    key: &'static str,
    default: MovieRenameMode,
) -> Result<MovieRenameMode, AppError> {
    let raw = env::var(key).unwrap_or_else(|_| match default {
        MovieRenameMode::Clean => "clean".to_string(),
        MovieRenameMode::ProviderIds => "provider_ids".to_string(),
    });

    match raw.to_ascii_lowercase().as_str() {
        "clean" => Ok(MovieRenameMode::Clean),
        "provider_ids" | "provider-ids" | "providerids" => Ok(MovieRenameMode::ProviderIds),
        _ => Err(AppError::InvalidConfigValue { key, value: raw }),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::AppConfig;
    use crate::models::execute::MovieRenameMode;

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
                ("MOVIE_RENAME_MODE", None),
            ],
            || {
                let config = AppConfig::from_env().expect("config from env");

                assert_eq!(config.host, "127.0.0.1");
                assert_eq!(config.port, 3001);
                assert_eq!(config.scan_ttl_minutes, 60);
                assert!(!config.follow_symlinks);
                assert_eq!(config.movie_rename_mode, MovieRenameMode::Clean);
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

    #[test]
    fn rejects_invalid_movie_rename_mode() {
        with_env_overrides(&[("MOVIE_RENAME_MODE", Some("surprise"))], || {
            let result = AppConfig::from_env();
            assert!(result.is_err());
        });
    }

    #[test]
    fn validates_startup_paths_when_directories_are_present() {
        let temp = tempfile::tempdir().expect("tempdir");
        let tv = temp.path().join("tv");
        let movies = temp.path().join("movies");
        let logs = temp.path().join("logs");
        std::fs::create_dir_all(&tv).expect("tv dir");
        std::fs::create_dir_all(&movies).expect("movies dir");
        std::fs::create_dir_all(&logs).expect("logs dir");

        with_env_overrides(
            &[
                ("TV_BASE_PATH", Some(tv.to_str().expect("tv path"))),
                ("MOVIES_BASE_PATH", Some(movies.to_str().expect("movies path"))),
                ("AUDIT_LOG_DIR", Some(logs.to_str().expect("logs path"))),
            ],
            || {
                let config = AppConfig::from_env().expect("config from env");
                assert!(config.validate_startup_paths().is_ok());
            },
        );
    }

    #[test]
    fn rejects_nonexistent_media_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let missing_tv = temp.path().join("missing-tv");
        let movies = temp.path().join("movies");
        let logs = temp.path().join("logs");
        std::fs::create_dir_all(&movies).expect("movies dir");
        std::fs::create_dir_all(&logs).expect("logs dir");

        with_env_overrides(
            &[
                (
                    "TV_BASE_PATH",
                    Some(missing_tv.to_str().expect("missing tv path")),
                ),
                ("MOVIES_BASE_PATH", Some(movies.to_str().expect("movies path"))),
                ("AUDIT_LOG_DIR", Some(logs.to_str().expect("logs path"))),
            ],
            || {
                let config = AppConfig::from_env().expect("config from env");
                let error = config
                    .validate_startup_paths()
                    .expect_err("expected startup validation failure");
                assert!(error.to_string().contains("TV_BASE_PATH"));
            },
        );
    }

    #[test]
    fn rejects_path_when_not_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let tv_file = temp.path().join("tv-file");
        let movies = temp.path().join("movies");
        let logs = temp.path().join("logs");
        std::fs::write(&tv_file, "not-a-dir").expect("tv file");
        std::fs::create_dir_all(&movies).expect("movies dir");
        std::fs::create_dir_all(&logs).expect("logs dir");

        with_env_overrides(
            &[
                ("TV_BASE_PATH", Some(tv_file.to_str().expect("tv file path"))),
                ("MOVIES_BASE_PATH", Some(movies.to_str().expect("movies path"))),
                ("AUDIT_LOG_DIR", Some(logs.to_str().expect("logs path"))),
            ],
            || {
                let config = AppConfig::from_env().expect("config from env");
                let error = config
                    .validate_startup_paths()
                    .expect_err("expected startup validation failure");
                assert!(error.to_string().contains("not a directory"));
            },
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_writable_audit_log_directory() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let tv = temp.path().join("tv");
        let movies = temp.path().join("movies");
        let logs = temp.path().join("logs");
        std::fs::create_dir_all(&tv).expect("tv dir");
        std::fs::create_dir_all(&movies).expect("movies dir");
        std::fs::create_dir_all(&logs).expect("logs dir");

        let original_permissions = std::fs::metadata(&logs)
            .expect("logs metadata")
            .permissions();
        let mut read_only = original_permissions.clone();
        read_only.set_mode(0o555);
        std::fs::set_permissions(&logs, read_only).expect("set logs permissions");

        // Some CI/container filesystems can still permit writes even after chmod(0555).
        // If that happens, skip instead of producing a flaky failure.
        if super::assert_directory_writable("AUDIT_LOG_DIR", &logs).is_ok() {
            std::fs::set_permissions(&logs, original_permissions).expect("restore logs permissions");
            return;
        }

        with_env_overrides(
            &[
                ("TV_BASE_PATH", Some(tv.to_str().expect("tv path"))),
                ("MOVIES_BASE_PATH", Some(movies.to_str().expect("movies path"))),
                ("AUDIT_LOG_DIR", Some(logs.to_str().expect("logs path"))),
            ],
            || {
                let config = AppConfig::from_env().expect("config from env");
                let error = config
                    .validate_startup_paths()
                    .expect_err("expected startup validation failure");
                assert!(error.to_string().contains("AUDIT_LOG_DIR"));
                assert!(error.to_string().contains("not writable"));
            },
        );

        std::fs::set_permissions(&logs, original_permissions).expect("restore logs permissions");
    }

    #[cfg(unix)]
    #[test]
    fn allows_read_only_media_paths_when_audit_log_dir_is_writable() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let tv = temp.path().join("tv");
        let movies = temp.path().join("movies");
        let logs = temp.path().join("logs");
        std::fs::create_dir_all(&tv).expect("tv dir");
        std::fs::create_dir_all(&movies).expect("movies dir");
        std::fs::create_dir_all(&logs).expect("logs dir");

        let tv_original_permissions = std::fs::metadata(&tv)
            .expect("tv metadata")
            .permissions();
        let movies_original_permissions = std::fs::metadata(&movies)
            .expect("movies metadata")
            .permissions();

        let mut read_only = tv_original_permissions.clone();
        read_only.set_mode(0o555);
        std::fs::set_permissions(&tv, read_only.clone()).expect("set tv permissions");
        std::fs::set_permissions(&movies, read_only).expect("set movies permissions");

        with_env_overrides(
            &[
                ("TV_BASE_PATH", Some(tv.to_str().expect("tv path"))),
                ("MOVIES_BASE_PATH", Some(movies.to_str().expect("movies path"))),
                ("AUDIT_LOG_DIR", Some(logs.to_str().expect("logs path"))),
            ],
            || {
                let config = AppConfig::from_env().expect("config from env");
                assert!(config.validate_startup_paths().is_ok());
            },
        );

        std::fs::set_permissions(&tv, tv_original_permissions).expect("restore tv permissions");
        std::fs::set_permissions(&movies, movies_original_permissions)
            .expect("restore movies permissions");
    }

    #[test]
    fn masks_database_url_credentials_in_diagnostics() {
        let masked = super::mask_database_url(Some(
            "postgres://supafast:super-secret@db.internal:5432/supafast_jellyfin",
        ));
        assert_eq!(masked, "postgres://***");
        assert!(!masked.contains("super-secret"));
    }

    #[test]
    fn reports_database_url_as_not_set_when_missing() {
        let masked = super::mask_database_url(None);
        assert_eq!(masked, "not_set");
    }
}
