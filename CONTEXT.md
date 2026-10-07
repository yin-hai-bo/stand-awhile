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
runtime uses `idle`, `walk`, and `jump` for either Cat or Dog. All PNG frames
are embedded, while the runtime catalog decodes these three clips per character.

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
with its exit animation and starts a full timer interval. This also works
for the initially visible Pet before the first countdown.
