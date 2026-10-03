use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechMessage {
    pub text: String,
    pub display_duration: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechBubbleConfig {
    pub messages: Vec<SpeechMessage>,
    pub hidden_gap: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Hidden,
    Visible,
}

pub struct SpeechBubblePlayer {
    config: SpeechBubbleConfig,
    phase: Phase,
    message_index: usize,
    phase_started_at: Option<Instant>,
    hidden_duration: Duration,
}

impl SpeechBubblePlayer {
    pub fn new(config: SpeechBubbleConfig) -> Self {
        let hidden_gap = config.hidden_gap;
        Self {
            config,
            phase: Phase::Hidden,
            message_index: 0,
            phase_started_at: None,
            hidden_duration: hidden_gap,
        }
    }

    pub fn start(&mut self, now: Instant, initial_delay: Duration) {
        self.phase = Phase::Hidden;
        self.message_index = 0;
        self.phase_started_at = Some(now);
        self.hidden_duration = initial_delay;
    }

    fn reset_hidden_duration(&mut self) {
        self.hidden_duration = self.config.hidden_gap;
    }

    pub fn update(&mut self, now: Instant) -> Option<&str> {
        if self.config.messages.is_empty() {
            return None;
        }

        if self.phase_started_at.is_none() {
            self.phase_started_at = Some(now);
        }

        loop {
            let elapsed = now.saturating_duration_since(self.phase_started_at.expect("phase start is set"));
            let duration = match self.phase {
                Phase::Hidden => self.hidden_duration.max(Duration::from_millis(1)),
                Phase::Visible => self.config.messages[self.message_index]
                    .display_duration
                    .max(Duration::from_millis(1)),
            };
            if elapsed < duration {
                return (self.phase == Phase::Visible).then(|| self.config.messages[self.message_index].text.as_str());
            }

            self.phase_started_at = Some(self.phase_started_at.expect("phase start is set") + duration);
            match self.phase {
                Phase::Hidden => self.phase = Phase::Visible,
                Phase::Visible => {
                    self.phase = Phase::Hidden;
                    self.message_index = (self.message_index + 1) % self.config.messages.len();
                    self.reset_hidden_duration();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SpeechBubbleConfig, SpeechBubblePlayer, SpeechMessage};
    use std::time::{Duration, Instant};

    fn player() -> SpeechBubblePlayer {
        SpeechBubblePlayer::new(SpeechBubbleConfig {
            messages: vec![
                SpeechMessage {
                    text: "one".to_owned(),
                    display_duration: Duration::from_secs(2),
                },
                SpeechMessage {
                    text: "two".to_owned(),
                    display_duration: Duration::from_secs(3),
                },
            ],
            hidden_gap: Duration::from_secs(1),
        })
    }

    #[test]
    fn cycles_messages_with_hidden_gaps() {
        let start = Instant::now();
        let mut player = player();

        assert_eq!(player.update(start), None);
        assert_eq!(player.update(start + Duration::from_millis(999)), None);
        assert_eq!(player.update(start + Duration::from_secs(1)), Some("one"));
        assert_eq!(player.update(start + Duration::from_secs(2)), Some("one"));
        assert_eq!(player.update(start + Duration::from_secs(3)), None);
        assert_eq!(player.update(start + Duration::from_secs(4)), Some("two"));
        assert_eq!(player.update(start + Duration::from_secs(7)), None);
        assert_eq!(player.update(start + Duration::from_secs(8)), Some("one"));
    }

    #[test]
    fn empty_messages_stay_hidden() {
        let mut player = SpeechBubblePlayer::new(SpeechBubbleConfig {
            messages: Vec::new(),
            hidden_gap: Duration::from_secs(1),
        });

        assert_eq!(player.update(Instant::now()), None);
    }

    #[test]
    fn start_uses_the_initial_delay_then_the_configured_gap() {
        let start = Instant::now();
        let mut player = player();
        player.start(start, Duration::from_millis(500));

        assert_eq!(player.update(start + Duration::from_millis(499)), None);
        assert_eq!(player.update(start + Duration::from_millis(500)), Some("one"));
        assert_eq!(
            player.update(start + Duration::from_secs(2) + Duration::from_millis(500)),
            None
        );
        assert_eq!(
            player.update(start + Duration::from_secs(3) + Duration::from_millis(500)),
            Some("two")
        );
    }
}
