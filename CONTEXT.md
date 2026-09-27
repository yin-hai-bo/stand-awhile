# Project context

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
current MVP uses the `idle`, `jump`, and later `walk` clips from the cat asset.

### Animation player

The pure time-based component that advances an Animation clip. It consumes a
monotonic timestamp and returns the selected frame; it does not know about
HWNDs, files, timers, or rendering.

### Reminder animation

The one-shot `jump` clip played when the standing timer reaches zero. After it
finishes, the Pet returns to `idle` until the user clicks it.

### Renderer

The component that turns a renderable frame into pixels submitted to the Pet
window. The MVP renderer uses CPU BGRA buffers and `UpdateLayeredWindow`.

### Window interaction

User input delivered to the Pet window. MVP interaction consists of dragging
with the left button, clicking to acknowledge a reminder, and a small
right-click context menu.

### Acknowledge

The user action of clicking the Pet after a reminder. Acknowledging hides the
Pet and starts the next timer interval.
