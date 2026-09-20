use crate::{
    companion::SpriteLoop,
    events::{EventKind, LocalEvent},
};

/// Maps normalized, privacy-preserving events to short visual reactions.
pub fn animation_for(event: &LocalEvent) -> SpriteLoop {
    match event.kind {
        EventKind::System
            if event.summary.contains("low battery")
                || event.summary.contains("network offline") =>
        {
            SpriteLoop::Emotion
        }
        EventKind::System => SpriteLoop::Warning,
        EventKind::Activity if event.detail == "active" || event.detail == "unlocked" => {
            SpriteLoop::SleepWake
        }
        EventKind::Activity => SpriteLoop::Sleep,
        EventKind::Notification => SpriteLoop::Notification,
        EventKind::Task
            if event.detail == "Calendar event starts soon" || event.detail == "Break reminder" =>
        {
            SpriteLoop::Notification
        }
        EventKind::Task if event.summary == "Task succeeded" => SpriteLoop::Success,
        EventKind::Task if event.summary == "Task failed" => SpriteLoop::Emotion,
        EventKind::Task
            if event.summary == "Task started"
                && (event.detail.contains("build") || event.detail.contains("compile")) =>
        {
            SpriteLoop::Compile
        }
        EventKind::Task if event.summary == "Task started" => SpriteLoop::Working,
        EventKind::Browser if event.summary == "Browser download completed" => SpriteLoop::Success,
        EventKind::File if event.summary == "Download completed" => SpriteLoop::Success,
        EventKind::Browser | EventKind::File | EventKind::Task => SpriteLoop::Presence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_notifications_to_the_notification_cycle() {
        let event = LocalEvent {
            kind: EventKind::Notification,
            summary: String::new(),
            detail: String::new(),
        };
        assert_eq!(animation_for(&event), SpriteLoop::Notification);
    }

    #[test]
    fn routes_low_battery_to_an_emotion() {
        let event = LocalEvent {
            kind: EventKind::System,
            summary: "Ikaros: low battery".to_owned(),
            detail: String::new(),
        };
        assert_eq!(animation_for(&event), SpriteLoop::Emotion);
    }

    #[test]
    fn routes_build_tasks_to_compile_animation() {
        let event = LocalEvent {
            kind: EventKind::Task,
            summary: "Task started".to_owned(),
            detail: "release build".to_owned(),
        };
        assert_eq!(animation_for(&event), SpriteLoop::Compile);
    }
}
