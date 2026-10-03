use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    Loop,
    Once,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimationClip {
    frames: Vec<Frame>,
    frame_duration: Duration,
    loop_mode: LoopMode,
}

impl AnimationClip {
    pub fn new(frames: Vec<Frame>, frame_duration: Duration, loop_mode: LoopMode) -> Result<Self, AnimationError> {
        if frames.is_empty() {
            return Err(AnimationError::EmptyFrames);
        }
        if frame_duration.is_zero() {
            return Err(AnimationError::ZeroFrameDuration);
        }

        Ok(Self {
            frames,
            frame_duration,
            loop_mode,
        })
    }

    fn frame_at(&self, index: usize) -> Frame {
        self.frames[index]
    }

    fn frame_count(&self) -> usize {
        self.frames.len()
    }

    fn frame_duration(&self) -> Duration {
        self.frame_duration
    }

    fn loop_mode(&self) -> LoopMode {
        self.loop_mode
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameSelection {
    pub frame: Frame,
    pub state: PlaybackState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationError {
    EmptyFrames,
    ZeroFrameDuration,
}

pub struct AnimationPlayer {
    clip: AnimationClip,
    state: PlaybackState,
    started_at: Option<Instant>,
    elapsed_before_pause: Duration,
    frame_index: usize,
}

impl AnimationPlayer {
    pub fn new(clip: AnimationClip) -> Self {
        Self {
            clip,
            state: PlaybackState::Stopped,
            started_at: None,
            elapsed_before_pause: Duration::ZERO,
            frame_index: 0,
        }
    }

    #[allow(unused)]
    pub fn state(&self) -> PlaybackState {
        self.state
    }

    pub fn play(&mut self, now: Instant) {
        self.state = PlaybackState::Playing;
        self.started_at = Some(now);
        self.elapsed_before_pause = Duration::ZERO;
        self.frame_index = 0;
    }

    #[allow(unused)]
    pub fn pause(&mut self, now: Instant) {
        if self.state != PlaybackState::Playing {
            return;
        }

        self.elapsed_before_pause = self.elapsed_at(now);
        self.state = PlaybackState::Paused;
        self.started_at = None;
        self.update_frame_index();
    }

    #[allow(unused)]
    pub fn resume(&mut self, now: Instant) {
        if self.state != PlaybackState::Paused {
            return;
        }

        self.started_at = Some(now - self.elapsed_before_pause);
        self.state = PlaybackState::Playing;
    }

    #[allow(unused)]
    pub fn reset(&mut self) {
        self.state = PlaybackState::Stopped;
        self.started_at = None;
        self.elapsed_before_pause = Duration::ZERO;
        self.frame_index = 0;
    }

    pub fn update(&mut self, now: Instant) -> FrameSelection {
        if self.state == PlaybackState::Playing {
            self.elapsed_before_pause = self.elapsed_at(now);
            self.update_frame_index();
        }

        FrameSelection {
            frame: self.clip.frame_at(self.frame_index),
            state: self.state,
        }
    }

    fn elapsed_at(&self, now: Instant) -> Duration {
        self.started_at
            .map(|started_at| now.saturating_duration_since(started_at))
            .unwrap_or(self.elapsed_before_pause)
    }

    fn update_frame_index(&mut self) {
        let elapsed = self.elapsed_before_pause;
        let frame_duration = self.clip.frame_duration();
        let frame_count = self.clip.frame_count();
        let frame_number = elapsed.as_nanos() / frame_duration.as_nanos();

        match self.clip.loop_mode() {
            LoopMode::Loop => {
                self.frame_index = (frame_number as usize) % frame_count;
            }
            LoopMode::Once => {
                let last_frame = frame_count - 1;
                self.frame_index = (frame_number as usize).min(last_frame);
                if frame_number >= frame_count as u128 {
                    self.state = PlaybackState::Finished;
                    self.started_at = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AnimationClip, AnimationError, AnimationPlayer, Frame, LoopMode, PlaybackState};
    use std::time::{Duration, Instant};

    fn clip(frame_count: u32, frame_duration_ms: u64, loop_mode: LoopMode) -> AnimationClip {
        AnimationClip::new(
            (0..frame_count).map(|id| Frame { id }).collect(),
            Duration::from_millis(frame_duration_ms),
            loop_mode,
        )
        .expect("test clip should be valid")
    }

    #[test]
    fn rejects_empty_frames() {
        assert_eq!(
            AnimationClip::new(Vec::new(), Duration::from_millis(100), LoopMode::Loop),
            Err(AnimationError::EmptyFrames)
        );
    }

    #[test]
    fn rejects_zero_frame_duration() {
        assert_eq!(
            AnimationClip::new(vec![Frame { id: 0 }], Duration::ZERO, LoopMode::Loop),
            Err(AnimationError::ZeroFrameDuration)
        );
    }

    #[test]
    fn selects_frames_at_boundaries() {
        let start = Instant::now();
        let mut player = AnimationPlayer::new(clip(3, 100, LoopMode::Loop));
        player.play(start);

        assert_eq!(player.update(start).frame.id, 0);
        assert_eq!(player.update(start + Duration::from_millis(99)).frame.id, 0);
        assert_eq!(player.update(start + Duration::from_millis(100)).frame.id, 1);
        assert_eq!(player.update(start + Duration::from_millis(299)).frame.id, 2);
    }

    #[test]
    fn loops_back_to_the_first_frame() {
        let start = Instant::now();
        let mut player = AnimationPlayer::new(clip(3, 100, LoopMode::Loop));
        player.play(start);

        assert_eq!(player.update(start + Duration::from_millis(300)).frame.id, 0);
        assert_eq!(player.state(), PlaybackState::Playing);
    }

    #[test]
    fn one_shot_finishes_on_the_last_frame() {
        let start = Instant::now();
        let mut player = AnimationPlayer::new(clip(3, 100, LoopMode::Once));
        player.play(start);

        let selection = player.update(start + Duration::from_millis(300));
        assert_eq!(selection.frame.id, 2);
        assert_eq!(selection.state, PlaybackState::Finished);

        let later_selection = player.update(start + Duration::from_secs(1));
        assert_eq!(later_selection.frame.id, 2);
        assert_eq!(later_selection.state, PlaybackState::Finished);
    }

    #[test]
    fn pause_and_resume_preserve_elapsed_time() {
        let start = Instant::now();
        let mut player = AnimationPlayer::new(clip(3, 100, LoopMode::Loop));
        player.play(start);
        player.pause(start + Duration::from_millis(150));

        assert_eq!(player.state(), PlaybackState::Paused);
        assert_eq!(player.update(start + Duration::from_secs(1)).frame.id, 1);

        player.resume(start + Duration::from_secs(1));
        assert_eq!(
            player
                .update(start + Duration::from_secs(1) + Duration::from_millis(49))
                .frame
                .id,
            1
        );
        assert_eq!(
            player
                .update(start + Duration::from_secs(1) + Duration::from_millis(50))
                .frame
                .id,
            2
        );
    }

    #[test]
    fn reset_returns_to_the_initial_stopped_frame() {
        let start = Instant::now();
        let mut player = AnimationPlayer::new(clip(3, 100, LoopMode::Loop));
        player.play(start);
        player.update(start + Duration::from_millis(200));
        player.reset();

        let selection = player.update(start + Duration::from_secs(1));
        assert_eq!(selection.frame.id, 0);
        assert_eq!(selection.state, PlaybackState::Stopped);
    }
}
