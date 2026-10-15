use crate::audio::{MIN_PEAK, MIN_RMS, SAMPLE_RATE_HZ};

const MIN_FREQ_HZ: f32 = 230.0;
const MAX_FREQ_HZ: f32 = 500.0;
const MAX_LAG: usize = 96;
const YIN_THRESHOLD: f32 = 0.18;
const MIN_CONFIDENCE: f32 = 0.72;
const MAX_TARGET_ERROR_CENTS: f32 = 170.0;

pub const OK_TOLERANCE_CENTS: f32 = 8.0;

pub const STRINGS: [TargetString; 4] = [
    TargetString {
        name: "G4",
        freq_hz: 392.00,
    },
    TargetString {
        name: "C4",
        freq_hz: 261.63,
    },
    TargetString {
        name: "E4",
        freq_hz: 329.63,
    },
    TargetString {
        name: "A4",
        freq_hz: 440.00,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuneState {
    Flat,
    Sharp,
    InTune,
}

#[derive(Clone, Copy, Debug)]
pub struct TargetString {
    pub name: &'static str,
    pub freq_hz: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PitchReading {
    pub freq_hz: f32,
    pub target: TargetString,
    pub cents: f32,
    pub confidence: f32,
    pub state: TuneState,
}

impl PitchReading {
    pub fn same_bucket_as(self, other: Self) -> bool {
        self.target.name == other.target.name
            && self.state == other.state
            && (self.cents - other.cents).abs() <= 18.0
    }
}

pub fn detect_pitch(samples: &[f32], rms: f32, peak: f32) -> Option<PitchReading> {
    if samples.len() < 512 || rms < MIN_RMS || peak < MIN_PEAK {
        return None;
    }

    let min_lag = (SAMPLE_RATE_HZ as f32 / MAX_FREQ_HZ).floor() as usize;
    let max_lag = (SAMPLE_RATE_HZ as f32 / MIN_FREQ_HZ).ceil() as usize;
    let max_lag = max_lag.min(MAX_LAG - 2).min(samples.len() / 2);

    let mut diff = [0.0f32; MAX_LAG];
    for lag in 1..=max_lag {
        let mut sum = 0.0f32;
        for index in 0..(samples.len() - lag) {
            let delta = samples[index] - samples[index + lag];
            sum += delta * delta;
        }
        diff[lag] = sum;
    }

    let mut cmnd = [1.0f32; MAX_LAG];
    let mut running_sum = 0.0f32;
    for lag in 1..=max_lag {
        running_sum += diff[lag];
        if running_sum > 0.0 {
            cmnd[lag] = diff[lag] * lag as f32 / running_sum;
        }
    }

    let mut best_lag = None;
    for lag in min_lag..=max_lag {
        if cmnd[lag] < YIN_THRESHOLD {
            let mut local = lag;
            while local + 1 <= max_lag && cmnd[local + 1] < cmnd[local] {
                local += 1;
            }
            best_lag = Some(local);
            break;
        }
    }

    let lag = best_lag.or_else(|| {
        (min_lag..=max_lag).min_by(|a, b| {
            cmnd[*a]
                .partial_cmp(&cmnd[*b])
                .unwrap_or(core::cmp::Ordering::Equal)
        })
    })?;

    let confidence = 1.0 - cmnd[lag].clamp(0.0, 1.0);
    if confidence < MIN_CONFIDENCE {
        return None;
    }

    let lag = parabolic_minimum(&cmnd, lag);
    if lag <= 0.0 {
        return None;
    }

    let freq_hz = SAMPLE_RATE_HZ as f32 / lag;
    if !(MIN_FREQ_HZ..=MAX_FREQ_HZ).contains(&freq_hz) {
        return None;
    }

    let target = closest_target(freq_hz);
    let cents = cents_between(freq_hz, target.freq_hz);
    if cents.abs() > MAX_TARGET_ERROR_CENTS {
        return None;
    }

    let state = if cents.abs() <= OK_TOLERANCE_CENTS {
        TuneState::InTune
    } else if cents < 0.0 {
        TuneState::Flat
    } else {
        TuneState::Sharp
    };

    Some(PitchReading {
        freq_hz,
        target,
        cents,
        confidence,
        state,
    })
}

fn closest_target(freq_hz: f32) -> TargetString {
    let mut best = STRINGS[0];
    let mut best_cents = cents_between(freq_hz, best.freq_hz).abs();

    for target in STRINGS.iter().skip(1) {
        let cents = cents_between(freq_hz, target.freq_hz).abs();
        if cents < best_cents {
            best = *target;
            best_cents = cents;
        }
    }

    best
}

fn cents_between(freq_hz: f32, target_hz: f32) -> f32 {
    1200.0 * (freq_hz / target_hz).log2()
}

fn parabolic_minimum(values: &[f32], index: usize) -> f32 {
    if index == 0 || index + 1 >= values.len() {
        return index as f32;
    }

    let left = values[index - 1];
    let center = values[index];
    let right = values[index + 1];
    let denominator = left - 2.0 * center + right;

    if denominator.abs() < 0.000_001 {
        return index as f32;
    }

    let offset = 0.5 * (left - right) / denominator;
    index as f32 + offset.clamp(-0.5, 0.5)
}
