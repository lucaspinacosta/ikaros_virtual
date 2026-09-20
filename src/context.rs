use std::{env, process::Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusContext {
    Coding,
    Browsing,
    Other,
}

impl FocusContext {
    pub fn label(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Browsing => "browsing",
            Self::Other => "another application",
        }
    }
}

pub fn focused_context() -> Option<FocusContext> {
    focused_context_when(env::var("IKAROS_FOCUSED_APP_AWARENESS").ok().as_deref() == Some("1"))
}

pub fn focused_context_when(enabled: bool) -> Option<FocusContext> {
    if !enabled {
        return None;
    }
    let output = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .ok()?;
    let class = parse_window_class(&String::from_utf8_lossy(&output.stdout))?.to_ascii_lowercase();
    Some(
        if ["code", "code-oss", "cursor", "zed", "neovide"].contains(&class.as_str()) {
            FocusContext::Coding
        } else if [
            "firefox",
            "zen",
            "chromium",
            "google-chrome",
            "brave-browser",
        ]
        .contains(&class.as_str())
        {
            FocusContext::Browsing
        } else {
            FocusContext::Other
        },
    )
}

pub fn focused_fullscreen() -> Option<bool> {
    focused_fullscreen_when(env::var("IKAROS_FOCUSED_APP_AWARENESS").ok().as_deref() == Some("1"))
}

pub fn focused_fullscreen_when(enabled: bool) -> Option<bool> {
    if !enabled {
        return None;
    }
    let output = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .ok()?;
    parse_fullscreen(&String::from_utf8_lossy(&output.stdout))
}

fn parse_window_class(json: &str) -> Option<&str> {
    let line = json
        .lines()
        .find(|line| line.trim_start().starts_with("\"class\""))?;
    Some(
        line.split(':')
            .nth(1)?
            .trim()
            .trim_matches(',')
            .trim_matches('"'),
    )
}

fn parse_fullscreen(json: &str) -> Option<bool> {
    let value = json
        .lines()
        .find(|line| line.trim_start().starts_with("\"fullscreen\""))?
        .split(':')
        .nth(1)?
        .trim()
        .trim_matches(',');
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_the_active_window_class() {
        let json = "{\n  \"class\": \"code\",\n  \"title\": \"private file\"\n}";
        assert_eq!(parse_window_class(json), Some("code"));
    }

    #[test]
    fn parses_fullscreen_without_reading_window_contents() {
        assert_eq!(parse_fullscreen("{\n  \"fullscreen\": true\n}"), Some(true));
    }
}
