//! Local event collector contracts. Each collector emits normalized events into Ikaros.

use std::process::Command;

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
    let session = std::env::var("XDG_SESSION_ID").ok()?;
    let output = Command::new("loginctl")
        .args(["show-session", &session, "-p", "LockedHint", "--value"])
        .output()
        .ok()?;
    match String::from_utf8_lossy(&output.stdout).trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}
