use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use fs2::FileExt;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const DEDUPLICATION_WINDOW_SECS: u64 = 30;
const RETENTION_SECS: u64 = 7 * 24 * 60 * 60;
const MAX_RETAINED_EVENTS: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    System,
    Activity,
    Notification,
    Browser,
    File,
    Task,
}

impl EventKind {
    fn label(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Activity => "activity",
            Self::Notification => "notification",
            Self::Browser => "browser",
            Self::File => "file",
            Self::Task => "task",
        }
    }
}

pub struct LocalEvent {
    pub kind: EventKind,
    pub summary: String,
    pub detail: String,
}

pub fn record(event: &LocalEvent) -> std::io::Result<()> {
    EventStore::at(event_log_path()).record(event)
}

pub struct EventStore {
    path: PathBuf,
    deduplication_window_secs: u64,
    retention_secs: u64,
    max_events: usize,
}

impl EventStore {
    pub fn at(path: PathBuf) -> Self {
        Self {
            path,
            deduplication_window_secs: DEDUPLICATION_WINDOW_SECS,
            retention_secs: RETENTION_SECS,
            max_events: MAX_RETAINED_EVENTS,
        }
    }

    pub fn record(&self, event: &LocalEvent) -> std::io::Result<()> {
        self.record_at(event, unix_timestamp())
    }

    pub fn prune(&self) -> std::io::Result<()> {
        self.prune_at(unix_timestamp())
    }

    fn record_at(&self, event: &LocalEvent, timestamp: u64) -> std::io::Result<()> {
        let _lock = self.lock()?;
        let mut entries = read_entries(&self.path)?;
        let fingerprint = event_fingerprint(event);
        if entries.iter().rev().any(|entry| {
            entry.fingerprint == fingerprint
                && timestamp.saturating_sub(entry.timestamp) < self.deduplication_window_secs
        }) {
            return Ok(());
        }
        entries.retain(|entry| timestamp.saturating_sub(entry.timestamp) <= self.retention_secs);
        entries.push(StoredEvent::from_event(timestamp, event));
        let keep_from = entries.len().saturating_sub(self.max_events);
        write_entries(&self.path, &entries[keep_from..])
    }

    fn prune_at(&self, timestamp: u64) -> std::io::Result<()> {
        let _lock = self.lock()?;
        let mut entries = read_entries(&self.path)?;
        let original_len = entries.len();
        entries.retain(|entry| timestamp.saturating_sub(entry.timestamp) <= self.retention_secs);
        if entries.len() != original_len {
            let keep_from = entries.len().saturating_sub(self.max_events);
            write_entries(&self.path, &entries[keep_from..])?;
        }
        Ok(())
    }

    fn lock(&self) -> std::io::Result<fs::File> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
            if self.path == event_log_path() {
                set_owner_only_permissions(parent, 0o700)?;
            }
        }
        let lock = open_private_file(&self.path.with_extension("log.lock"))?;
        lock.lock_exclusive()?;
        Ok(lock)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredEvent {
    timestamp: u64,
    kind: String,
    summary: String,
    detail: String,
    fingerprint: String,
}

impl StoredEvent {
    fn from_event(timestamp: u64, event: &LocalEvent) -> Self {
        Self {
            timestamp,
            kind: event.kind.label().to_owned(),
            summary: sanitize(&event.summary),
            detail: sanitize(&event.detail),
            fingerprint: event_fingerprint(event),
        }
    }
}

fn read_entries(path: &Path) -> std::io::Result<Vec<StoredEvent>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    Ok(content
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(4, '\t');
            Some(StoredEvent {
                timestamp: fields.next()?.parse().ok()?,
                kind: fields.next()?.to_owned(),
                summary: fields.next()?.to_owned(),
                detail: fields.next()?.to_owned(),
                fingerprint: String::new(),
            })
        })
        .map(|mut entry| {
            entry.fingerprint = format!(
                "{}\u{1f}{}\u{1f}{}",
                entry.kind, entry.summary, entry.detail
            );
            entry
        })
        .collect())
}

fn write_entries(path: &Path, entries: &[StoredEvent]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("log.{}.tmp", std::process::id()));
    let mut file = open_private_file(&temporary)?;
    for entry in entries {
        writeln!(
            file,
            "{}\t{}\t{}\t{}",
            entry.timestamp, entry.kind, entry.summary, entry.detail
        )?;
    }
    file.sync_all()?;
    fs::rename(temporary, path)?;
    set_owner_only_permissions(path, 0o600)
}

fn open_private_file(path: &Path) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true).read(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path)?;
    set_owner_only_permissions(path, 0o600)?;
    Ok(file)
}

fn set_owner_only_permissions(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

fn event_fingerprint(event: &LocalEvent) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        event.kind.label(),
        sanitize(&event.summary),
        sanitize(&event.detail)
    )
}

fn sanitize(value: &str) -> String {
    value.replace(['\n', '\r', '\t'], " ")
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn event_log_path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_owned()))
                .join(".local/state")
        })
        .join("ikaros/events.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(summary: &str) -> LocalEvent {
        LocalEvent {
            kind: EventKind::Task,
            summary: summary.to_owned(),
            detail: "details".to_owned(),
        }
    }

    #[test]
    fn deduplicates_events_inside_the_window() {
        let path = std::env::temp_dir().join(format!("ikaros-events-{}", std::process::id()));
        let store = EventStore::at(path.clone());
        store.record_at(&event("completed"), 100).unwrap();
        store.record_at(&event("completed"), 120).unwrap();
        assert_eq!(read_entries(&path).unwrap().len(), 1);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn removes_expired_events_before_writing() {
        let path = std::env::temp_dir().join(format!("ikaros-retention-{}", std::process::id()));
        let store = EventStore::at(path.clone());
        store.record_at(&event("old"), 1).unwrap();
        store.record_at(&event("new"), RETENTION_SECS + 2).unwrap();
        let entries = read_entries(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].summary, "new");
        let _ = fs::remove_file(path);
    }
}
