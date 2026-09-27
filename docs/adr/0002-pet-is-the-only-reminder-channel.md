# ADR 0002: Make the Pet the only reminder channel

## Status

Accepted

## Decision

When the timer reaches zero, the application shows the Pet and plays its
reminder animation. Toast notifications and tray balloon fallbacks are removed
from the timer-completion path. The system tray remains an application control
surface, not a second reminder channel.

## Context

The product direction is an animated desktop reminder. Keeping both a Pet and
Toast for the same event would produce duplicate notifications and make the
acknowledgement behavior ambiguous. The current Toast module is only used for
timer reminders, so removing it does not affect another feature.

## Consequences

Positive:

- one clear reminder and acknowledgement model;
- less notification-specific initialization and fallback code;
- the Pet remains useful when the main window is hidden in the tray.

Negative:

- if the Pet cannot be created or displayed, there is no notification fallback;
- future accessibility alternatives must be designed explicitly rather than
  appearing as an accidental fallback.

## Revisit condition

Revisit if users need a non-visual reminder channel or if Pet creation failures
are observed in supported Windows environments.
