#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimerState {
    NotStarted,
    Running,
    Paused,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Countdown {
    period: u32,
    remaining: u32,
    state: TimerState,
}

impl Countdown {
    pub(crate) fn new(period: u32) -> Self {
        Self {
            period,
            remaining: period,
            state: TimerState::NotStarted,
        }
    }

    pub(crate) fn start(&mut self) {
        if self.remaining == 0 {
            self.remaining = self.period;
        }
        self.state = TimerState::Running;
    }

    pub(crate) fn pause(&mut self) {
        self.state = TimerState::Paused;
    }

    pub(crate) fn reset(&mut self) {
        self.remaining = self.period;
        self.state = TimerState::NotStarted;
    }

    pub(crate) fn restart(&mut self) {
        self.reset();
        self.start();
    }

    /// Returns true only when this running tick finishes the countdown.
    pub(crate) fn tick(&mut self) -> bool {
        if self.state != TimerState::Running {
            return false;
        }
        self.remaining = self.remaining.saturating_sub(1);
        if self.remaining == 0 {
            self.state = TimerState::Finished;
            return true;
        }
        false
    }

    pub(crate) fn set_period(&mut self, seconds: u32) -> bool {
        if self.period == seconds {
            return false;
        }
        self.period = seconds;
        self.remaining = seconds;
        true
    }

    pub(crate) fn remaining(&self) -> u32 {
        self.remaining
    }

    pub(crate) fn displayed_remaining(&self) -> u32 {
        if self.state == TimerState::Finished {
            self.period
        } else {
            self.remaining
        }
    }

    pub(crate) fn state(&self) -> TimerState {
        self.state
    }

    pub(crate) fn buttons(&self) -> (bool, bool, bool) {
        (
            self.state != TimerState::Running,
            self.state == TimerState::Running,
            self.state != TimerState::Finished
                && (self.state != TimerState::NotStarted || self.remaining != self.period),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_pause_and_resume_preserve_remaining_seconds() {
        let mut timer = Countdown::new(3);
        assert_eq!(timer.buttons(), (true, false, false));
        assert!(!timer.tick());
        timer.start();
        assert_eq!(timer.buttons(), (false, true, true));
        assert!(!timer.tick());
        assert_eq!(timer.remaining(), 2);
        timer.pause();
        assert_eq!(timer.buttons(), (true, false, true));
        assert!(!timer.tick());
        assert_eq!(timer.remaining(), 2);
        timer.start();
        assert!(!timer.tick());
        assert_eq!(timer.remaining(), 1);
    }

    #[test]
    fn completion_happens_once_and_start_restores_full_interval() {
        let mut timer = Countdown::new(2);
        timer.start();
        assert!(!timer.tick());
        assert!(timer.tick());
        assert_eq!(timer.state(), TimerState::Finished);
        assert_eq!(timer.remaining(), 0);
        assert_eq!(timer.displayed_remaining(), 2);
        assert_eq!(timer.buttons(), (true, false, false));
        assert!(!timer.tick());
        timer.start();
        assert_eq!(timer.remaining(), 2);
        assert_eq!(timer.state(), TimerState::Running);
    }

    #[test]
    fn reset_and_acknowledgement_restore_the_interval() {
        for paused in [false, true] {
            let mut timer = Countdown::new(3);
            timer.start();
            timer.tick();
            if paused {
                timer.pause();
            }
            timer.reset();
            assert_eq!(timer, Countdown::new(3));
            timer.start();
            timer.tick();
            timer.restart();
            assert_eq!(timer.remaining(), 3);
            assert_eq!(timer.state(), TimerState::Running);
        }
    }

    #[test]
    fn changing_period_preserves_state_and_unchanged_period_preserves_progress() {
        for state in [
            TimerState::NotStarted,
            TimerState::Running,
            TimerState::Paused,
            TimerState::Finished,
        ] {
            let mut timer = Countdown::new(1200);
            if state != TimerState::NotStarted {
                timer.start();
                let ticks = if state == TimerState::Finished { 1200 } else { 469 };
                for _ in 0..ticks {
                    timer.tick();
                }
                if state == TimerState::Paused {
                    timer.pause();
                }
            }
            let before = timer;
            assert!(!timer.set_period(1200));
            assert_eq!(timer, before);
            assert!(timer.set_period(600));
            assert_eq!(timer.state(), state);
            assert_eq!(timer.remaining(), 600);
            assert_eq!(timer.displayed_remaining(), 600);
        }
    }

    #[test]
    fn countdown_instances_do_not_share_state() {
        let mut first = Countdown::new(2);
        let mut second = Countdown::new(30);
        first.start();
        second.start();
        assert!(!first.tick());
        assert_eq!(second.remaining(), 30);
        second.pause();
        assert!(first.tick());
        assert_eq!(second.state(), TimerState::Paused);
        first.restart();
        assert_eq!(first.remaining(), 2);
        assert_eq!(first.state(), TimerState::Running);
        assert_eq!(second.remaining(), 30);
    }

    #[test]
    fn finished_display_preserves_seconds_and_zero_duration_does_not_underflow() {
        for period in [0, 1, 30, 600, 1200, 3661] {
            let mut timer = Countdown::new(period);
            timer.start();
            for _ in 0..period.saturating_sub(1) {
                assert!(!timer.tick());
            }
            assert!(timer.tick());
            assert_eq!(timer.displayed_remaining(), period);
            assert_eq!(timer.remaining(), 0);
        }
    }
}
