# ADR 0001: Use an independent CPU-rendered layered Pet window for the MVP

## Status

Accepted

## Decision

The desktop Pet is implemented as an independent Win32 top-level popup window.
The MVP uses `WS_EX_LAYERED` with CPU BGRA buffers submitted by
`UpdateLayeredWindow`. The existing main window owns the Pet lifecycle and
timer coordination. The renderer is kept behind a small seam so a future
Direct2D/DirectComposition implementation can be evaluated without changing
the Pet controller or animation domain.

## Context

The project needs a transparent, draggable, animated character on Windows.
The selected assets are small PNG frame sequences. The MVP has one character,
low animation frequency, and no particle or vector-heavy effects.

Direct2D can use hardware acceleration, but a traditional layered window still
needs a CPU-accessible premultiplied BGRA surface when using
`UpdateLayeredWindow`. Adding Direct2D before measuring the actual workload
would add device/resource lifecycle complexity without a guaranteed reduction
in total CPU or memory traffic.

## Consequences

Positive:

- smallest path to validate transparency, animation, input, and timer behavior;
- no third-party GUI framework;
- pure animation and Pet logic remain independent of Win32 rendering details;
- future renderer replacement has a defined boundary.

Negative:

- continuous complex effects may eventually become CPU-bound;
- the MVP does not exercise a GPU composition path;
- the renderer must correctly maintain premultiplied alpha and reusable buffers.

## Revisit condition

Measure frame update time, layered-window submit time, memory usage, and missed
frame intervals after the MVP is functional. Revisit Direct2D or
DirectComposition only if those measurements show a user-visible problem.
