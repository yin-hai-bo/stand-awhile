# Project context

## Implementation map

The application is Rust with native Win32 GUI APIs. On-demand asset decoding,
window messages, animation updates, and rendering run on the main thread.

| Module | Responsibility |
| --- | --- |
| `main.rs` | DPI awareness, configuration, GDI+ startup, window creation, message loop |
| `single_instance.rs` | Session-local instance mutex, bounded existing-window lookup, activation message |
| `startup.rs` | `--autostart` parsing, duplicate-launch policy, initial visibility and countdown start |
| `autostart.rs` | Current-user Run entry synchronization, configuration rollback and startup error messages |
| `window_proc.rs` | Countdown state, Pet commands, settings application, main window lifetime |
| `countdown.rs` | Pure countdown transitions, remaining/displayed seconds, and button availability; owned by the main window |
| `settings.rs`, `config.rs` | Settings UI and UTF-8 JSON persistence in Roaming AppData |
| `asset.rs`, `build.rs` | Embedded manifest and PNGs; in-memory GDI+ decoding of the selected character |
| `animation.rs` | Pure `Instant`-driven frame selection, looping, pause/resume, completion |
| `pet_window.rs` | Pet window, clip transitions, entrance/exit motion, dragging, alpha hit testing, position persistence |
| `render.rs`, `gdi/` | CPU pixel surfaces, DPI resampling, reusable DIBs, `UpdateLayeredWindow` |
| `speech_bubble.rs`, `speech_bubble_window.rs` | Fixed message cycle and localized bubble window |
| `timer_panel.rs`, `tray_icon.rs`, `about.rs`, `ui/` | Countdown display, tray controls, About, Win32 controls and drawing |

Startup does not decode animation frames or create the Pet window. At countdown
completion, the main thread decodes only the selected character and moves its
animations into a newly created Pet window. Immediate hiding or completion of
the exit animation destroys that window and releases its frames, scaled cache,
and render surfaces. The next reminder reloads the selected character and
restores the last position. Changing character while a Pet exists loads its
replacement synchronously; without a Pet, it only updates the selection.

The main countdown uses a one-second Win32 timer to decrement its remaining
seconds. Its state and remaining seconds live in `WindowState.countdown`, without
a global timer-state mutex or remaining-seconds atomic. A startup-only atomic
provides the initial display value before the window state is attached.
Main-window reads return owned snapshots, and mutations use short closures that
do not call window APIs. Pet, GDI+, and component snapshots share ownership with
`Rc`, so synchronous callbacks can release the attached state without invalidating
an operation in progress. No animation pixel buffers are cloned. `WM_NCDESTROY`
clears the attached pointer before dropping state and its resources.
Pet-window mutations also use short closures. Position, capture, visibility,
render submission, bubble updates, and menu tracking run outside Pet state
borrows. Renderer, render surface, and bubble controller handles use `Rc` to
keep an operation's resources alive through callbacks. Nested rendering waits
for the next timer tick; pending bubble starts and language changes are retained.
Pet destruction clears its attached pointer before dropping resources. Its owner
still releases a hidden Pet asynchronously, rejecting stale hide notifications.
Settings-window calls also use owned snapshots; local changes are committed in
short closures before notifying the main window. Its brushes and owned title
fonts use `Rc` so callbacks that close the panel cannot release resources still
used by an operation. The attached pointer is cleared before close notification
and cleanup. Ordinary setting changes preserve an unsaved minutes input; startup
preference failures still roll back saved configuration even if the panel closes.
Bubble-window reads use owned snapshots, and mutations use short closures before
layout or visibility calls. Fonts, brush, and pen share ownership through `Rc`
until in-flight drawing or layout ends. A layout revision rejects stale work
after synchronous callbacks; nested updates invalidate the position cache so
the next update reapplies the current layout. Destruction clears the attached
pointer before releasing state, including when the Pet owner destroys its bubble.
Pet animation uses a 16 ms window timer only while the Pet window
is visible, including its entrance and exit animations, and `Instant` to
select frames. Hiding the Pet stops that timer; queued timer messages do
not update or render a hidden Pet. There is no asset worker or GPU renderer.

At non-native DPI, looping Idle/Walk frames are resampled lazily once and
cached without their transparent padding. Restoring a cached frame preserves
the full surface pixels and alpha hit testing. Changing character or DPI
clears the cache; native-size frames and the one-shot Jump are not cached.

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
56 PNG frames are retained and embedded; each reminder decodes only the selected
character's 28 frames, without cloning their pixel buffers.

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
