//! Local event collectors only inspect sources explicitly selected by the user.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

use crate::events::{EventKind, LocalEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    Locked,
    Unlocked,
    IdleStarted,
    IdleEnded,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileEvent {
    Created(String),
    Modified(String),
    DownloadCompleted(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEvent {
    Started(String),
    Succeeded(String),
    Failed(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationEvent {
    Received {
        app: String,
        summary: String,
        body: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserEvent {
    TabChanged { title: String, url: String },
    DownloadCompleted(String),
    MediaChanged { playing: bool },
}

pub fn file_event(event: FileEvent) -> LocalEvent {
    let (summary, detail) = match event {
        FileEvent::Created(path) => ("File created", path),
        FileEvent::Modified(path) => ("File modified", path),
        FileEvent::DownloadCompleted(path) => ("Download completed", path),
    };
    LocalEvent {
        kind: EventKind::File,
        summary: summary.to_owned(),
        detail,
    }
}

pub fn notification_event(event: NotificationEvent) -> LocalEvent {
    match event {
        NotificationEvent::Received { app, summary, body } => LocalEvent {
            kind: EventKind::Notification,
            summary: format!("{app}: {summary}"),
            detail: body,
        },
    }
}

pub fn browser_event(event: BrowserEvent) -> LocalEvent {
    let (summary, detail) = match event {
        BrowserEvent::TabChanged { title, url } => {
            ("Browser tab changed", format!("{title} ({url})"))
        }
        BrowserEvent::DownloadCompleted(path) => ("Browser download completed", path),
        BrowserEvent::MediaChanged { playing } => (
            "Browser media changed",
            if playing { "playing" } else { "paused" }.to_owned(),
        ),
    };
    LocalEvent {
        kind: EventKind::Browser,
        summary: summary.to_owned(),
        detail,
    }
}

#[derive(Debug, Default)]
pub struct IdleWatcher {
    idle: Option<bool>,
}

impl IdleWatcher {
    pub fn poll(&mut self, idle: Option<bool>) -> Option<SessionEvent> {
        let current = idle?;
        let previous = self.idle.replace(current);
        match (previous, current) {
            (Some(false), true) => Some(SessionEvent::IdleStarted),
            (Some(true), false) => Some(SessionEvent::IdleEnded),
            _ => None,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.idle.unwrap_or(false)
    }
}

pub fn session_idle() -> Option<bool> {
    session_property("IdleHint")
}

pub fn session_event(event: SessionEvent) -> LocalEvent {
    let detail = match event {
        SessionEvent::Locked => "locked",
        SessionEvent::Unlocked => "unlocked",
        SessionEvent::IdleStarted => "idle",
        SessionEvent::IdleEnded => "active",
    };
    LocalEvent {
        kind: EventKind::Activity,
        summary: "Session state".to_owned(),
        detail: detail.to_owned(),
    }
}

pub fn task_event(event: TaskEvent) -> LocalEvent {
    let (summary, detail) = match event {
        TaskEvent::Started(name) => ("Task started", name),
        TaskEvent::Succeeded(name) => ("Task succeeded", name),
        TaskEvent::Failed(name) => ("Task failed", name),
    };
    LocalEvent {
        kind: EventKind::Task,
        summary: summary.to_owned(),
        detail,
    }
}

pub fn session_locked() -> Option<bool> {
    session_property("LockedHint")
}

fn session_property(property: &str) -> Option<bool> {
    let session = std::env::var("XDG_SESSION_ID").ok()?;
    let output = Command::new("loginctl")
        .args(["show-session", &session, "-p", property, "--value"])
        .output()
        .ok()?;
    match String::from_utf8_lossy(&output.stdout).trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileState {
    modified: Option<SystemTime>,
    size: u64,
    stable_polls: u8,
}

/// Polls one user-approved directory. A download completes after two unchanged polls.
pub struct FileWatcher {
    directory: PathBuf,
    files: HashMap<PathBuf, FileState>,
    primed: bool,
}

impl FileWatcher {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            files: HashMap::new(),
            primed: false,
        }
    }

    pub fn poll(&mut self) -> Vec<FileEvent> {
        let mut current = HashMap::new();
        let mut events = Vec::new();
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return events;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() || is_partial_download(&path) {
                continue;
            }
            let state = FileState {
                modified: metadata.modified().ok(),
                size: metadata.len(),
                stable_polls: 0,
            };
            match self.files.get(&path) {
                None => events.push(FileEvent::Created(display_path(&path))),
                Some(previous)
                    if previous.modified != state.modified || previous.size != state.size =>
                {
                    events.push(FileEvent::Modified(display_path(&path)));
                }
                Some(previous) if previous.stable_polls == 1 => {
                    events.push(FileEvent::DownloadCompleted(display_path(&path)));
                }
                _ => {}
            }
            let stable_polls = self.files.get(&path).map_or(0, |previous| {
                if previous.modified == state.modified && previous.size == state.size {
                    previous.stable_polls.saturating_add(1)
                } else {
                    0
                }
            });
            current.insert(
                path,
                FileState {
                    stable_polls,
                    ..state
                },
            );
        }
        self.files = current;
        if !self.primed {
            self.primed = true;
            events.clear();
        }
        events
    }
}

fn is_partial_download(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        matches!(
            extension.to_string_lossy().as_ref(),
            "part" | "crdownload" | "tmp"
        )
    })
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub trait TaskCompletionAdapter {
    fn poll(&mut self) -> Vec<TaskEvent>;
}

pub struct SystemdUnitWatcher {
    units: HashMap<String, Option<String>>,
}

impl SystemdUnitWatcher {
    pub fn new(unit_names: impl IntoIterator<Item = String>) -> Self {
        Self {
            units: unit_names.into_iter().map(|unit| (unit, None)).collect(),
        }
    }
}

impl TaskCompletionAdapter for SystemdUnitWatcher {
    fn poll(&mut self) -> Vec<TaskEvent> {
        let mut events = Vec::new();
        for (unit, previous) in &mut self.units {
            let current = Command::new("systemctl")
                .args(["--user", "is-active", unit])
                .output()
                .ok()
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
            let Some(current) = current else { continue };
            match (previous.as_deref(), current.as_str()) {
                (Some("active"), "inactive") => events.push(TaskEvent::Succeeded(unit.clone())),
                (_, "failed") if previous.as_deref() != Some("failed") => {
                    events.push(TaskEvent::Failed(unit.clone()))
                }
                (_, "active") if previous.as_deref() != Some("active") => {
                    events.push(TaskEvent::Started(unit.clone()))
                }
                _ => {}
            }
            *previous = Some(current);
        }
        events
    }
}

pub struct ProcessTaskAdapter {
    child: std::process::Child,
    task_name: String,
    reported_started: bool,
    completed: bool,
}

impl ProcessTaskAdapter {
    /// Observes a process started through an independently approved action.
    pub fn new(task_name: impl Into<String>, child: std::process::Child) -> Self {
        Self {
            child,
            task_name: task_name.into(),
            reported_started: false,
            completed: false,
        }
    }
}

impl TaskCompletionAdapter for ProcessTaskAdapter {
    fn poll(&mut self) -> Vec<TaskEvent> {
        if self.completed {
            return Vec::new();
        }
        if !self.reported_started {
            self.reported_started = true;
            return vec![TaskEvent::Started(self.task_name.clone())];
        }
        match self.child.try_wait() {
            Ok(Some(status)) if status.success() => {
                self.completed = true;
                vec![TaskEvent::Succeeded(self.task_name.clone())]
            }
            Ok(Some(_)) | Err(_) => {
                self.completed = true;
                vec![TaskEvent::Failed(self.task_name.clone())]
            }
            Ok(None) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_watcher_only_emits_transitions() {
        let mut watcher = IdleWatcher::default();
        assert_eq!(watcher.poll(Some(false)), None);
        assert_eq!(watcher.poll(Some(true)), Some(SessionEvent::IdleStarted));
        assert_eq!(watcher.poll(Some(true)), None);
        assert_eq!(watcher.poll(Some(false)), Some(SessionEvent::IdleEnded));
    }

    #[test]
    fn browser_events_do_not_drop_the_explicit_source() {
        let event = browser_event(BrowserEvent::DownloadCompleted("report.pdf".to_owned()));
        assert_eq!(event.kind, EventKind::Browser);
        assert_eq!(event.summary, "Browser download completed");
    }

    #[test]
    fn watcher_reports_a_new_stable_file_as_a_completed_download() {
        let directory = std::env::temp_dir().join(format!(
            "ikaros-watcher-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let mut watcher = FileWatcher::new(directory.clone());
        assert!(watcher.poll().is_empty());
        let file = directory.join("report.pdf");
        fs::write(&file, b"done").unwrap();
        assert!(matches!(watcher.poll().as_slice(), [FileEvent::Created(_)]));
        assert!(watcher.poll().is_empty());
        assert!(matches!(
            watcher.poll().as_slice(),
            [FileEvent::DownloadCompleted(path)] if path.ends_with("report.pdf")
        ));
        fs::remove_dir_all(directory).unwrap();
    }
}
