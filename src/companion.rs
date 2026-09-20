use std::{fs, time::Duration};

const MEMORY_ALERT_THRESHOLD_PERCENT: u8 = 90;
const MEMORY_ALERT_COOLDOWN: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perch {
    BottomLeft,
    BottomRight,
}

impl Perch {
    pub fn label(self) -> &'static str {
        match self {
            Self::BottomLeft => "the bottom-left corner",
            Self::BottomRight => "the bottom-right corner",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpriteLoop {
    Perch,
    Blink,
    Sleep,
    WalkLeft,
    WalkRight,
    Fly,
    Alert,
    Flinch,
    Idle,
    Party,
    Working,
    Thinking,
    Warning,
    Charging,
    Presence,
    Success,
    Notification,
    Silly,
    Emotion,
    SleepWake,
    Compile,
}

impl SpriteLoop {
    pub fn frame_count(self) -> u8 {
        match self {
            Self::Perch => 4,
            Self::Blink => 4,
            Self::Sleep => 4,
            Self::Idle | Self::Warning | Self::Success => 4,
            Self::Working | Self::Compile => 3,
            Self::WalkLeft => 8,
            Self::WalkRight => 4,
            Self::Fly => 4,
            Self::Alert | Self::Flinch => 3,
            Self::Party | Self::Thinking => 5,
            Self::Charging
            | Self::Presence
            | Self::Notification
            | Self::Silly
            | Self::Emotion
            | Self::SleepWake => 5,
        }
    }

    pub fn frame_duration(self) -> Duration {
        match self {
            Self::Fly => Duration::from_millis(80),
            Self::WalkLeft | Self::WalkRight => Duration::from_millis(140),
            Self::Alert | Self::Flinch => Duration::from_millis(110),
            Self::Idle | Self::Thinking => Duration::from_millis(250),
            Self::Working | Self::Compile | Self::Success | Self::Notification | Self::Silly => {
                Duration::from_millis(200)
            }
            Self::Party => Duration::from_millis(167),
            Self::Warning => Duration::from_millis(110),
            Self::Charging | Self::Presence | Self::Emotion | Self::SleepWake => {
                Duration::from_millis(250)
            }
            _ => Duration::from_millis(450),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Perched(Perch),
    Walking,
    Flying,
    Alert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    ReadSystemTelemetry,
    ShowNotifications,
    ReadSelectedFiles,
    BrowserSummary,
    RunApprovedCommand,
}

#[derive(Debug, Default)]
pub struct Permissions {
    granted: Vec<Capability>,
}

impl Permissions {
    pub fn grant(&mut self, capability: Capability) {
        if !self.granted.contains(&capability) {
            self.granted.push(capability);
        }
    }

    pub fn revoke(&mut self, capability: Capability) {
        self.granted.retain(|granted| *granted != capability);
    }

    pub fn allows(&self, capability: Capability) -> bool {
        self.granted.contains(&capability)
    }
}

#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    pub memory_total_kib: u64,
    pub memory_available_kib: u64,
}

impl SystemSnapshot {
    pub fn read() -> Self {
        let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
        Self {
            memory_total_kib: meminfo_value(&meminfo, "MemTotal:"),
            memory_available_kib: meminfo_value(&meminfo, "MemAvailable:"),
        }
    }

    pub fn memory_used_percent(&self) -> u8 {
        if self.memory_total_kib == 0 {
            return 0;
        }

        let used = self
            .memory_total_kib
            .saturating_sub(self.memory_available_kib);
        ((used * 100) / self.memory_total_kib) as u8
    }
}

fn meminfo_value(meminfo: &str, key: &str) -> u64 {
    meminfo
        .lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetEvent {
    MemoryPressure,
}

impl PetEvent {
    pub fn message(self, snapshot: &SystemSnapshot) -> String {
        match self {
            Self::MemoryPressure => format!(
                "Memory use is {}%. Would you like to open System Monitor?",
                snapshot.memory_used_percent()
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionProposal {
    pub id: &'static str,
    pub label: &'static str,
    pub program: &'static str,
    pub arguments: &'static [&'static str],
}

impl ActionProposal {
    pub fn open_system_monitor() -> Self {
        Self {
            id: "open-system-monitor",
            label: "Open System Monitor",
            program: "gnome-system-monitor",
            arguments: &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    CommandPermissionNotGranted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedAction(ActionProposal);

impl ApprovedAction {
    pub fn proposal(&self) -> &ActionProposal {
        &self.0
    }
}

pub struct Companion {
    activity: Activity,
    animation: SpriteLoop,
    frame: u8,
    frame_elapsed: Duration,
    permissions: Permissions,
    latest_snapshot: Option<SystemSnapshot>,
    last_memory_alert: Option<Duration>,
}

impl Default for Companion {
    fn default() -> Self {
        Self::new()
    }
}

impl Companion {
    pub fn new() -> Self {
        let mut permissions = Permissions::default();
        permissions.grant(Capability::ReadSystemTelemetry);
        permissions.grant(Capability::ShowNotifications);

        Self {
            activity: Activity::Perched(Perch::BottomRight),
            animation: SpriteLoop::Perch,
            frame: 0,
            frame_elapsed: Duration::ZERO,
            permissions,
            latest_snapshot: None,
            last_memory_alert: None,
        }
    }

    pub fn observe(&mut self, snapshot: SystemSnapshot, now: Duration) -> Option<PetEvent> {
        if !self.permissions.allows(Capability::ReadSystemTelemetry) {
            return None;
        }

        let event = if snapshot.memory_used_percent() >= MEMORY_ALERT_THRESHOLD_PERCENT
            && self.memory_alert_is_due(now)
        {
            self.last_memory_alert = Some(now);
            self.set_activity(Activity::Alert, SpriteLoop::Alert);
            Some(PetEvent::MemoryPressure)
        } else {
            None
        };
        self.latest_snapshot = Some(snapshot);
        event
    }

    pub fn begin_roaming(&mut self, flying: bool) {
        if flying {
            self.set_activity(Activity::Flying, SpriteLoop::Fly);
        } else {
            self.begin_walking(false);
        }
    }

    pub fn begin_walking(&mut self, right: bool) {
        let animation = if right {
            SpriteLoop::WalkRight
        } else {
            SpriteLoop::WalkLeft
        };
        self.set_activity(Activity::Walking, animation);
    }

    pub fn set_visual_animation(&mut self, animation: SpriteLoop) {
        self.animation = animation;
        self.frame = 0;
        self.frame_elapsed = Duration::ZERO;
    }

    pub fn perch(&mut self, perch: Perch) {
        self.set_activity(Activity::Perched(perch), SpriteLoop::Perch);
    }

    pub fn tick(&mut self, elapsed: Duration) {
        self.frame_elapsed += elapsed;
        let frame_duration = self.animation.frame_duration();
        while self.frame_elapsed >= frame_duration {
            self.frame_elapsed -= frame_duration;
            self.frame = (self.frame + 1) % self.animation.frame_count();
        }
    }

    pub fn approve_action(
        &self,
        proposal: ActionProposal,
    ) -> Result<ApprovedAction, ApprovalError> {
        if !self.permissions.allows(Capability::RunApprovedCommand) {
            return Err(ApprovalError::CommandPermissionNotGranted);
        }
        Ok(ApprovedAction(proposal))
    }

    pub fn permissions_mut(&mut self) -> &mut Permissions {
        &mut self.permissions
    }

    pub fn activity(&self) -> Activity {
        self.activity
    }

    pub fn animation(&self) -> SpriteLoop {
        self.animation
    }

    pub fn frame(&self) -> u8 {
        self.frame
    }

    pub fn position(&self) -> Perch {
        match self.activity {
            Activity::Perched(perch) => perch,
            Activity::Walking | Activity::Flying | Activity::Alert => Perch::BottomRight,
        }
    }

    pub fn status_line(&self) -> String {
        match &self.latest_snapshot {
            Some(snapshot) => format!("Memory use: {}%.", snapshot.memory_used_percent()),
            None => "No system telemetry has been collected.".to_owned(),
        }
    }

    fn memory_alert_is_due(&self, now: Duration) -> bool {
        self.last_memory_alert
            .is_none_or(|last_alert| now.saturating_sub(last_alert) >= MEMORY_ALERT_COOLDOWN)
    }

    fn set_activity(&mut self, activity: Activity, animation: SpriteLoop) {
        self.activity = activity;
        self.animation = animation;
        self.frame = 0;
        self.frame_elapsed = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(available_kib: u64) -> SystemSnapshot {
        SystemSnapshot {
            memory_total_kib: 1000,
            memory_available_kib: available_kib,
        }
    }

    #[test]
    fn parses_memory_values() {
        let meminfo = "MemTotal:       1000 kB\nMemAvailable:    250 kB\n";
        assert_eq!(meminfo_value(meminfo, "MemTotal:"), 1000);
        assert_eq!(meminfo_value(meminfo, "MemAvailable:"), 250);
    }

    #[test]
    fn memory_pressure_alerts_once_per_cooldown() {
        let mut owl = Companion::new();
        assert_eq!(
            owl.observe(snapshot(50), Duration::ZERO),
            Some(PetEvent::MemoryPressure)
        );
        assert_eq!(owl.observe(snapshot(50), Duration::from_secs(60)), None);
        assert_eq!(
            owl.observe(snapshot(50), MEMORY_ALERT_COOLDOWN),
            Some(PetEvent::MemoryPressure)
        );
    }

    #[test]
    fn roaming_selects_the_matching_sprite_loop() {
        let mut owl = Companion::new();
        owl.begin_roaming(true);
        assert_eq!(owl.activity(), Activity::Flying);
        assert_eq!(owl.animation(), SpriteLoop::Fly);
        owl.perch(Perch::BottomLeft);
        assert_eq!(owl.position(), Perch::BottomLeft);
    }

    #[test]
    fn animation_advances_and_wraps() {
        let mut owl = Companion::new();
        owl.begin_roaming(true);
        owl.tick(Duration::from_millis(4 * 80));
        assert_eq!(owl.frame(), 0);
        owl.tick(Duration::from_millis(80));
        assert_eq!(owl.frame(), 1);
    }

    #[test]
    fn sprite_loop_lengths_match_the_active_art_pack() {
        assert_eq!(SpriteLoop::WalkLeft.frame_count(), 8);
        assert_eq!(SpriteLoop::WalkRight.frame_count(), 4);
        assert_eq!(SpriteLoop::Fly.frame_count(), 4);
        assert_eq!(SpriteLoop::Alert.frame_count(), 3);
    }

    #[test]
    fn commands_require_a_separate_permission_and_approval() {
        let mut owl = Companion::new();
        let proposal = ActionProposal::open_system_monitor();
        assert_eq!(
            owl.approve_action(proposal.clone()),
            Err(ApprovalError::CommandPermissionNotGranted)
        );
        owl.permissions_mut().grant(Capability::RunApprovedCommand);
        assert_eq!(
            owl.approve_action(proposal).unwrap().proposal().id,
            "open-system-monitor"
        );
    }
}
