#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub calendar_path: Option<PathBuf>,
    pub watch_directory: Option<PathBuf>,
    pub watched_units: Vec<String>,
    pub routines: Vec<Routine>,
    pub break_reminder_minutes: Option<u64>,
    pub quiet_hours: Option<QuietHours>,
    pub paused_until_unix_secs: Option<u64>,
    pub read_notifications: bool,
    pub focused_app_awareness: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Routine {
    pub name: String,
    pub hour: u8,
    pub minute: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuietHours {
    pub start_hour: u8,
    pub end_hour: u8,
}

impl Settings {
    pub fn load() -> Self {
        fs::read(settings_path())
            .ok()
            .and_then(|json| serde_json::from_slice(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            set_owner_only_permissions(parent, 0o700)?;
        }
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&path)?;
        set_owner_only_permissions(&path, 0o600)?;
        file.write_all(&serde_json::to_vec_pretty(self).expect("settings are serializable"))
    }

    pub fn reactions_paused(&self) -> bool {
        self.paused_until_unix_secs
            .is_some_and(|until| now_unix_secs() < until)
    }

    pub fn pause_for_one_hour(&mut self) {
        self.paused_until_unix_secs = Some(now_unix_secs() + 60 * 60);
    }
}

fn set_owner_only_permissions(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

pub fn settings_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_owned())).join(".config")
        })
        .join("ikaros/settings.json")
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_pause_is_not_active() {
        let settings = Settings {
            paused_until_unix_secs: Some(1),
            ..Settings::default()
        };
        assert!(!settings.reactions_paused());
    }
}
