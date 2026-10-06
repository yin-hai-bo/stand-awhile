# ADR 0001: Use an independent CPU-rendered layered Pet window

## Status

Accepted

## Decision

The desktop Pet is implemented as an independent Win32 top-level popup window.
The implementation uses `WS_EX_LAYERED` with CPU premultiplied BGRA buffers
submitted by `UpdateLayeredWindow`. The existing main window owns the Pet lifecycle and
timer coordination. `render.rs` and `gdi/` handle pixel surfaces and
layered-window submission; `pet_window.rs` handles Win32 input, placement,
and clip transitions. `animation.rs` selects frames independently of Win32.
There is currently no renderer trait or GPU backend.

## Context

The project needs a transparent, draggable, animated character on Windows.
The selected assets are PNG frame sequences. One Pet is displayed at a time,
with Cat and Dog available as character choices. The application has
low animation frequency and no particle or vector-heavy effects.

Direct2D can use hardware acceleration, but a traditional layered window still
needs a CPU-accessible premultiplied BGRA surface when using
`UpdateLayeredWindow`. Adding Direct2D before measuring the actual workload
would add device/resource lifecycle complexity without a guaranteed reduction
in total CPU or memory traffic.

## Consequences

Positive:

- smallest path to validate transparency, animation, input, and timer behavior;
- no third-party GUI framework;
- pure animation frame selection remains independent of Win32;
- rendering submission is isolated in `render.rs` and `gdi/`.

Negative:

- continuous complex effects may eventually become CPU-bound;
- the application does not exercise a GPU composition path;
- the renderer must correctly maintain premultiplied alpha and reusable buffers.

## Revisit condition

Measure frame update time, layered-window submit time, memory usage, and missed
frame intervals when evaluating a performance issue. Revisit Direct2D or
DirectComposition only if those measurements show a user-visible problem.
