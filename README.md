# Ikaros Virtual

Ikaros is a Linux-only pixel-art owl companion for this laptop. It roams while the desktop is active and returns to a bottom corner when idle. It observes selected system information and may perform only actions the user has explicitly approved.

## Current slice

The buildable Rust core reads memory telemetry from `/proc/meminfo`, advances named sprite loops, rate-limits threshold alerts, and establishes a permission boundary. It intentionally has no broad file, browser, or command access. Command proposals require both the command capability and a user approval; execution is not implemented yet.

Run it with:

```bash
cargo run
```

On Hyprland, this opens a transparent layer-shell owl. It varies its rest time and randomly chooses ground and flight destinations across the active display; it also bobs to MPRIS music detected through `playerctl`. Press `Ctrl+C` in the launching terminal to close it.

Run tests with:

```bash
cargo test
```

## Start on login

The systemd user unit is stored at `systemd/ikaros-virtual.service`. After building the release binary, enable it with:

```bash
systemctl --user enable --now /home/lucaspinacosta/.Applications/ikaros_virtual/systemd/ikaros-virtual.service
```

Check it with `systemctl --user status ikaros-virtual.service`. Disable automatic startup with `systemctl --user disable --now ikaros-virtual.service`.

## Hyprland overlay

The graphical layer will use GTK 4 plus `gtk4-layer-shell`, which implements the wlroots layer-shell protocol used by Hyprland. Install the Arch development package before adding that adapter:

```bash
sudo pacman -S gtk4-layer-shell
```

The overlay will be transparent, non-focusable, and click-through while the owl is roaming. Clicking the owl will temporarily enable its input region to open its status and permission controls.

## Sprite assets

The active recreated owl sprites are stored in `assets/sprites/clockwork-owl/` as transparent 256x256 PNG frames. The graphical adapter should load its `animation-manifest.json` rather than infer frame names. `assets/sprites/test-clockwork/` is legacy material and must not be used by new renderer code.

## Permission model

Enabled by default:

- read-only system telemetry
- local desktop notifications

Explicit opt-in only:

- selected files or directories, never the entire home directory by default
- browser summaries through a browser extension and native messaging
- incoming desktop-notification summaries (`IKAROS_READ_NOTIFICATIONS=1`)
- allowlisted, reviewed commands

Every approved action should be visible in an activity log and revocable in settings.

For the current command-line configuration, set `IKAROS_WATCH_DIRECTORY` to one explicitly selected directory to enable its file/download watcher. Existing files are ignored during its first scan; a newly observed file produces a creation event and a completion event after two unchanged polling cycles.

Firefox-family browsers, including Zen, use `browser-extension-firefox/`. Chromium-based browsers use `browser-extension/`.

See `docs/capabilities.md` for the implemented local telemetry and the remaining opt-in integrations.

## Next implementation steps

1. Add the GTK layer-shell overlay and display a placeholder sprite.
2. Add pixel sprite sheets and an animation state machine for perch, walk, fly, blink, and alert.
3. Poll UPower, NetworkManager, disk space, and CPU load with rate-limited notification rules.
4. Add a settings window for per-capability permissions and an approved-action allowlist.
5. Build a browser extension using native messaging for browser-specific, opt-in events.
