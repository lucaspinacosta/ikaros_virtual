//! Session-bus notification capture. `dbus-monitor` is used so GTK remains the only GUI runtime.

use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc::SyncSender,
    thread,
};

use crate::collectors::NotificationEvent;

pub struct NotificationWatcher {
    _child: Child,
}

impl NotificationWatcher {
    /// Starts a best-effort listener for notifications visible on the current session bus.
    pub fn start(sender: SyncSender<NotificationEvent>) -> std::io::Result<Self> {
        let mut child = Command::new("dbus-monitor")
            .args([
                "--session",
                "interface='org.freedesktop.Notifications',member='Notify'",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take().expect("stdout was piped");
        thread::spawn(move || {
            let mut message = Vec::new();
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.is_empty() {
                    if let Some(event) = parse_notification(&message) {
                        let _ = sender.send(event);
                    }
                    message.clear();
                    continue;
                }
                if line.starts_with("method call") {
                    if let Some(event) = parse_notification(&message) {
                        let _ = sender.send(event);
                    }
                    message.clear();
                }
                message.push(line);
            }
            if let Some(event) = parse_notification(&message) {
                let _ = sender.send(event);
            }
        });
        Ok(Self { _child: child })
    }
}

impl Drop for NotificationWatcher {
    fn drop(&mut self) {
        let _ = self._child.kill();
        let _ = self._child.wait();
    }
}

fn parse_notification(lines: &[String]) -> Option<NotificationEvent> {
    if !lines.iter().any(|line| line.contains("member=Notify")) {
        return None;
    }
    let strings: Vec<_> = lines
        .iter()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("string \"")
                .and_then(|value| value.strip_suffix('"'))
                .map(str::to_owned)
        })
        .collect();
    // Notify(app_name, replaces_id, app_icon, summary, body, actions, hints, timeout).
    Some(NotificationEvent::Received {
        app: strings.first()?.to_owned(),
        summary: strings.get(2)?.to_owned(),
        body: strings.get(3).cloned().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_notify_method_arguments() {
        let lines = [
            "method call sender=:1.1 -> destination=:1.2 path=/org/freedesktop/Notifications; interface=org.freedesktop.Notifications; member=Notify",
            "   string \"chat\"",
            "   uint32 0",
            "   string \"\"",
            "   string \"New message\"",
            "   string \"Hello\"",
        ];
        assert_eq!(
            parse_notification(&lines.map(str::to_owned)),
            Some(NotificationEvent::Received {
                app: "chat".to_owned(),
                summary: "New message".to_owned(),
                body: "Hello".to_owned()
            })
        );
    }
}
