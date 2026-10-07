# Stand Awhile (Chinese name: 站一站)

![Platform](https://img.shields.io/badge/platform-Windows-blue)
![License](https://img.shields.io/badge/license-MIT-green)

**StandAwhile** (Chinese name: 站一站) is a lightweight Windows desktop utility that gently reminds you to stand up, stretch, and move every 20 minutes.

> “Stand awhile, breathe, and stretch.”

It reminds you to take a movement break without taking focus away from your work.

---

## ✨ Features

- ⏱ **Smart Timer**: Default 20-minute interval (fully configurable)
- 🐾 **Desktop Pet Reminders**: Show a transparent animated Pet when the timer completes
- 🪶 **Tray Controls**: Start the timer, show the main window, open Settings or About, and exit
- 🛠 **Customizable**:
  - Adjust reminder intervals
  - Choose the Pet character (`cat` or `dog`)
  - Choose the application language and theme
  - Choose whether closing the main window exits or hides it to the tray
- 🖥 **Native Experience**: Pure Windows application, no browser dependencies

---

## 📸 Screenshot

![StandAwhile Screenshot](docs/images/screenshot.jpg)

<img src="docs/images/settings.jpg" style="width:auto; max-width:none;">

---

## 🚀 Getting Started

### Download & Run
1. Go to the [Releases](https://github.com/yin-hai-bo/stand-awhile/releases) page
2. Download the executable (extract it first if distributed in an archive)
3. Run `stand-awhile.exe`
4. The main window appears with the Pet hidden; click Play to start the countdown

The executable includes the Cat and Dog images and animation manifest. You can
distribute the EXE alone; no external `assets` folder is required at runtime.

Only one instance runs per Windows sign-in session. Opening the app again restores
the existing main window without restarting the countdown or changing the Pet.
If the first instance is still starting, the second waits up to 5 seconds for its
window; if unavailable, it reports an error instead of starting another instance.

For background startup, run `stand-awhile.exe --autostart` with
`launch_at_startup` set to `true`. The main window stays hidden from creation,
the tray icon appears, and a full countdown starts immediately. This initial
hidden state applies regardless of the auto-hide preference. The Pet appears
when the countdown ends; the tray can restore the main window at any time.
Without the argument, or with the preference disabled, startup is manual.
An `--autostart` invocation while an instance is running exits silently,
regardless of the preference, without changing its windows or countdown.
Enable **Launch at startup** in Settings to use this mode automatically at Windows sign-in.

The Pet is the application's reminder channel. It can be dragged, clicked to
acknowledge a reminder, or controlled through its context menu. The application
does not use Windows toast notifications or tray balloon reminders.

Click the Pet to hide it and start a new countdown. Its speech bubble shows a
fixed reminder and this click instruction in the selected application language
(Chinese or English). Bubble text and timing are built in; the former
`speech_bubble` configuration is ignored and removed when configuration is saved.
The Pet starts with its Walk animation, switches to Idle while the bubble is
visible, and returns to Walk when the bubble disappears.
When a countdown ends, the Pet flies in from the nearest top or bottom screen
edge with Jump, then switches to Walk before the bubble cycle starts.

On the first countdown start of each app session, clicking the Pet or choosing
**Start** from either the Pet or tray menu also hides the main window to the
tray. Starting with the main window's Play button keeps the window visible.
Later starts do not automatically hide it, including after pause or reset.
This behavior is independent of the close-to-tray setting.

Pause stops the countdown and hides the Pet. Reset restores the configured
interval, stops the countdown, and keeps the Pet hidden. Closing the main window
exits by default. Enable **Minimize to tray** in Settings to hide only the main
window; the countdown and any visible Pet reminder continue.

### Settings and Configuration

Open **Settings** from the main window, tray menu, or Pet menu. Character,
language, theme, close behavior, and auto-hide changes are saved and applied immediately.
The auto-hide Yes/No options share the main window checkbox's preference.
**Launch at startup** has Yes/No options, defaulting to No. Selecting Yes registers
the quoted current EXE path with `--autostart` in the current user's
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, under `Yinhaibo.StandAwhile`.
Selecting No removes that value. Administrator privileges are not required.
Changing this option preserves the current countdown and window state. If the
registry update fails, the previous configuration and radio selection are
restored and an error is shown; a failed configuration restore is reported too.
Each primary instance also synchronizes the entry from the configuration, so
manually opening a moved EXE updates its registered path. Startup sync errors
are shown while the app continues running. The app does not change Windows'
separate enabled/disabled status for startup apps.
Settings accepts whole minutes (at least 1), converted to seconds when you click
**Back**. Existing intervals are displayed rounded up to minutes; leaving the
input unchanged preserves the original seconds. The countdown always displays
the exact seconds. Changing the interval replaces the remaining countdown;
changing other settings preserves it.

The UTF-8 configuration file is `%APPDATA%\yinhaibo\stand-awhile\config.json`.
It is created on first launch. For manual changes, close the application, edit
the file, then restart. Omitted or null fields use the defaults below.

| Field | Default | Values / behavior |
| --- | --- | --- |
| `period` | `1200` | Countdown interval in seconds |
| `character` | `"cat"` | `"cat"` or `"dog"`; unknown characters fall back to Cat |
| `language` | `"auto"` | `"auto"`, `"zh"`, or `"en"`; Auto follows the Windows UI language, using Chinese for Chinese locales and English otherwise |
| `theme` | `"system"` | `"system"`, `"light"`, or `"dark"` |
| `tray_when_close` | `true` | `true` hides the main window on close; `false` exits |
| `launch_at_startup` | `false` | Registers Windows sign-in startup with `--autostart`, starting a hidden background countdown |

Runtime information is stored separately in the UTF-8 file
`%LOCALAPPDATA%\yinhaibo\stand-awhile\state.json`.
It records `pet_position` automatically after dragging. Older
positions in `config.json` are ignored; the first launch without `state.json`
uses the default position.

### Build from Source

Prerequisites:

- Windows 10 or later
- Rust toolchain installed via [rustup](https://rustup.rs/)
- Microsoft C++ Build Tools, installed through Visual Studio Build Tools or Visual Studio

Clone and build:

```powershell
git clone https://github.com/yin-hai-bo/stand-awhile.git
cd stand-awhile
cargo build --release
```

Run from source:

```powershell
cargo run
```

The release executable will be generated at:

```text
target/release/stand-awhile.exe
```

Release builds require a clean Git working tree. For development with local
changes, use `cargo build` or `cargo run`. Image and manifest changes require a
rebuild because they are embedded in the executable.

For development checks, run:

```powershell
cargo fmt -- --check
cargo test
cargo build
```

See [Pet verification](docs/desktop-pet-verification.md) for Windows smoke checks
and [Project context](CONTEXT.md) for the implementation's module boundaries.
