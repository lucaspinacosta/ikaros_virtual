//! Local, selected-file calendar and routine collectors.

use std::{collections::HashSet, fs, path::PathBuf, time::Duration};

use chrono::{DateTime, Datelike, Local, NaiveDateTime, TimeZone, Timelike};

use crate::{collectors::TaskEvent, settings::Routine};

pub struct CalendarWatcher {
    path: PathBuf,
    announced: HashSet<i64>,
}

impl CalendarWatcher {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            announced: HashSet::new(),
        }
    }

    pub fn poll_at(&mut self, now: DateTime<Local>) -> Vec<TaskEvent> {
        let Ok(contents) = fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        for start in contents.lines().filter_map(parse_start) {
            let seconds = start.signed_duration_since(now).num_seconds();
            if (0..=300).contains(&seconds) && self.announced.insert(start.timestamp()) {
                events.push(TaskEvent::Started("Calendar event starts soon".to_owned()));
            }
        }
        self.announced
            .retain(|timestamp| *timestamp >= now.timestamp() - 24 * 60 * 60);
        events
    }
}

pub struct RoutineWatcher {
    routines: Vec<Routine>,
    announced: HashSet<(String, i32, u32)>,
    break_interval: Option<Duration>,
    active_for: Duration,
}

impl RoutineWatcher {
    pub fn new(routines: Vec<Routine>, break_reminder_minutes: Option<u64>) -> Self {
        Self {
            routines,
            announced: HashSet::new(),
            break_interval: break_reminder_minutes.map(|minutes| Duration::from_secs(minutes * 60)),
            active_for: Duration::ZERO,
        }
    }

    pub fn tick(&mut self, now: DateTime<Local>, elapsed: Duration, idle: bool) -> Vec<TaskEvent> {
        if idle {
            self.active_for = Duration::ZERO;
        } else {
            self.active_for += elapsed;
        }
        let mut events = self
            .routines
            .iter()
            .filter(|routine| {
                routine.hour == now.hour() as u8 && routine.minute == now.minute() as u8
            })
            .filter(|routine| {
                self.announced
                    .insert((routine.name.clone(), now.year(), now.ordinal()))
            })
            .map(|routine| TaskEvent::Started(routine.name.clone()))
            .collect::<Vec<_>>();
        if self
            .break_interval
            .is_some_and(|interval| self.active_for >= interval)
        {
            self.active_for = Duration::ZERO;
            events.push(TaskEvent::Started("Break reminder".to_owned()));
        }
        events
    }
}

fn parse_start(line: &str) -> Option<DateTime<Local>> {
    let value = line.strip_prefix("DTSTART")?.split_once(':')?.1;
    if let Some(value) = value.strip_suffix('Z') {
        return NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S")
            .ok()
            .map(|value| value.and_utc().with_timezone(&Local));
    }
    Local
        .from_local_datetime(&NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?)
        .single()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_local_ical_start_time() {
        assert!(parse_start("DTSTART:20261224T093000").is_some());
    }

    #[test]
    fn breaks_only_fire_after_the_full_interval() {
        let now = Local::now();
        let mut watcher = RoutineWatcher::new(Vec::new(), Some(60));
        assert!(
            watcher
                .tick(now, Duration::from_secs(59 * 60), false)
                .is_empty()
        );
        assert_eq!(
            watcher.tick(now, Duration::from_secs(60), false),
            vec![TaskEvent::Started("Break reminder".to_owned())]
        );
    }
}
