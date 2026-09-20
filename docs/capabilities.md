# Ikaros Capability Roadmap

## Available now

- local memory, one-minute load, root disk availability, battery state, and NetworkManager connection state;
- MPRIS playback state through `playerctl`, without track metadata or history;
- desktop notifications for low battery, memory pressure, low disk space, and a connection-loss transition;
- local autonomous movement and approved-command model.
- opt-in Hyprland focused-application and fullscreen detection without window titles;
- browser summaries through an explicit native-messaging extension;
- selected-directory storage analysis and download monitoring (`IKAROS_WATCH_DIRECTORY`);
- selected local `.ics` calendar start-time reminders, manual routines, break reminders, and explicitly named systemd user units configured through `--configure`.

## Next integrations

- a status window showing the current local snapshot and per-alert controls;
- UPower and NetworkManager event subscriptions to replace polling where practical;
- update summaries and user-defined, allowlisted maintenance actions.

## Boundaries

Ikaros must not read arbitrary files, browser contents, microphone/camera data, or execute commands automatically. Each new source and action requires an independent visible permission and an activity log.
