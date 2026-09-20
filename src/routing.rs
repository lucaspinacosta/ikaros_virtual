use crate::{
    companion::SpriteLoop,
    events::{EventKind, LocalEvent},
};

/// Maps normalized, privacy-preserving events to short visual reactions.
pub fn animation_for(event: &LocalEvent) -> SpriteLoop {
    match event.kind {
        EventKind::System => SpriteLoop::Warning,
        EventKind::Activity => SpriteLoop::Sleep,
        EventKind::Notification => SpriteLoop::Notification,
        EventKind::Task
            if event.detail == "Calendar event starts soon" || event.detail == "Break reminder" =>
        {
            SpriteLoop::Notification
        }
        EventKind::Task if event.summary == "Task succeeded" => SpriteLoop::Success,
        EventKind::Task if event.summary == "Task started" => SpriteLoop::Working,
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
}
