//! Trim, fade and volume: the playback range of a clip and its gain over time.

/// How a clip plays (PowerPoint's Playback tab). Times in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipParams {
    /// Trimmed from the start.
    pub trim_start: f64,
    /// Trimmed from the end.
    pub trim_end: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    /// 0..1.
    pub volume: f32,
}

impl Default for ClipParams {
    fn default() -> Self {
        ClipParams { trim_start: 0.0, trim_end: 0.0, fade_in: 0.0, fade_out: 0.0, volume: 1.0 }
    }
}

impl ClipParams {
    /// From the model's millisecond fields.
    pub fn from_ms(trim_start: u32, trim_end: u32, fade_in: u32, fade_out: u32, volume: f64) -> ClipParams {
        ClipParams {
            trim_start: trim_start as f64 / 1000.0,
            trim_end: trim_end as f64 / 1000.0,
            fade_in: fade_in as f64 / 1000.0,
            fade_out: fade_out as f64 / 1000.0,
            volume: volume.clamp(0.0, 1.0) as f32,
        }
    }

    /// The played range `[start, end)` in media seconds for a clip `duration` long. Trims that
    /// would leave nothing are ignored (the whole clip plays); an unknown duration (0) gives an
    /// open end (`f64::INFINITY`).
    pub fn range(&self, duration: f64) -> (f64, f64) {
        if duration.is_nan() || duration <= 0.0 {
            return (self.trim_start.max(0.0), f64::INFINITY);
        }
        let start = self.trim_start.clamp(0.0, duration);
        let end = (duration - self.trim_end.max(0.0)).clamp(0.0, duration);
        if end - start < 0.01 { (0.0, duration) } else { (start, end) }
    }

    /// Gain at media time `t`: volume × fade-in ramp × fade-out ramp (linear). Fades longer than
    /// half the played range are shortened so they never overlap.
    pub fn gain(&self, t: f64, duration: f64) -> f32 {
        let (start, end) = self.range(duration);
        let len = end - start;
        let half = if len.is_finite() { len / 2.0 } else { f64::INFINITY };
        let fi = self.fade_in.max(0.0).min(half);
        let fo = self.fade_out.max(0.0).min(half);
        let mut g = 1.0;
        if fi > 0.0 {
            g *= ((t - start) / fi).clamp(0.0, 1.0);
        }
        if fo > 0.0 && end.is_finite() {
            g *= ((end - t) / fo).clamp(0.0, 1.0);
        }
        self.volume * g as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_applies_trims() {
        let c = ClipParams { trim_start: 1.0, trim_end: 2.0, ..Default::default() };
        assert_eq!(c.range(10.0), (1.0, 8.0));
        // Trims longer than the clip are ignored.
        let c = ClipParams { trim_start: 6.0, trim_end: 6.0, ..Default::default() };
        assert_eq!(c.range(10.0), (0.0, 10.0));
        // Unknown duration: open end.
        assert_eq!(ClipParams::default().range(0.0), (0.0, f64::INFINITY));
    }

    #[test]
    fn gain_fades_and_volume() {
        let c = ClipParams { trim_start: 1.0, fade_in: 2.0, fade_out: 1.0, volume: 0.5, ..Default::default() };
        let d = 10.0;
        assert_eq!(c.gain(1.0, d), 0.0);
        assert!((c.gain(2.0, d) - 0.25).abs() < 1e-6);
        assert!((c.gain(5.0, d) - 0.5).abs() < 1e-6);
        assert!((c.gain(9.5, d) - 0.25).abs() < 1e-6);
        assert_eq!(c.gain(10.0, d), 0.0);
    }

    #[test]
    fn long_fades_never_overlap() {
        let c = ClipParams { fade_in: 5.0, fade_out: 5.0, ..Default::default() };
        // 2 s clip: each fade is capped at 1 s, so the middle reaches full volume.
        assert!((c.gain(1.0, 2.0) - 1.0).abs() < 1e-6);
        assert!((c.gain(0.5, 2.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn from_ms_converts() {
        let c = ClipParams::from_ms(1500, 250, 0, 1000, 2.0);
        assert_eq!((c.trim_start, c.trim_end, c.fade_out, c.volume), (1.5, 0.25, 1.0, 1.0));
    }
}
