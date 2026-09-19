# Activity Signals

Ikaros treats system context as short-lived signals, never as a record of user activity.

## Music

The current prototype invokes `playerctl status` every two seconds. It only checks whether an MPRIS player reports `Playing`; it does not read, store, or display track titles, artists, playlists, or playback history. While music is playing, the owl stays near its current perch and bobs to a synthetic beat using its perch frames.

## Coding and browsing

The next settings panel will offer an opt-in focused-application signal through Hyprland. It will inspect only the active window's application class, not its title or contents.

- coding applications: the owl settles nearby and uses a low-distraction perch loop;
- browsers: the owl continues normal roaming with less frequent alerts.

Browser-tab details require a separate browser extension and an explicit `BrowserSummary` permission. No browser integration is enabled in the current build.
