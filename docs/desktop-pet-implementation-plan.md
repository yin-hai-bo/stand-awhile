# Desktop Pet implementation plan

## Outcome

Add a desktop Pet to Stand Awhile. The Pet is an independent transparent
Win32 window managed by the existing main window. When the timer reaches zero,
the Pet appears, plays one `jump` animation, and remains idle until clicked.
Clicking acknowledges the reminder, hides the Pet, and starts the next timer
interval.

The MVP deliberately excludes autonomous movement, Direct2D/Direct3D,
position persistence, multi-pet support, and new Pet settings. Toast and tray
balloon reminders are removed because the Pet becomes the only reminder
channel.

## Commit rules

Each STEP below is intended to be one independently reviewable commit. A STEP
must compile and pass the relevant tests before the next STEP starts. Do not
combine unrelated cleanup with a STEP. Every Rust change requires `cargo fmt`.

## Dependency map

```text
STEP 1  animation domain
   ↓
STEP 2  asset loading and frame preparation
   ↓
STEP 3  layered renderer
   ↓
STEP 4  independent Pet window shell
   ↓
STEP 5  animation playback
   ↓
STEP 6  dragging, click acknowledgement, context menu
   ↓
STEP 7  timer integration and Toast removal
   ↓
STEP 8  DPI, monitor bounds, and lifecycle hardening
   ↓
STEP 9  verification, diagnostics, and documentation
```

## STEP 1 — Extract the animation domain ✅ 已完成

Goal: create a platform-independent animation model and clock-driven player.

Likely changes:

- Add an `animation` module.
- Define clip, frame reference, loop mode, playback state, and frame
  selection types.
- Implement `AnimationPlayer::update(now)` using `Instant` or an injected
  elapsed duration.
- Support looping `idle`/`walk` and one-shot `jump`.
- Keep the module independent of Win32, image decoding, and file paths.

Tests:

- frame selection before, between, and after frame boundaries;
- loop wraparound;
- one-shot completion and stable final state;
- pause/reset behavior if included in the chosen API;
- zero-duration and empty-clip validation.

Review boundary: only pure animation types and tests. No window or asset code.

## STEP 2 — Load and prepare the character assets ✅ 已完成

Goal: turn the checked-in character manifest and PNG files into renderable
frames without reading files during animation ticks.

Likely changes:

- Add an `asset` module.
- Parse `assets/pets/cat-dog/manifest.json` for all declared characters.
- Resolve numbered frame paths deterministically; do not rely on directory
  enumeration order.
- Decode PNG frames with the existing Windows graphics stack.
- Convert frames to a consistent premultiplied BGRA representation.
- Keep only the MVP clips (`idle` and `jump`) in the initial runtime cache;
  retain the other asset files for later STEPs.

Tests:

- manifest parsing and frame ordering;
- missing file and malformed manifest errors;
- premultiplied-alpha conversion on representative pixels;
- cache does not reload a frame on repeated lookup.

Review boundary: asset ownership, decoding, and cache API. No HWND creation.

## STEP 3 — Implement the CPU layered renderer ✅ 已完成

Goal: submit one prepared BGRA frame to a transparent top-level window.

Likely changes:

- Add a renderer module with a small `Renderer` seam.
- Allocate/reuse a 32-bit BGRA DIB or equivalent CPU surface.
- Composite a frame into the surface with correct alpha and baseline/pivot
  handling.
- Call `UpdateLayeredWindow` with `AC_SRC_ALPHA`.
- Reuse buffers when the frame size is unchanged.
- Keep all unsafe Win32/GDI code inside the renderer/window boundary.

Tests and checks:

- pure surface sizing and frame placement tests;
- premultiplied-alpha color checks;
- a manual smoke check showing one cat frame with transparent edges;
- no repeated allocation when submitting the same-size frame.

Review boundary: static rendering only. No timer integration or drag logic.

## STEP 4 — Create the independent Pet window shell ✅ 已完成

Goal: create and destroy a usable Pet window without changing reminder logic.

Likely changes:

- Add a Pet window module and registration path.
- Create a `WS_POPUP` window with `WS_EX_TOOLWINDOW`, `WS_EX_LAYERED`, and
  `WS_EX_NOACTIVATE`.
- Use `HWND_TOPMOST` without activating the window.
- Place it at the bottom-right of the primary work area with a margin.
- Show one static frame from the selected character through the renderer.
- Make shutdown and `WM_NCDESTROY` ownership explicit.

Tests and checks:

- create/show/hide/destroy smoke test on Windows;
- verify the main window remains the foreground window;
- verify the Pet is absent from Alt+Tab;
- verify the transparent background is not an opaque rectangle.

Review boundary: window lifetime, styles, placement, and renderer hookup.

## STEP 5 — Connect animation playback to the Pet window ✅ 已完成

Goal: display the selected character's `idle` animation and one-shot `jump` without blocking the
message loop.

Likely changes:

- Give the Pet controller ownership of the current clip and player.
- Use a small window timer only to schedule updates; calculate actual frame
  progress from monotonic time.
- Submit a new frame only when the selected frame changes.
- Implement `idle → jump → idle` transitions.
- Use the agreed initial durations: idle 120 ms per frame, jump 100 ms per
  frame.

Tests and checks:

- transition tests independent of HWNDs;
- one-shot jump returns to idle;
- timer jitter does not cause frame progression to depend on callback count;
- manual check that the message loop remains responsive.

Review boundary: animation/controller integration only; no timer policy change.

## STEP 6 — Add Pet interaction ✅ 已完成

Goal: make the Pet draggable and acknowledgeable.

Likely changes:

- Handle left-button press/move/release in the Pet window.
- Drag the window using screen-coordinate deltas without activating it.
- Treat a release with a small movement as a click.
- Add a minimal right-click menu: show main window, exit.
- Keep MVP hit testing rectangular; defer alpha hit testing and metadata
  hitboxes.
- Keep click and drag thresholds as named constants with unit-testable helper
  logic.

Tests and checks:

- click-vs-drag classification;
- movement preserves the initial pointer offset;
- right-click commands route to the main application;
- clicking transparent pixels is explicitly documented as deferred behavior.

Review boundary: input and command routing. No timer reset implementation.

## STEP 7 — Integrate the reminder loop and remove Toast ✅ 已完成

Goal: make the Pet the only timer-completion reminder.

Likely changes:

- Extend the existing timer completion path to show the Pet and play `jump`.
- Hide the Pet with a short slide-out animation when a timer starts.
- Keep the timer running while the main window is hidden to the tray.
- On Pet acknowledgement, reset the remaining duration and automatically
  start the next interval.
- Hide the Pet on pause, reset, application close, and shutdown.
- Remove Toast initialization, Toast display, tray balloon fallback, and
  reminder-only localized strings/methods.
- Keep the normal tray icon and tray menu.

Tests and checks:

- timer completion requests the Pet and never invokes Toast;
- acknowledgement starts exactly one new timer interval;
- pause/reset/close hide the Pet;
- hidden-to-tray mode still produces a Pet reminder;
- existing countdown button behavior remains correct.

Review boundary: timer/Pet coordination and notification removal.

## STEP 8 — Add DPI, monitor bounds, and lifecycle hardening ✅ 已完成

Goal: make the Pet reliable on high-DPI and multi-monitor desktops.

Likely changes:

- Keep the process and main window DPI-aware without scaling the Pet's
  pixel-sized PNG assets a second time.
- Allow dragging across monitors and select the target monitor from the
  pointer position.
- Keep at least a visible portion of the Pet during dragging, while allowing
  the rest of it to be clipped by the screen edge.
- Keep the initial placement on the primary monitor.
- Move a partially clipped Pet back into the work area before a reminder.
- Ensure Pet destruction precedes renderer/resource teardown.
- Verify topmost behavior does not use an infinite foreground/topmost loop.

Tests and checks:

- clamping at each monitor edge;
- partially clipped drag positions retain a visible portion;
- manual two-monitor and mixed-scale smoke checks.

Review boundary: coordinate conversion, monitor selection, and shutdown.

## STEP 9 — Final verification and follow-up seams ✅ 已完成

Goal: make the MVP easy to review and leave measured extension points.

Likely changes:

- Add focused diagnostics for frame update time, submit time, and cache size.
- Document the Pet lifecycle and asset contract.
- Add or update Windows-only smoke-test instructions.
- Run `cargo fmt`, `cargo test`, and a release build check as appropriate.
- Record known deferred work: walk/autonomous movement,
  settings, and Direct2D/
  DirectComposition evaluation.

Acceptance checklist:

- application starts with Pet visible while the timer has not started;
- timer completion shows the selected character and plays `jump`;
- Pet can be dragged and clicked;
- click hides Pet and starts the next interval;
- tray-hidden mode still works;
- no Toast or tray balloon reminder remains;
- transparent edges and high-DPI placement are correct;
- configured speech messages cycle with their display durations and hidden gaps;
- all focused tests pass.

## Deferred work after the MVP

- ✅ 原地播放 `walk` 动画且不改变窗口位置；
- ✅ dog 角色选择与通用角色目录；角色通过配置文件中的 `character` 字段选择，默认值为 `cat`；
- ✅ 可配置气泡文本；消息支持独立显示时长，消息之间支持配置隐藏间隔；
- ✅ 基于 alpha 的命中测试与逐帧 hitbox；透明像素不会触发 Pet 交互；
- ✅ 保存 Pet 位置；记录显示器设备标识、工作区相对坐标和屏幕坐标，并在显示器不可用时回退；
- Pet settings;
- richer context menu and interaction states;
- Direct2D/DirectComposition only after profiling demonstrates a real need;
- asset worker/background loading if startup measurements justify it.
