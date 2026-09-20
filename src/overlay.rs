use std::{
    cell::RefCell,
    path::PathBuf,
    process::Command,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use chrono::{Local, Timelike};
use gtk::{gdk::prelude::SurfaceExt, glib, prelude::*};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::calendar::{CalendarWatcher, RoutineWatcher};
use crate::collectors::{
    FileWatcher, IdleWatcher, SessionEvent, SystemdUnitWatcher, TaskCompletionAdapter, file_event,
    notification_event, session_event, session_idle, session_locked, task_event,
};
use crate::companion::{Companion, SpriteLoop};
use crate::context::{FocusContext, focused_fullscreen_when};
use crate::events::{EventKind, EventStore, LocalEvent, event_log_path, record};
use crate::settings::Settings;
use crate::status::{
    AlertEngine, format_status, read_status, read_status_with_focus, send_notification,
};
use crate::{notifications::NotificationWatcher, routing::animation_for};

const OWL_SIZE_PX: i32 = 90;
const SCREEN_MARGIN_PX: f64 = 20.0;
const SIGNAL_POLL_INTERVAL: Duration = Duration::from_secs(2);
const STATUS_POLL_INTERVAL: Duration = Duration::from_secs(10);
const SESSION_POLL_INTERVAL: Duration = Duration::from_secs(5);
const MUSIC_DANCE_DURATION: Duration = Duration::from_secs(5);
const MUSIC_DANCE_CYCLE: Duration = Duration::from_secs(90);

pub fn run() {
    let app = gtk::Application::builder()
        .application_id("io.ikaros.Virtual")
        .build();
    app.connect_activate(build_overlay);
    app.run();
}

pub fn run_status_panel() {
    let app = gtk::Application::builder()
        .application_id("io.ikaros.Virtual.Status")
        .build();
    app.connect_activate(build_status_panel);
    app.run_with_args::<&str>(&[]);
}

fn build_status_panel(app: &gtk::Application) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Ikaros Settings")
        .default_width(620)
        .default_height(680)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    let title = gtk::Label::new(Some("Ikaros local awareness settings"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    let status = gtk::Label::new(Some(&format_status(&read_status())));
    status.set_xalign(0.0);
    status.set_selectable(true);
    let settings = Settings::load();
    let calendar_path = entry_row("Calendar (.ics) path", settings.calendar_path.as_ref());
    let watch_directory = entry_row("Watched directory path", settings.watch_directory.as_ref());
    let notifications = gtk::Switch::new();
    notifications.set_active(settings.read_notifications);
    let focus_awareness = gtk::Switch::new();
    focus_awareness.set_active(settings.focused_app_awareness);
    let break_minutes = gtk::SpinButton::with_range(0.0, 480.0, 5.0);
    break_minutes.set_value(settings.break_reminder_minutes.unwrap_or(0) as f64);
    let quiet_enabled = gtk::Switch::new();
    quiet_enabled.set_active(settings.quiet_hours.is_some());
    let quiet_start = gtk::SpinButton::with_range(0.0, 23.0, 1.0);
    let quiet_end = gtk::SpinButton::with_range(0.0, 23.0, 1.0);
    quiet_start.set_value(
        settings
            .quiet_hours
            .map_or(22.0, |hours| hours.start_hour as f64),
    );
    quiet_end.set_value(
        settings
            .quiet_hours
            .map_or(7.0, |hours| hours.end_hour as f64),
    );
    let units = gtk::TextView::new();
    units.set_monospace(true);
    units.buffer().set_text(&settings.watched_units.join("\n"));
    let routines = gtk::TextView::new();
    routines.set_monospace(true);
    routines.buffer().set_text(
        &settings
            .routines
            .iter()
            .map(|routine| format!("{}@{:02}:{:02}", routine.name, routine.hour, routine.minute))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let saved = gtk::Label::new(None);
    saved.set_xalign(0.0);
    saved.add_css_class("dim-label");
    let monitor = gtk::Button::with_label("Open System Monitor");
    monitor.connect_clicked(|_| {
        let _ = Command::new("gnome-system-monitor").spawn();
    });
    let pause = gtk::Button::with_label("Pause reactions for one hour");
    pause.connect_clicked(|_| {
        let mut settings = Settings::load();
        settings.pause_for_one_hour();
        let _ = settings.save();
    });
    let save = gtk::Button::with_label("Save settings");
    save.add_css_class("suggested-action");
    save.connect_clicked({
        let calendar_path = calendar_path.clone();
        let watch_directory = watch_directory.clone();
        let notifications = notifications.clone();
        let focus_awareness = focus_awareness.clone();
        let break_minutes = break_minutes.clone();
        let quiet_enabled = quiet_enabled.clone();
        let quiet_start = quiet_start.clone();
        let quiet_end = quiet_end.clone();
        let units = units.clone();
        let routines = routines.clone();
        let saved = saved.clone();
        move |_| {
            let routines_text = text_view_text(&routines);
            let routines = match parse_routines(&routines_text) {
                Ok(routines) => routines,
                Err(error) => {
                    saved.set_text(&error);
                    return;
                }
            };
            let mut settings = Settings::load();
            settings.calendar_path = path_from_entry(&calendar_path);
            settings.watch_directory = path_from_entry(&watch_directory);
            settings.read_notifications = notifications.is_active();
            settings.focused_app_awareness = focus_awareness.is_active();
            settings.break_reminder_minutes =
                (break_minutes.value_as_int() > 0).then_some(break_minutes.value_as_int() as u64);
            settings.quiet_hours =
                quiet_enabled
                    .is_active()
                    .then_some(crate::settings::QuietHours {
                        start_hour: quiet_start.value_as_int() as u8,
                        end_hour: quiet_end.value_as_int() as u8,
                    });
            settings.watched_units = text_view_text(&units)
                .lines()
                .map(str::trim)
                .filter(|unit| !unit.is_empty())
                .map(str::to_owned)
                .collect();
            settings.routines = routines;
            match settings.save() {
                Ok(()) => saved.set_text(
                    "Saved. Restart the companion to apply source-path and watcher changes.",
                ),
                Err(error) => saved.set_text(&format!("Could not save settings: {error}")),
            }
        }
    });
    content.append(&title);
    content.append(&status);
    content.append(&labeled_row(
        "Read incoming desktop notifications",
        &notifications,
    ));
    content.append(&labeled_row(
        "Focused application and fullscreen awareness",
        &focus_awareness,
    ));
    content.append(&labeled_row(
        "Break reminder (minutes, 0 disables)",
        &break_minutes,
    ));
    content.append(&labeled_row("Enable quiet hours", &quiet_enabled));
    content.append(&labeled_row("Quiet hours start (0-23)", &quiet_start));
    content.append(&labeled_row("Quiet hours end (0-23)", &quiet_end));
    content.append(&calendar_path);
    content.append(&watch_directory);
    content.append(&section("Systemd user units (one per line)", &units));
    content.append(&section(
        "Manual routines: Name@HH:MM (one per line)",
        &routines,
    ));
    let browser_note = gtk::Label::new(Some(
        "Browser sharing is enabled in the Ikaros browser extension's own preferences.",
    ));
    browser_note.set_wrap(true);
    browser_note.set_xalign(0.0);
    browser_note.add_css_class("dim-label");
    content.append(&browser_note);
    content.append(&monitor);
    content.append(&pause);
    content.append(&save);
    content.append(&saved);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&content)
        .vexpand(true)
        .build();
    window.set_child(Some(&scroll));
    window.present();
    glib::timeout_add_local(Duration::from_secs(5), move || {
        status.set_text(&format_status(&read_status()));
        glib::ControlFlow::Continue
    });
}

fn entry_row(label: &str, value: Option<&PathBuf>) -> gtk::Box {
    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    entry.set_text(value.and_then(|path| path.to_str()).unwrap_or_default());
    labeled_row(label, &entry)
}

fn labeled_row(label: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let label = gtk::Label::new(Some(label));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    row.append(&label);
    row.append(widget);
    row
}

fn section(label: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let label = gtk::Label::new(Some(label));
    label.set_xalign(0.0);
    section.append(&label);
    widget.set_size_request(-1, 72);
    section.append(widget);
    section
}

fn path_from_entry(row: &gtk::Box) -> Option<PathBuf> {
    row.last_child()?
        .downcast::<gtk::Entry>()
        .ok()
        .and_then(|entry| {
            let value = entry.text();
            (!value.is_empty()).then(|| PathBuf::from(value.as_str()))
        })
}

fn text_view_text(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string()
}

fn parse_routines(value: &str) -> Result<Vec<crate::settings::Routine>, String> {
    value
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (name, time) = line
                .split_once('@')
                .ok_or_else(|| "Routines must use Name@HH:MM".to_owned())?;
            let (hour, minute) = time
                .split_once(':')
                .ok_or_else(|| "Routine times must use HH:MM".to_owned())?;
            let hour = hour
                .trim()
                .parse::<u8>()
                .map_err(|_| "Routine hour must be 0-23".to_owned())?;
            let minute = minute
                .trim()
                .parse::<u8>()
                .map_err(|_| "Routine minute must be 0-59".to_owned())?;
            if name.trim().is_empty() || hour > 23 || minute > 59 {
                return Err("Routine name or time is invalid".to_owned());
            }
            Ok(crate::settings::Routine {
                name: name.trim().to_owned(),
                hour,
                minute,
            })
        })
        .collect()
}

fn build_overlay(app: &gtk::Application) {
    if !gtk4_layer_shell::is_supported() {
        eprintln!("Ikaros requires a Wayland compositor with layer-shell support.");
        app.quit();
        return;
    }

    install_overlay_style();
    let window = gtk::ApplicationWindow::builder().application(app).build();
    window.add_css_class("owl-overlay");
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("ikaros-owl"));
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        window.set_anchor(edge, true);
    }
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_decorated(false);
    window.set_resizable(false);
    let (output_width, output_height) = active_output_size();
    window.set_default_size(output_width, output_height);
    window.connect_realize(|window| {
        if let Some(surface) = window.surface() {
            // The full-screen layer is visually transparent and never blocks desktop input.
            surface.set_input_region(Some(&gtk::cairo::Region::create()));
        }
    });

    let canvas = gtk::Fixed::new();
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);
    let picture = gtk::Image::from_pixbuf(Some(&load_frame(SpriteLoop::Perch, 0)));
    picture.set_pixel_size(OWL_SIZE_PX);
    picture.set_size_request(OWL_SIZE_PX, OWL_SIZE_PX);
    canvas.put(&picture, 0.0, 0.0);
    window.set_child(Some(&canvas));
    window.present();

    let companion = Rc::new(RefCell::new(Companion::new()));
    let life = Rc::new(RefCell::new(PetLife::default()));
    let last_update = Rc::new(RefCell::new(Instant::now()));
    let started_at = Instant::now();
    let last_status_poll = Rc::new(RefCell::new(started_at - STATUS_POLL_INTERVAL));
    let alert_engine = Rc::new(RefCell::new(AlertEngine::default()));
    let last_session_poll = Rc::new(RefCell::new(started_at - SESSION_POLL_INTERVAL));
    let locked = Rc::new(RefCell::new(None));
    let idle_watcher = Rc::new(RefCell::new(IdleWatcher::default()));
    let settings = Settings::load();
    let calendar_watcher = settings
        .calendar_path
        .clone()
        .map(CalendarWatcher::new)
        .map(RefCell::new);
    let routine_watcher = Rc::new(RefCell::new(RoutineWatcher::new(
        settings.routines.clone(),
        settings.break_reminder_minutes,
    )));
    let unit_watcher = Rc::new(RefCell::new(SystemdUnitWatcher::new(
        settings.watched_units.clone(),
    )));
    let file_watcher = settings
        .watch_directory
        .clone()
        .map(FileWatcher::new)
        .map(RefCell::new);
    let (notification_sender, notification_receiver) = mpsc::sync_channel(64);
    // The monitor is best-effort: desktop use continues when dbus-monitor is unavailable.
    let notification_watcher = settings
        .read_notifications
        .then(|| NotificationWatcher::start(notification_sender).ok())
        .flatten();
    glib::timeout_add_local(Duration::from_millis(50), {
        let companion = Rc::clone(&companion);
        let life = Rc::clone(&life);
        let last_update = Rc::clone(&last_update);
        let picture = picture.clone();
        let canvas = canvas.clone();
        let last_status_poll = Rc::clone(&last_status_poll);
        let alert_engine = Rc::clone(&alert_engine);
        let last_session_poll = Rc::clone(&last_session_poll);
        let locked = Rc::clone(&locked);
        let idle_watcher = Rc::clone(&idle_watcher);
        let routine_watcher = Rc::clone(&routine_watcher);
        let unit_watcher = Rc::clone(&unit_watcher);
        move || {
            // Keep the child process alive for the lifetime of the GTK callback.
            let _ = &notification_watcher;
            let now = Instant::now();
            let elapsed = now.duration_since(*last_update.borrow());
            *last_update.borrow_mut() = now;

            if now.duration_since(*last_session_poll.borrow()) >= SESSION_POLL_INTERVAL {
                *last_session_poll.borrow_mut() = now;
                if let Some(current) = session_locked() {
                    let previous = locked.replace(Some(current));
                    if previous != Some(current) {
                        let _ = record(&session_event(if current {
                            SessionEvent::Locked
                        } else {
                            SessionEvent::Unlocked
                        }));
                    }
                }
                if let Some(event) = idle_watcher.borrow_mut().poll(session_idle()) {
                    let event = session_event(event);
                    let _ = record(&event);
                    life.borrow_mut().trigger_reaction(animation_for(&event));
                }
                if let Some(watcher) = &file_watcher {
                    for event in watcher.borrow_mut().poll() {
                        let event = file_event(event);
                        let _ = record(&event);
                        life.borrow_mut().trigger_reaction(animation_for(&event));
                    }
                }
            }

            for notification in notification_receiver.try_iter().take(4) {
                let event = notification_event(notification);
                let _ = record(&event);
                life.borrow_mut().trigger_reaction(animation_for(&event));
            }

            if now.duration_since(*last_status_poll.borrow()) >= STATUS_POLL_INTERVAL {
                *last_status_poll.borrow_mut() = now;
                let _ = EventStore::at(event_log_path()).prune();
                let settings = Settings::load();
                let status = read_status_with_focus(crate::context::focused_context_when(
                    settings.focused_app_awareness,
                ));
                life.borrow_mut().set_preferences(
                    settings.reactions_paused(),
                    settings.quiet_hours.is_some_and(|quiet_hours| {
                        let hour = Local::now().hour() as u8;
                        if quiet_hours.start_hour <= quiet_hours.end_hour {
                            (quiet_hours.start_hour..quiet_hours.end_hour).contains(&hour)
                        } else {
                            hour >= quiet_hours.start_hour || hour < quiet_hours.end_hour
                        }
                    }),
                );
                life.borrow_mut().set_status(
                    status.focus,
                    status
                        .battery
                        .as_ref()
                        .is_some_and(|battery| battery.charging),
                );
                life.borrow_mut().set_fullscreen(
                    focused_fullscreen_when(settings.focused_app_awareness).unwrap_or(false),
                );
                if let Some(focus) = status.focus {
                    let _ = record(&LocalEvent {
                        kind: EventKind::Activity,
                        summary: "Focused application context".to_owned(),
                        detail: focus.label().to_owned(),
                    });
                }
                if let Some(watcher) = &calendar_watcher {
                    for event in watcher.borrow_mut().poll_at(Local::now()) {
                        dispatch_task_event(event, &life);
                    }
                }
                for event in routine_watcher.borrow_mut().tick(
                    Local::now(),
                    STATUS_POLL_INTERVAL,
                    idle_watcher.borrow().is_idle(),
                ) {
                    dispatch_task_event(event, &life);
                }
                for event in unit_watcher.borrow_mut().poll() {
                    dispatch_task_event(event, &life);
                }
                for alert in alert_engine
                    .borrow_mut()
                    .evaluate(&status, now.duration_since(started_at))
                {
                    send_notification(alert, &status);
                    life.borrow_mut().trigger_warning();
                    let _ = record(&LocalEvent {
                        kind: EventKind::System,
                        summary: alert.summary().to_owned(),
                        detail: format_status(&status),
                    });
                }
            }

            let width = f64::from(canvas.allocated_width());
            let height = f64::from(canvas.allocated_height());
            if width <= 0.0 || height <= 0.0 {
                return glib::ControlFlow::Continue;
            }

            let mut life = life.borrow_mut();
            life.set_locked(locked.borrow().unwrap_or(false));
            life.set_idle(idle_watcher.borrow().is_idle());
            life.update_music_signal(elapsed);
            let state = life.tick(elapsed, width, height);
            let mut companion = companion.borrow_mut();
            if companion.animation() != state.animation {
                apply_animation(&mut companion, state.animation);
            }
            companion.tick(elapsed);
            picture.set_from_pixbuf(Some(&load_frame(companion.animation(), companion.frame())));
            canvas.move_(&picture, state.x, state.y);
            glib::ControlFlow::Continue
        }
    });
}

fn apply_animation(companion: &mut Companion, animation: SpriteLoop) {
    match animation {
        SpriteLoop::WalkLeft => companion.begin_walking(false),
        SpriteLoop::WalkRight => companion.begin_walking(true),
        SpriteLoop::Fly => companion.begin_roaming(true),
        SpriteLoop::Perch | SpriteLoop::Blink | SpriteLoop::Sleep => {
            companion.perch(crate::companion::Perch::BottomRight)
        }
        SpriteLoop::Alert
        | SpriteLoop::Flinch
        | SpriteLoop::Idle
        | SpriteLoop::Party
        | SpriteLoop::Working
        | SpriteLoop::Thinking
        | SpriteLoop::Warning
        | SpriteLoop::Charging
        | SpriteLoop::Presence
        | SpriteLoop::Success
        | SpriteLoop::Notification
        | SpriteLoop::Silly => companion.set_visual_animation(animation),
    }
}

#[derive(Clone, Copy)]
struct PetFrame {
    x: f64,
    y: f64,
    animation: SpriteLoop,
}

#[derive(Clone, Copy)]
enum PetMode {
    Rest,
    Fly {
        start_x: f64,
        target_x: f64,
        arc_height: f64,
    },
    Walk {
        target_x: f64,
    },
    Dance,
}

struct PetLife {
    x: f64,
    y: f64,
    mode: PetMode,
    initialized: bool,
    mode_elapsed: Duration,
    mode_duration: Duration,
    signal_elapsed: Duration,
    music_playing: bool,
    music_elapsed: Duration,
    focus: Option<FocusContext>,
    warning_for: Duration,
    reaction: Option<(SpriteLoop, Duration)>,
    charging: bool,
    locked: bool,
    idle: bool,
    fullscreen: bool,
    paused: bool,
    quiet: bool,
    rng: u64,
}

impl Default for PetLife {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            mode: PetMode::Rest,
            initialized: false,
            mode_elapsed: Duration::ZERO,
            mode_duration: Duration::from_secs(3),
            signal_elapsed: SIGNAL_POLL_INTERVAL,
            music_playing: false,
            music_elapsed: Duration::ZERO,
            focus: None,
            warning_for: Duration::ZERO,
            reaction: None,
            charging: false,
            locked: false,
            idle: false,
            fullscreen: false,
            paused: false,
            quiet: false,
            rng: session_seed(),
        }
    }
}

impl PetLife {
    fn tick(&mut self, elapsed: Duration, width: f64, height: f64) -> PetFrame {
        let floor_y = height - f64::from(OWL_SIZE_PX) - SCREEN_MARGIN_PX;
        if !self.initialized {
            self.x = width - f64::from(OWL_SIZE_PX) - SCREEN_MARGIN_PX;
            self.y = floor_y;
            self.initialized = true;
        }

        if self.music_playing {
            self.music_elapsed += elapsed;
        } else {
            self.music_elapsed = Duration::ZERO;
        }

        if self.locked || self.idle {
            return PetFrame {
                x: self.x,
                y: floor_y,
                animation: SpriteLoop::Sleep,
            };
        }

        if self.paused || self.quiet || self.fullscreen {
            return PetFrame {
                x: self.x,
                y: floor_y,
                animation: SpriteLoop::Perch,
            };
        }

        if let Some((animation, remaining)) = self.reaction {
            let remaining = remaining.saturating_sub(elapsed);
            self.reaction = (remaining > Duration::ZERO).then_some((animation, remaining));
            return PetFrame {
                x: self.x,
                y: floor_y,
                animation,
            };
        }

        self.warning_for = self.warning_for.saturating_sub(elapsed);
        if self.warning_for > Duration::ZERO && matches!(self.mode, PetMode::Rest) {
            return PetFrame {
                x: self.x,
                y: self.y,
                animation: SpriteLoop::Warning,
            };
        }

        if self.music_playing
            && self.music_elapsed.as_millis() % MUSIC_DANCE_CYCLE.as_millis()
                < MUSIC_DANCE_DURATION.as_millis()
        {
            self.mode = PetMode::Dance;
            self.mode_elapsed += elapsed;
            let beat = (self.mode_elapsed.as_secs_f64() * 9.0).sin();
            self.y = floor_y - beat.abs() * 10.0;
            return PetFrame {
                x: self.x,
                y: self.y,
                animation: SpriteLoop::Party,
            };
        }

        if matches!(self.mode, PetMode::Dance) {
            self.mode = PetMode::Rest;
            self.mode_elapsed = Duration::ZERO;
            self.y = floor_y;
        }

        self.mode_elapsed += elapsed;
        if matches!(self.focus, Some(FocusContext::Coding)) && matches!(self.mode, PetMode::Rest) {
            self.mode_duration = Duration::from_secs(30);
        }
        let walk_has_arrived =
            matches!(self.mode, PetMode::Walk { target_x } if (self.x - target_x).abs() < 1.0);
        if self.mode_elapsed >= self.mode_duration || walk_has_arrived {
            self.start_next_mode(width, height);
        }

        let seconds = elapsed.as_secs_f64();
        let animation = match self.mode {
            PetMode::Rest if self.charging => SpriteLoop::Charging,
            PetMode::Rest => match self.focus {
                Some(FocusContext::Coding) => SpriteLoop::Working,
                Some(FocusContext::Browsing) => SpriteLoop::Thinking,
                _ => SpriteLoop::Idle,
            },
            PetMode::Walk { target_x } => {
                let walking_left = target_x < self.x;
                self.x = approach(self.x, target_x, 58.0 * seconds);
                self.y = floor_y;
                if walking_left {
                    SpriteLoop::WalkLeft
                } else {
                    SpriteLoop::WalkRight
                }
            }
            PetMode::Fly {
                start_x,
                target_x,
                arc_height,
            } => {
                let progress =
                    (self.mode_elapsed.as_secs_f64() / self.mode_duration.as_secs_f64()).min(1.0);
                self.x = start_x + (target_x - start_x) * progress;
                self.y = floor_y - (progress * std::f64::consts::PI).sin() * arc_height;
                SpriteLoop::Fly
            }
            PetMode::Dance => SpriteLoop::Perch,
        };
        PetFrame {
            x: self.x,
            y: self.y,
            animation,
        }
    }

    fn update_music_signal(&mut self, elapsed: Duration) {
        self.signal_elapsed += elapsed;
        if self.signal_elapsed >= SIGNAL_POLL_INTERVAL {
            self.signal_elapsed = Duration::ZERO;
            self.music_playing = music_is_playing();
        }
    }

    fn set_status(&mut self, focus: Option<FocusContext>, charging: bool) {
        self.focus = focus;
        self.charging = charging;
    }

    fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }

    fn set_idle(&mut self, idle: bool) {
        self.idle = idle;
    }

    fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }

    fn set_preferences(&mut self, paused: bool, quiet: bool) {
        self.paused = paused;
        self.quiet = quiet;
    }

    fn trigger_warning(&mut self) {
        self.warning_for = Duration::from_secs(4);
    }

    fn trigger_reaction(&mut self, animation: SpriteLoop) {
        self.reaction = Some((animation, Duration::from_secs(3)));
    }

    fn start_next_mode(&mut self, width: f64, height: f64) {
        let floor_y = height - f64::from(OWL_SIZE_PX) - SCREEN_MARGIN_PX;
        if let PetMode::Fly { target_x, .. } = self.mode {
            // A flight always finishes on the ground before a new behavior can begin.
            self.x = target_x;
            self.y = floor_y;
        }
        self.mode_elapsed = Duration::ZERO;
        let maximum_x = width - f64::from(OWL_SIZE_PX) - SCREEN_MARGIN_PX;
        self.mode = match self.next_random() % 100 {
            0..35 => {
                self.mode_duration = Duration::from_secs(2 + self.next_random() % 5);
                PetMode::Rest
            }
            35..70 => {
                self.mode_duration = Duration::from_secs(3 + self.next_random() % 5);
                PetMode::Walk {
                    target_x: self.random_between(SCREEN_MARGIN_PX, maximum_x),
                }
            }
            _ => {
                self.mode_duration = Duration::from_secs(2 + self.next_random() % 4);
                PetMode::Fly {
                    start_x: self.x,
                    target_x: self.random_between(SCREEN_MARGIN_PX, maximum_x),
                    arc_height: self.random_between(height * 0.18, height * 0.52),
                }
            }
        };
    }

    fn next_random(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn random_between(&mut self, minimum: f64, maximum: f64) -> f64 {
        let fraction = self.next_random() as f64 / u64::MAX as f64;
        minimum + (maximum - minimum) * fraction
    }
}

fn session_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |time| {
            time.as_nanos() as u64 ^ u64::from(std::process::id())
        })
}

fn approach(value: f64, target: f64, maximum_step: f64) -> f64 {
    if value < target {
        (value + maximum_step).min(target)
    } else {
        (value - maximum_step).max(target)
    }
}

fn music_is_playing() -> bool {
    Command::new("playerctl")
        .arg("status")
        .output()
        .ok()
        .is_some_and(|output| output.status.success() && output.stdout == b"Playing\n")
}

fn install_overlay_style() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(".owl-overlay { background-color: transparent; }");
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("a display is required for the owl overlay"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn active_output_size() -> (i32, i32) {
    let display = gtk::gdk::Display::default().expect("a display is required for the owl overlay");
    let monitor = display
        .monitors()
        .item(0)
        .expect("at least one display output is required")
        .downcast::<gtk::gdk::Monitor>()
        .expect("display monitor type must be valid");
    let geometry = monitor.geometry();
    (geometry.width(), geometry.height())
}

fn frame_path(animation: SpriteLoop, frame: u8) -> PathBuf {
    let (directory, prefix) = match animation {
        SpriteLoop::Perch | SpriteLoop::Blink => ("perch_blink", "perch_blink"),
        SpriteLoop::Sleep => ("sleep_idle", "sleep_idle"),
        SpriteLoop::WalkLeft => ("walk_left", "walk_left"),
        SpriteLoop::WalkRight => ("walk_right", "walk_right"),
        SpriteLoop::Fly => ("wing_flap", "wing_flap"),
        SpriteLoop::Alert | SpriteLoop::Flinch => ("alert_flinch", "alert_flinch"),
        SpriteLoop::Idle => ("idle_loop", "idle_01"),
        SpriteLoop::Party => ("fun_party_loop", "dance_start"),
        SpriteLoop::Working => ("working_loop", "typing"),
        SpriteLoop::Thinking => ("thinking_loop", "neutral"),
        SpriteLoop::Warning => ("warning_to_error", "caution"),
        SpriteLoop::Charging => ("charging_loop", "charge_start"),
        SpriteLoop::Presence => ("companion_presence", "presence_01"),
        SpriteLoop::Success => ("success_celebration", "success_notice"),
        SpriteLoop::Notification => ("notification_cycle", "message"),
        SpriteLoop::Silly => ("silly_loop", "playful_grin"),
    };
    let root = match animation {
        SpriteLoop::Idle
        | SpriteLoop::Party
        | SpriteLoop::Working
        | SpriteLoop::Thinking
        | SpriteLoop::Warning
        | SpriteLoop::Charging
        | SpriteLoop::Presence
        | SpriteLoop::Success
        | SpriteLoop::Notification
        | SpriteLoop::Silly => "assets/sprites/companion-v2",
        _ => "assets/sprites/clockwork-owl",
    };
    let filename = match animation {
        SpriteLoop::Idle => format!("{frame:02}_idle_{:02}.png", frame + 1),
        SpriteLoop::Party => [
            "00_dance_start.png",
            "01_dance_sway.png",
            "02_spin.png",
            "03_hop.png",
            "04_dance_reset.png",
        ][frame as usize]
            .to_owned(),
        SpriteLoop::Working => [
            "00_ready_keyboard.png",
            "01_typing.png",
            "02_scan_panel.png",
            "03_confirm_task.png",
            "04_ready_loop.png",
        ][frame as usize]
            .to_owned(),
        SpriteLoop::Thinking => [
            "00_neutral.png",
            "01_glance.png",
            "02_thinking.png",
            "03_processing.png",
            "04_insight.png",
        ][frame as usize]
            .to_owned(),
        SpriteLoop::Warning => [
            "00_caution.png",
            "01_concerned.png",
            "02_warning_high.png",
            "03_error.png",
            "04_critical_error.png",
        ][frame as usize]
            .to_owned(),
        SpriteLoop::Charging => [
            "00_charge_start.png",
            "01_charge_build.png",
            "02_charge_peak.png",
            "03_charge_stable.png",
            "04_charge_rest.png",
        ][frame as usize]
            .to_owned(),
        SpriteLoop::Presence => format!("{frame:02}_presence_{:02}.png", frame + 1),
        SpriteLoop::Success => [
            "00_success_notice.png",
            "01_smile.png",
            "02_hop.png",
            "03_confetti.png",
            "04_happy_wink.png",
        ][frame as usize]
            .to_owned(),
        SpriteLoop::Notification => [
            "00_message.png",
            "01_reminder.png",
            "02_mention.png",
            "03_incoming_call.png",
            "04_urgent_alert.png",
        ][frame as usize]
            .to_owned(),
        SpriteLoop::Silly => [
            "00_playful_grin.png",
            "01_wink.png",
            "02_tongue_out.png",
            "03_goofy_tilt.png",
            "04_grin_reset.png",
        ][frame as usize]
            .to_owned(),
        _ => format!("{prefix}_{frame:02}.png"),
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(root)
        .join(directory)
        .join(filename)
}

fn dispatch_task_event(event: crate::collectors::TaskEvent, life: &Rc<RefCell<PetLife>>) {
    let event = task_event(event);
    let _ = record(&event);
    life.borrow_mut().trigger_reaction(animation_for(&event));
}

fn load_frame(animation: SpriteLoop, frame: u8) -> gdk_pixbuf::Pixbuf {
    gdk_pixbuf::Pixbuf::from_file_at_scale(
        frame_path(animation, frame),
        OWL_SIZE_PX,
        OWL_SIZE_PX,
        true,
    )
    .unwrap_or_else(|_| {
        gdk_pixbuf::Pixbuf::from_file_at_scale(
            frame_path(SpriteLoop::Perch, 0),
            OWL_SIZE_PX,
            OWL_SIZE_PX,
            true,
        )
        .expect("the base owl sprite must be available")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_destinations_stay_within_the_requested_bounds() {
        let mut life = PetLife {
            rng: 1,
            ..PetLife::default()
        };
        for _ in 0..100 {
            let destination = life.random_between(16.0, 2418.0);
            assert!((16.0..=2418.0).contains(&destination));
        }
    }

    #[test]
    fn flight_steps_do_not_overshoot_their_target() {
        assert_eq!(approach(10.0, 20.0, 50.0), 20.0);
        assert_eq!(approach(20.0, 10.0, 50.0), 10.0);
    }

    #[test]
    fn a_completed_flight_lands_before_the_next_behavior() {
        let mut life = PetLife {
            x: 80.0,
            y: 80.0,
            mode: PetMode::Fly {
                start_x: 80.0,
                target_x: 700.0,
                arc_height: 250.0,
            },
            initialized: true,
            mode_elapsed: Duration::ZERO,
            mode_duration: Duration::from_secs(2),
            signal_elapsed: Duration::ZERO,
            music_playing: false,
            music_elapsed: Duration::ZERO,
            focus: None,
            warning_for: Duration::ZERO,
            reaction: None,
            charging: false,
            locked: false,
            idle: false,
            fullscreen: false,
            paused: false,
            quiet: false,
            rng: 1,
        };

        let state = life.tick(Duration::from_secs(2), 1000.0, 800.0);
        assert_eq!(state.y, 800.0 - f64::from(OWL_SIZE_PX) - SCREEN_MARGIN_PX);
    }

    #[test]
    fn active_sprite_is_decoded_at_the_configured_display_size() {
        let frame = load_frame(SpriteLoop::Perch, 0);
        assert_eq!(frame.width(), OWL_SIZE_PX);
        assert_eq!(frame.height(), OWL_SIZE_PX);
    }

    #[test]
    fn music_dance_is_an_accent_not_a_permanent_override() {
        let mut life = PetLife {
            music_playing: true,
            music_elapsed: Duration::from_secs(6),
            ..PetLife::default()
        };
        assert_ne!(
            life.tick(Duration::ZERO, 1000.0, 800.0).animation,
            SpriteLoop::Party
        );
    }

    #[test]
    fn parses_routines_from_settings_text() {
        assert_eq!(
            parse_routines("Wrap up@17:30\nBreak@09:05").unwrap(),
            vec![
                crate::settings::Routine {
                    name: "Wrap up".to_owned(),
                    hour: 17,
                    minute: 30,
                },
                crate::settings::Routine {
                    name: "Break".to_owned(),
                    hour: 9,
                    minute: 5,
                },
            ]
        );
    }
}
