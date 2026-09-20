fn main() {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|argument| argument == "--configure")
    {
        if let Err(error) = configure(&arguments[1..]) {
            eprintln!("Ikaros configuration failed: {error}");
            std::process::exit(2);
        }
    } else if arguments.iter().any(|argument| argument == "--native-host") {
        if let Err(error) = ikaros_virtual::browser::run_native_host() {
            eprintln!("Ikaros native host failed: {error}");
        }
    } else if arguments
        .iter()
        .any(|argument| argument == "--status" || argument == "--settings")
    {
        ikaros_virtual::overlay::run_status_panel();
    } else {
        ikaros_virtual::overlay::run();
    }
}

fn configure(arguments: &[String]) -> Result<(), String> {
    use ikaros_virtual::settings::{QuietHours, Routine, Settings};

    let mut settings = Settings::load();
    match arguments {
        [option, value] if option == "--calendar" => settings.calendar_path = Some(value.into()),
        [option, value] if option == "--watch-unit" => {
            if !settings.watched_units.contains(value) {
                settings.watched_units.push(value.clone());
            }
        }
        [option, name, time] if option == "--routine" => {
            let (hour, minute) = parse_time(time)?;
            settings.routines.retain(|routine| routine.name != *name);
            settings.routines.push(Routine { name: name.clone(), hour, minute });
        }
        [option, minutes] if option == "--break-minutes" => {
            settings.break_reminder_minutes = Some(minutes.parse().map_err(|_| "break minutes must be a whole number")?);
        }
        [option, start, end] if option == "--quiet-hours" => {
            let start_hour = start
                .parse::<u8>()
                .map_err(|_| "quiet-hour start must be 0-23")?;
            let end_hour = end
                .parse::<u8>()
                .map_err(|_| "quiet-hour end must be 0-23")?;
            if start_hour > 23 || end_hour > 23 {
                return Err("quiet hours must use values from 0 to 23".to_owned());
            }
            settings.quiet_hours = Some(QuietHours {
                start_hour,
                end_hour,
            });
        }
        [option] if option == "--pause-hour" => settings.pause_for_one_hour(),
        [option] if option == "--resume" => settings.paused_until_unix_secs = None,
        _ => return Err("usage: --configure --calendar PATH | --watch-unit UNIT | --routine NAME HH:MM | --break-minutes MINUTES | --quiet-hours START END | --pause-hour | --resume".to_owned()),
    }
    settings.save().map_err(|error| error.to_string())
}

fn parse_time(value: &str) -> Result<(u8, u8), String> {
    let (hour, minute) = value.split_once(':').ok_or("routine time must use HH:MM")?;
    let hour = hour
        .parse::<u8>()
        .map_err(|_| "routine hour must be 0-23")?;
    let minute = minute
        .parse::<u8>()
        .map_err(|_| "routine minute must be 0-59")?;
    if hour > 23 || minute > 59 {
        return Err("routine time is out of range".to_owned());
    }
    Ok((hour, minute))
}
