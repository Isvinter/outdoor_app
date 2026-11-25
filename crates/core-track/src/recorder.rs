use crate::{
    Activity, Segment, StatsParams, Track, TrackError, TrackPoint, TrackStats, compute_stats,
};

/// State of a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderState {
    Recording,
    Paused,
    Stopped,
}

impl RecorderState {
    fn name(self) -> &'static str {
        match self {
            RecorderState::Recording => "recording",
            RecorderState::Paused => "paused",
            RecorderState::Stopped => "stopped",
        }
    }
}

/// State machine of a track recording, holding the track recorded so far.
///
/// It decides which fixes belong to the track and into which segment they go.
/// Persistence is the caller's job: store every point for which
/// [`Recorder::add_point`] returns `true` under [`Recorder::current_segment`].
#[derive(Debug)]
pub struct Recorder {
    state: RecorderState,
    track: Track,
    /// A pause or restart happened since the last point: the next one opens a segment.
    segment_break: bool,
    last_time_ms: Option<i64>,
}

impl Recorder {
    /// Begins a new recording.
    pub fn start(activity: Activity) -> Self {
        Self {
            state: RecorderState::Recording,
            track: Track {
                name: None,
                activity: Some(activity),
                segments: Vec::new(),
            },
            segment_break: false,
            last_time_ms: None,
        }
    }

    /// Continues a recording that was interrupted, e.g. by a crash. New points go
    /// into a new segment.
    pub fn continue_from(mut track: Track, activity: Activity) -> Self {
        track.activity = Some(activity);
        track.segments.retain(|s| !s.points.is_empty());
        let last_time_ms = track
            .segments
            .iter()
            .flat_map(|s| &s.points)
            .filter_map(|p| p.time_ms)
            .max();
        Self {
            state: RecorderState::Recording,
            track,
            segment_break: true,
            last_time_ms,
        }
    }

    pub fn state(&self) -> RecorderState {
        self.state
    }

    /// The track recorded so far.
    pub fn track(&self) -> &Track {
        &self.track
    }

    /// The next point starts a new segment, as after a resume; for a long
    /// gap without fixes.
    pub fn break_segment(&mut self) {
        self.segment_break = !self.track.segments.is_empty();
    }

    /// Index of the segment the most recently added point belongs to.
    pub fn current_segment(&self) -> usize {
        self.track.segments.len().saturating_sub(1)
    }

    /// Offers a fix to the recording. Returns whether it became part of the track.
    ///
    /// Rejected are fixes while not recording, fixes without a valid position or
    /// time, and fixes that are not newer than the previous one. Plausibility
    /// (GPS jumps) is judged later by the statistics, on the unaltered data.
    pub fn add_point(&mut self, point: TrackPoint) -> bool {
        if self.state != RecorderState::Recording {
            return false;
        }
        let valid_position = point.lat.is_finite()
            && point.lon.is_finite()
            && point.lat.abs() <= 90.0
            && point.lon.abs() <= 180.0;
        let Some(time_ms) = point.time_ms.filter(|_| valid_position) else {
            return false;
        };
        if self.last_time_ms.is_some_and(|last| time_ms <= last) {
            return false;
        }
        if self.segment_break || self.track.segments.is_empty() {
            self.track.segments.push(Segment::default());
            self.segment_break = false;
        }
        if let Some(segment) = self.track.segments.last_mut() {
            segment.points.push(point);
        }
        self.last_time_ms = Some(time_ms);
        true
    }

    pub fn pause(&mut self) -> Result<(), TrackError> {
        self.transition("pause", RecorderState::Recording, RecorderState::Paused)
    }

    pub fn resume(&mut self) -> Result<(), TrackError> {
        self.transition("resume", RecorderState::Paused, RecorderState::Recording)?;
        // Only break the segment if the current one has points.
        self.segment_break = !self.track.segments.is_empty();
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), TrackError> {
        if self.state == RecorderState::Stopped {
            return Err(self.invalid("stop"));
        }
        self.state = RecorderState::Stopped;
        Ok(())
    }

    /// Statistics of the track recorded so far.
    pub fn stats(&self, params: &StatsParams) -> TrackStats {
        compute_stats(&self.track, params)
    }

    fn transition(
        &mut self,
        action: &'static str,
        from: RecorderState,
        to: RecorderState,
    ) -> Result<(), TrackError> {
        if self.state != from {
            return Err(self.invalid(action));
        }
        self.state = to;
        Ok(())
    }

    fn invalid(&self, action: &'static str) -> TrackError {
        TrackError::InvalidTransition {
            state: self.state.name(),
            action,
        }
    }
}
