# Project context

## Implementation map

The application is Rust with native Win32 GUI APIs. Startup asset decoding,
window messages, animation updates, and rendering run on the main thread.

| Module | Responsibility |
| --- | --- |
| `main.rs` | DPI awareness, configuration, GDI+ startup, window creation, message loop |
| `single_instance.rs` | Session-local instance mutex, bounded existing-window lookup, activation message |
| `startup.rs` | `--autostart` parsing, duplicate-launch policy, initial visibility and countdown start |
| `autostart.rs` | Current-user Run entry synchronization, configuration rollback and startup error messages |
| `window_proc.rs` | Countdown state, Pet commands, settings application, main window lifetime |
| `settings.rs`, `config.rs` | Settings UI and UTF-8 JSON persistence in Roaming AppData |
| `asset.rs`, `build.rs` | Embedded manifest and PNGs; in-memory GDI+ decoding and character catalog |
| `animation.rs` | Pure `Instant`-driven frame selection, looping, pause/resume, completion |
| `pet_window.rs` | Pet window, clip transitions, entrance/exit motion, dragging, alpha hit testing, position persistence |
| `render.rs`, `gdi/` | CPU pixel surfaces, DPI resampling, reusable DIBs, `UpdateLayeredWindow` |
| `speech_bubble.rs`, `speech_bubble_window.rs` | Fixed message cycle and localized bubble window |
| `timer_panel.rs`, `tray_icon.rs`, `about.rs`, `ui/` | Countdown display, tray controls, About, Win32 controls and drawing |

The main countdown uses a one-second Win32 timer to decrement its remaining
seconds. Pet animation uses a 16 ms window timer to schedule updates and
`Instant` to select frames. There is no asset worker or GPU renderer.

The countdown intentionally accepts occasional delays of a few seconds to
ten or more seconds; exact wall-clock timing is not required. See
[ADR 0003](docs/adr/0003-countdown-timing-tolerance.md).

## Startup and timer behavior

Each ordinary launch shows the main window with the Pet hidden and waits for
the user to start the countdown. `--autostart` with `launch_at_startup=true`
starts a full countdown with the main window hidden from creation. An existing
instance handles ordinary duplicate launches by restoring its main window;
duplicate `--autostart` launches exit silently. Neither changes timer or Pet state.

`auto_hide_on_start` defaults to true and applies to every timer start or
resume from the main window, menus, or Pet acknowledgement. Background startup
stays hidden regardless of this preference. `tray_when_close` defaults to
true; closing hides the main window while the timer and Pet continue.
`launch_at_startup` defaults to false and controls synchronization of the
current-user Windows Run entry.

At completion, the displayed time returns to the configured full interval and
Reset is disabled. Reset during a countdown stops it, restores the interval,
and keeps the Pet hidden. Settings accepts whole minutes but configuration
stores seconds; an untouched input preserves the original seconds. Reset to
defaults preserves the current launch-at-startup preference.

## Domain glossary

### Pet

The visible desktop character shown in its own transparent window. A Pet is
not the existing countdown window and does not own timer policy.

### Pet window

The independent Win32 popup window used to display and receive input for the
Pet. It is layered, non-activating, and managed by the main application
window.

### Animation clip

A named ordered sequence of frames with a frame duration and loop mode. The
runtime uses `idle`, `walk`, and `jump` for either Cat or Dog. Only these clips'
56 PNG frames are retained and embedded, and the runtime catalog decodes all of them.

### Animation player

The pure time-based component that advances an Animation clip. It consumes a
monotonic timestamp and returns the selected frame; it does not know about
HWNDs, files, timers, or rendering.

### Reminder animation

The one-shot `jump` clip played when the standing timer reaches zero. After it
finishes and the entrance motion completes, the Pet switches to `walk`.
After a 500 ms delay, its fixed localized bubble appears for 5 seconds and
then stays hidden for 10 seconds, repeating until interrupted. Bubble
visibility selects `idle`; the hidden phase selects `walk`.

### Renderer

The component that turns a renderable frame into pixels submitted to the Pet
window. The renderer uses CPU premultiplied BGRA buffers and `UpdateLayeredWindow`.

### Window interaction

User input delivered to the Pet window: left-button dragging, clicking to
start the next countdown, and a right-click menu with timer, main-window,
Settings, About, and Exit commands. Losing capture cancels the drag.

### Acknowledge

The user action of clicking the visible Pet. Acknowledging hides the Pet
with its exit animation and starts a full timer interval. The main window
also hides if auto-hide is enabled. The Pet is hidden on startup and appears
when a countdown completes.
