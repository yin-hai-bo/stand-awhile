use std::time::{Duration, Instant};

const DISPLAY_DURATION: Duration = Duration::from_secs(5);
const HIDDEN_GAP: Duration = Duration::from_secs(10);

pub struct SpeechBubblePlayer {
    visible: bool,
    phase_started_at: Option<Instant>,
    hidden_duration: Duration,
}

impl SpeechBubblePlayer {
    pub fn new() -> Self {
        Self {
            visible: false,
            phase_started_at: None,
            hidden_duration: HIDDEN_GAP,
        }
    }

    pub fn start(&mut self, now: Instant, initial_delay: Duration) {
        self.visible = false;
        self.phase_started_at = Some(now);
        self.hidden_duration = initial_delay;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn update(&mut self, now: Instant) -> bool {
        let mut started_at = *self.phase_started_at.get_or_insert(now);
        loop {
            let duration = if self.visible {
                DISPLAY_DURATION
            } else {
                self.hidden_duration
            };
            if now.saturating_duration_since(started_at) < duration {
                return self.visible;
            }
            started_at += duration;
            self.phase_started_at = Some(started_at);
            self.visible = !self.visible;
            self.hidden_duration = HIDDEN_GAP;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_with_fixed_display_duration_and_hidden_gap() {
        let now = Instant::now();
        let mut player = SpeechBubblePlayer::new();
        assert!(!player.update(now));
        assert!(!player.update(now + Duration::from_millis(9999)));
        assert!(player.update(now + Duration::from_secs(10)));
        assert!(player.update(now + Duration::from_millis(14999)));
        assert!(!player.update(now + Duration::from_secs(15)));
        assert!(!player.update(now + Duration::from_millis(24999)));
        assert!(player.update(now + Duration::from_secs(25)));
    }

    #[test]
    fn start_uses_initial_delay_and_restarts_the_cycle() {
        let now = Instant::now();
        let mut player = SpeechBubblePlayer::new();
        player.start(now, Duration::from_millis(500));
        assert!(!player.update(now + Duration::from_millis(499)));
        assert!(player.update(now + Duration::from_millis(500)));
        assert!(!player.update(now + Duration::from_millis(5500)));
        assert!(player.update(now + Duration::from_millis(15500)));
        player.start(now + Duration::from_secs(20), Duration::ZERO);
        assert!(!player.is_visible());
        assert!(player.update(now + Duration::from_secs(20)));
    }

    #[test]
    fn catches_up_after_a_delayed_update() {
        let now = Instant::now();
        let mut player = SpeechBubblePlayer::new();
        player.update(now);
        assert!(player.update(now + Duration::from_secs(41)));
        assert!(!player.update(now + Duration::from_secs(46)));
    }
}
