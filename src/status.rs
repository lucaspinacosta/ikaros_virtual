use std::{collections::HashMap, fs, process::Command, time::Duration};

use crate::context::{FocusContext, focused_context};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryStatus {
    pub percentage: u8,
    pub charging: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemStatus {
    pub memory_used_percent: u8,
    pub load_average: Option<String>,
    pub disk_available_gib: Option<u64>,
    pub battery: Option<BatteryStatus>,
    pub network_connected: bool,
    pub focus: Option<FocusContext>,
}

pub fn read_status() -> SystemStatus {
    let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let total = meminfo_value(&meminfo, "MemTotal:");
    let available = meminfo_value(&meminfo, "MemAvailable:");
    let memory_used_percent = if total == 0 {
        0
    } else {
        ((total.saturating_sub(available) * 100) / total) as u8
    };

    SystemStatus {
        memory_used_percent,
        load_average: fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|value| value.split_whitespace().next().map(str::to_owned)),
        disk_available_gib: disk_available_gib(),
        battery: battery_status(),
        network_connected: network_connected(),
        focus: focused_context(),
    }
}

pub fn format_status(status: &SystemStatus) -> String {
    let battery = status.battery.as_ref().map_or_else(
        || "Battery: unavailable".to_owned(),
        |battery| {
            let state = if battery.charging {
                "charging"
            } else {
                "discharging"
            };
            format!("Battery: {}% ({state})", battery.percentage)
        },
    );
    let disk = status.disk_available_gib.map_or_else(
        || "Disk: unavailable".to_owned(),
        |available| format!("Disk free: {available} GiB"),
    );
    let load = status.load_average.as_deref().map_or_else(
        || "Load average: unavailable".to_owned(),
        |load| format!("Load average: {load}"),
    );
    let network = if status.network_connected {
        "connected"
    } else {
        "offline"
    };
    let focus = status
        .focus
        .map_or_else(|| "disabled".to_owned(), |focus| focus.label().to_owned());
    format!(
        "Memory: {}%\n{load}\n{disk}\n{battery}\nNetwork: {network}\nFocus: {focus}",
        status.memory_used_percent
    )
}

fn meminfo_value(meminfo: &str, key: &str) -> u64 {
    meminfo
        .lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn disk_available_gib() -> Option<u64> {
    let output = Command::new("df").args(["-Pk", "/"]).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let fields: Vec<_> = text.lines().nth(1)?.split_whitespace().collect();
    fields
        .get(3)?
        .parse::<u64>()
        .ok()
        .map(|kib| kib / 1_048_576)
}

fn battery_status() -> Option<BatteryStatus> {
    let devices = Command::new("upower").arg("-e").output().ok()?;
    let devices_text = String::from_utf8_lossy(&devices.stdout);
    let device = devices_text.lines().find(|line| line.contains("battery"))?;
    let details = Command::new("upower").args(["-i", device]).output().ok()?;
    parse_battery_status(&String::from_utf8_lossy(&details.stdout))
}

fn parse_battery_status(details: &str) -> Option<BatteryStatus> {
    let percentage = details
        .lines()
        .find_map(|line| line.trim().strip_prefix("percentage:"))?
        .trim()
        .strip_suffix('%')?
        .parse()
        .ok()?;
    let charging = details
        .lines()
        .find_map(|line| line.trim().strip_prefix("state:"))
        .is_some_and(|state| state.trim() == "charging");
    Some(BatteryStatus {
        percentage,
        charging,
    })
}

fn network_connected() -> bool {
    Command::new("nmcli")
        .args(["-t", "-f", "STATE", "general"])
        .output()
        .ok()
        .is_some_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "connected")
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum AlertKind {
    LowBattery,
    MemoryPressure,
    LowDisk,
    NetworkOffline,
}

impl AlertKind {
    pub fn summary(self) -> &'static str {
        match self {
            Self::LowBattery => "Ikaros: low battery",
            Self::MemoryPressure => "Ikaros: memory pressure",
            Self::LowDisk => "Ikaros: low disk space",
            Self::NetworkOffline => "Ikaros: network offline",
        }
    }
}

pub struct AlertEngine {
    last_seen: HashMap<AlertKind, Duration>,
    cooldown: Duration,
    was_connected: Option<bool>,
}

impl Default for AlertEngine {
    fn default() -> Self {
        Self {
            last_seen: HashMap::new(),
            cooldown: Duration::from_secs(15 * 60),
            was_connected: None,
        }
    }
}

impl AlertEngine {
    pub fn evaluate(&mut self, status: &SystemStatus, now: Duration) -> Vec<AlertKind> {
        let mut alerts = Vec::new();
        let candidates = [
            status
                .battery
                .as_ref()
                .filter(|battery| battery.percentage <= 15 && !battery.charging)
                .map(|_| AlertKind::LowBattery),
            (status.memory_used_percent >= 90).then_some(AlertKind::MemoryPressure),
            status
                .disk_available_gib
                .filter(|available| *available < 10)
                .map(|_| AlertKind::LowDisk),
            self.was_connected
                .is_some_and(|was_connected| was_connected && !status.network_connected)
                .then_some(AlertKind::NetworkOffline),
        ];
        self.was_connected = Some(status.network_connected);

        for candidate in candidates.into_iter().flatten() {
            if self
                .last_seen
                .get(&candidate)
                .is_none_or(|last_seen| now.saturating_sub(*last_seen) >= self.cooldown)
            {
                self.last_seen.insert(candidate, now);
                alerts.push(candidate);
            }
        }
        alerts
    }
}

pub fn send_notification(alert: AlertKind, status: &SystemStatus) {
    let body = match alert {
        AlertKind::LowBattery => status.battery.as_ref().map_or_else(
            || "Battery is low.".to_owned(),
            |battery| format!("Battery is at {}%.", battery.percentage),
        ),
        AlertKind::MemoryPressure => format!("Memory use is {}%.", status.memory_used_percent),
        AlertKind::LowDisk => status.disk_available_gib.map_or_else(
            || "Disk space is low.".to_owned(),
            |available| format!("Only {available} GiB remain on the root filesystem."),
        ),
        AlertKind::NetworkOffline => "The active NetworkManager connection was lost.".to_owned(),
    };
    let _ = Command::new("notify-send")
        .args(["--app-name=Ikaros", alert.summary(), &body])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_battery_state() {
        let details = "  state: discharging\n  percentage: 14%\n";
        assert_eq!(
            parse_battery_status(details),
            Some(BatteryStatus {
                percentage: 14,
                charging: false
            })
        );
    }

    #[test]
    fn only_alerts_when_network_transitions_offline() {
        let mut engine = AlertEngine::default();
        let connected = SystemStatus {
            memory_used_percent: 10,
            load_average: None,
            disk_available_gib: Some(100),
            battery: None,
            network_connected: true,
            focus: None,
        };
        let offline = SystemStatus {
            network_connected: false,
            ..connected.clone()
        };
        assert!(engine.evaluate(&connected, Duration::ZERO).is_empty());
        assert_eq!(
            engine.evaluate(&offline, Duration::from_secs(1)),
            vec![AlertKind::NetworkOffline]
        );
    }
}
