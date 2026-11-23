//! Bounded, dependency-free decoding and rest calibration of 3Dconnexion HID input reports.

use std::time::{Duration, Instant};

const FULL_SCALE: f64 = 500.0;
const DEADZONE: f64 = 0.04;
const REST_LIMIT: f64 = 60.0;
const REST_RANGE: f64 = 10.0;
const CALIBRATION_TIME: Duration = Duration::from_millis(400);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Report {
    /// Mask 1 = translation, 2 = rotation, 3 = a combined six-axis report.
    Axes {
        values: [i16; 6],
        mask: u8,
    },
    Buttons(u8),
}

/// hidapi includes the report ID. A legacy motion report has six payload bytes; combined
/// report 1 has twelve. Windows may pad the short button report to the input-report size.
pub fn decode(bytes: &[u8]) -> Option<Report> {
    let (&id, payload) = bytes.split_first()?;
    if id == 3 {
        return payload.first().copied().map(Report::Buttons);
    }
    let (mask, offset, count) = match id {
        1 if payload.len() >= 12 => (3, 0, 6),
        1 if payload.len() >= 6 => (1, 0, 3),
        2 if payload.len() >= 6 => (2, 3, 3),
        _ => return None,
    };
    let mut values = [0; 6];
    for (out, pair) in values.iter_mut().skip(offset).take(count).zip(payload.as_chunks::<2>().0.iter()) {
        *out = i16::from_le_bytes([*pair.first()?, *pair.get(1)?]);
    }
    Some(Report::Axes { values, mask })
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    pub pan: [f64; 2],
    pub zoom: f64,
}

impl Motion {
    pub fn active(self) -> bool {
        self.pan.iter().any(|v| *v != 0.0) || self.zoom != 0.0
    }
}

#[derive(Default)]
struct Rest {
    since: Option<Instant>,
    count: u32,
    sum: [f64; 6],
    min: [f64; 6],
    max: [f64; 6],
}

impl Rest {
    fn observe(&mut self, raw: [f64; 6], now: Instant) -> Option<[f64; 6]> {
        if raw.iter().any(|v| v.abs() > REST_LIMIT) {
            *self = Self::default();
            return None;
        }
        if self.since.is_none() || raw.iter().zip(self.min.iter().zip(&self.max)).any(|(v, (min, max))| v.max(*max) - v.min(*min) > REST_RANGE) {
            *self = Self { since: Some(now), count: 1, sum: raw, min: raw, max: raw };
            return None;
        }
        for ((min, max), value) in self.min.iter_mut().zip(&mut self.max).zip(raw) {
            *min = min.min(value);
            *max = max.max(value);
        }
        // A fixed bound even if a device sends far more than the expected 125 reports/sec.
        if self.count < 128 {
            self.count += 1;
            for (sum, value) in self.sum.iter_mut().zip(raw) {
                *sum += value;
            }
        }
        if self.count >= 16 && self.since.is_some_and(|since| now.saturating_duration_since(since) >= CALIBRATION_TIME) {
            Some(self.sum.map(|sum| sum / f64::from(self.count)))
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct Filter {
    raw: [f64; 6],
    bias: Option<[f64; 6]>,
    rest: Rest,
    parts: u8,
    seen: [Option<Instant>; 2],
    idle_since: Option<Instant>,
}

impl Filter {
    pub fn calibrated(&self) -> bool {
        self.bias.is_some()
    }

    pub fn recalibrate(&mut self) {
        *self = Self::default();
    }

    pub fn update(&mut self, values: [i16; 6], mask: u8, now: Instant) -> Motion {
        for (index, seen) in self.seen.iter_mut().enumerate() {
            if mask & (1 << index) != 0 {
                *seen = Some(now);
            }
        }
        for (index, (raw, value)) in self.raw.iter_mut().zip(values).enumerate() {
            if mask & if index < 3 { 1 } else { 2 } != 0 {
                *raw = f64::from(value);
            }
        }
        self.parts |= mask & 3;
        let complete = self.parts == 3;
        if complete {
            self.parts = 0;
        }
        if self.bias.is_none() && complete {
            self.bias = self.rest.observe(self.raw, now);
        }
        let Some(mut bias) = self.bias else { return Motion::default() };
        // Follow small resting drift only after two seconds entirely inside the deadzone.
        if complete {
            if self.raw.iter().zip(bias).all(|(raw, bias)| (*raw - bias).abs() <= FULL_SCALE * DEADZONE) {
                let since = *self.idle_since.get_or_insert(now);
                if now.saturating_duration_since(since) >= Duration::from_secs(2) {
                    for (bias, raw) in bias.iter_mut().zip(self.raw) {
                        *bias += (raw - *bias) * 0.01;
                    }
                    self.bias = Some(bias);
                }
            } else {
                self.idle_since = None;
            }
        }
        let mut axes = [0.0; 6];
        for ((axis, raw), bias) in axes.iter_mut().zip(self.raw).zip(bias) {
            let value = ((raw - bias) / FULL_SCALE).clamp(-1.0, 1.0);
            *axis = if value.abs() <= DEADZONE { 0.0 } else { value.signum() * (value.abs() - DEADZONE) / (1.0 - DEADZONE) };
        }
        // A missing rotation report must not keep zooming while translations still arrive.
        for (group, seen) in axes.as_chunks_mut::<3>().0.iter_mut().zip(self.seen) {
            if seen.is_none_or(|at| now.saturating_duration_since(at) >= Duration::from_millis(250)) {
                group.fill(0.0);
            }
        }
        let [tx, ty, tz, _rx, _ry, rz] = axes;
        Motion { pan: [tx, ty], zoom: (tz + rz).clamp(-1.0, 1.0) }
    }
}
