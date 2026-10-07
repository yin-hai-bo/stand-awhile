# ADR 0003: Accept occasional countdown delays

## Status

Accepted

## Decision

The countdown is a movement reminder, not a precision timer. Occasional
delays of a few seconds to ten or more seconds are acceptable by design.

Keep the current one-second Win32 timer and decrement the remaining seconds
once per `WM_TIMER` message. Elapsed-time compensation or deadline-based
counting is not required. The countdown display still shows the exact
remaining seconds stored by the application, including configured intervals
shorter than one minute.

## Context

Window-message processing can delay timer updates. For this application's
purpose, occasional delays do not justify adding more precise timing logic.
Such delays alone are not a bug or a release blocker.

## Consequences

- Countdown behavior remains simple and consistent with the current design.
- A reminder may appear later than the configured interval in real time.
- Tests should verify countdown transitions and remaining-second display
  without requiring exact wall-clock timing.

## Revisit condition

Revisit if the product requires precise timing or observed delays interfere
with its movement-reminder purpose.
