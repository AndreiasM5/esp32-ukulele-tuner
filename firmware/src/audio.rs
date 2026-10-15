pub const SAMPLE_RATE_HZ: u32 = 16_000;
pub const SAMPLE_COUNT: usize = 2048;

pub const MIN_RMS: f32 = 0.006;
pub const QUIET_RMS: f32 = 0.0018;
pub const MIN_PEAK: f32 = 0.018;

pub struct AudioLevel {
    pub rms: f32,
    pub peak: f32,
}

pub fn decode_inmp441_samples(bytes: &[u8], out: &mut [f32]) -> usize {
    let mut count = 0usize;
    let mut sum = 0.0f32;

    for (dst, chunk) in out.iter_mut().zip(bytes.chunks_exact(4)) {
        let raw_24_bit = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) >> 8;
        let sample = raw_24_bit as f32 / 8_388_608.0;
        *dst = sample;
        sum += sample;
        count += 1;
    }

    if count == 0 {
        out.fill(0.0);
        return 0;
    }

    let mean = sum / count as f32;
    for sample in out.iter_mut().take(count) {
        *sample -= mean;
    }
    for sample in out.iter_mut().skip(count) {
        *sample = 0.0;
    }

    count
}

pub fn level(samples: &[f32]) -> AudioLevel {
    if samples.is_empty() {
        return AudioLevel {
            rms: 0.0,
            peak: 0.0,
        };
    }

    let mut energy = 0.0f32;
    let mut peak = 0.0f32;

    for sample in samples {
        energy += sample * sample;
        peak = peak.max(sample.abs());
    }

    AudioLevel {
        rms: (energy / samples.len() as f32).sqrt(),
        peak,
    }
}
