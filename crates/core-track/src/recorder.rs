use crate::{
    Activity, RestDetector, RestEvent, RestParams, Segment, SegmentReason, StatsParams, Track,
    TrackError, TrackPoint, TrackStats, compute_stats,
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
/// It decides which fixes belong to the track and into which segment (section) they
/// go, and gives every segment its activity and the reason it began. Persistence is
/// the caller's job: store every point for which [`Recorder::add_point`] returns
/// `true` under [`Recorder::current_segment`].
///
/// With rest detection on (see [`Recorder::set_rest_detection`], off by default) a
/// detected rest begins a [`SegmentReason::Rest`] segment with the fix it was
/// detected at, and moving on begins a [`SegmentReason::RestEnd`] segment. Stored
/// points are never moved: the fixes standing before the detection stay in the
/// previous segment.
#[derive(Debug)]
pub struct Recorder {
    state: RecorderState,
    track: Track,
    /// Activity of the running section, also of a rest and the sections after it.
    activity: Activity,
    /// The next point opens a segment for this reason.
    pending: Option<SegmentReason>,
    rest: Option<RestDetector>,
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
            activity,
            pending: None,
            rest: None,
            last_time_ms: None,
        }
    }

    /// Continues a recording that was interrupted, e.g. by a crash. New points go
    /// into a new segment with the reason [`SegmentReason::Continue`].
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
            activity,
            pending: Some(SegmentReason::Continue),
            rest: None,
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

    /// Activity of the running section.
    pub fn activity(&self) -> Activity {
        self.activity
    }

    /// Whether a rest has been detected and not yet ended.
    pub fn resting(&self) -> bool {
        self.rest.as_ref().is_some_and(RestDetector::resting)
    }

    /// Turns rest detection on with the given parameters, or off with `None`. Off by
    /// default.
    pub fn set_rest_detection(&mut self, params: Option<RestParams>) {
        self.rest = params.map(RestDetector::new);
    }

    /// Switches the activity: the next point begins a new segment with it. Before the
    /// first point it becomes the activity of the first segment and of the track.
    /// Ends a rest. Switching to the running activity outside a rest changes nothing.
    pub fn switch_activity(&mut self, activity: Activity) {
        if activity == self.activity && !self.resting() {
            return;
        }
        self.activity = activity;
        if self.track.segments.is_empty() {
            self.track.activity = Some(activity);
        }
        self.break_segment(SegmentReason::ActivityChange);
    }

    /// The next point starts a new segment for `reason`, e.g. [`SegmentReason::Gap`]
    /// for a long time without fixes. Ends a rest. Without points so far the next one
    /// starts the first segment as usual.
    pub fn break_segment(&mut self, reason: SegmentReason) {
        if let Some(rest) = &mut self.rest {
            rest.reset();
        }
        if !self.track.segments.is_empty() {
            self.pending = Some(reason);
        }
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
        let event = self.rest.as_mut().and_then(|r| r.offer(&point));
        let reason = match event {
            Some(RestEvent::Started) => Some(SegmentReason::Rest),
            Some(RestEvent::Ended) => Some(SegmentReason::RestEnd),
            None if self.track.segments.is_empty() => Some(SegmentReason::Start),
            None => self.pending,
        };
        if let Some(reason) = reason {
            self.track.segments.push(Segment {
                points: Vec::new(),
                activity: Some(self.activity),
                reason,
            });
            self.pending = None;
        }
        if let Some(segment) = self.track.segments.last_mut() {
            segment.points.push(point);
        }
        self.last_time_ms = Some(time_ms);
        true
    }

    /// Pauses the recording. Ends a rest.
    pub fn pause(&mut self) -> Result<(), TrackError> {
        self.transition("pause", RecorderState::Recording, RecorderState::Paused)?;
        if let Some(rest) = &mut self.rest {
            rest.reset();
        }
        Ok(())
    }

    /// Resumes a paused recording; the next point begins a new segment.
    pub fn resume(&mut self) -> Result<(), TrackError> {
        self.transition("resume", RecorderState::Paused, RecorderState::Recording)?;
        self.break_segment(SegmentReason::Resume);
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
