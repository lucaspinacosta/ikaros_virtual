use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    let path = event_log_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(
        file,
        "{timestamp}\t{}\t{}\t{}",
        event.kind.label(),
        event.summary.replace('\n', " "),
        event.detail.replace('\n', " ")
    )
}

fn event_log_path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_owned()))
                .join(".local/state")
        })
        .join("ikaros/events.log")
}
