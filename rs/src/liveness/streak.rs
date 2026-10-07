//! The state machine behind silence detection, apart from CoreAudio so it can be tested without a
//! device (Swift `ZeroStreak.swift`). A receiver whose transmitter is off delivers exact digital
//! zero; a real microphone never does. Runs on the realtime thread: no allocation, no panics.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flip {
    BecameSilent,
    BecameLive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Nothing measured yet: the first buffer either way is reported.
    Unknown,
    Live,
    Silent,
}

#[derive(Debug, Clone)]
pub struct ZeroStreak {
    threshold_frames: usize,
    state: State,
    zero_frames: usize,
}

impl ZeroStreak {
    /// Consecutive all-zero frames needed before a device counts as silent, at least one.
    pub fn new(threshold_frames: usize) -> Self {
        Self { threshold_frames: threshold_frames.max(1), state: State::Unknown, zero_frames: 0 }
    }

    pub fn threshold_frames(&self) -> usize {
        self.threshold_frames
    }

    pub fn zero_frames(&self) -> usize {
        self.zero_frames
    }

    pub fn is_silent(&self) -> bool {
        self.state == State::Silent
    }

    /// Feeds one buffer. Returns a flip only when the verdict changes.
    pub fn feed(&mut self, frames: usize, all_zero: bool) -> Option<Flip> {
        if !all_zero {
            self.zero_frames = 0;
            if self.state == State::Live {
                return None;
            }
            self.state = State::Live;
            return Some(Flip::BecameLive);
        }
        self.zero_frames = self.zero_frames.saturating_add(frames);
        if self.zero_frames < self.threshold_frames || self.state == State::Silent {
            return None;
        }
        self.state = State::Silent;
        Some(Flip::BecameSilent)
    }
}
