use crate::dsp::{Delay, Filter, Smooth, MAX_FILTER_LATENCY, RATE};
use biquad::{Biquad, Coefficients, DirectForm1, ToHertz, Type, Q_BUTTERWORTH_F32};
use signalsmith_stretch::Stretch;
use std::collections::BTreeMap;

type Params = BTreeMap<String, f32>;
const RMS_SAMPLES: usize = 240;
const VOICE_WINDOW: usize = 2048;
const VOICE_HOP: usize = 64;

fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}
fn time_coefficient(ms: f32) -> f32 {
    (-1000.0 / (RATE * ms)).exp()
}

struct Rms {
    history: [f32; RMS_SAMPLES],
    cursor: usize,
    power: f64,
}
impl Rms {
    fn new() -> Self {
        Self {
            history: [0.0; RMS_SAMPLES],
            cursor: 0,
            power: 0.0,
        }
    }
    fn next(&mut self, input: f32) -> f32 {
        let squared = input * input;
        self.power += squared as f64 - self.history[self.cursor] as f64;
        self.history[self.cursor] = squared;
        self.cursor = (self.cursor + 1) % RMS_SAMPLES;
        (self.power.max(0.0) / RMS_SAMPLES as f64).sqrt() as f32
    }
}

pub(crate) struct Gate {
    detector: Rms,
    threshold: Smooth,
    attack: Smooth,
    hold: Smooth,
    release: Smooth,
    open: bool,
    holding: usize,
    gain: f32,
}
impl Gate {
    pub(crate) fn new(p: &Params) -> Self {
        Self {
            detector: Rms::new(),
            threshold: Smooth::new(db_to_gain(p["threshold_db"])),
            attack: Smooth::new(time_coefficient(p["attack_ms"])),
            hold: Smooth::new(p["hold_ms"] * RATE / 1000.0),
            release: Smooth::new(time_coefficient(p["release_ms"])),
            open: false,
            holding: 0,
            gain: 0.0,
        }
    }
}
impl Filter for Gate {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (x, y) in input.iter().zip(output) {
            let level = self.detector.next(*x);
            let threshold = self.threshold.next();
            let hold = self.hold.next() as usize;
            if level >= threshold {
                self.open = true;
                self.holding = hold;
            } else if self.open {
                if level >= threshold * 0.7079458 {
                    self.holding = hold;
                } else if self.holding > 0 {
                    self.holding -= 1;
                } else {
                    self.open = false;
                }
            }
            let attack = self.attack.next();
            let release = self.release.next();
            let target = if self.open { 1.0 } else { 0.0 };
            let coefficient = if self.open { attack } else { release };
            self.gain = target + (self.gain - target) * coefficient;
            *y = *x * self.gain;
        }
    }
    fn set_params(&mut self, p: &Params) {
        self.threshold.set(db_to_gain(p["threshold_db"]));
        self.attack.set(time_coefficient(p["attack_ms"]));
        self.hold.set(p["hold_ms"] * RATE / 1000.0);
        self.release.set(time_coefficient(p["release_ms"]));
    }
}

pub(crate) struct Compressor {
    detector: Rms,
    threshold: Smooth,
    ratio: Smooth,
    attack: Smooth,
    release: Smooth,
    makeup: Smooth,
    gain_db: f32,
}
impl Compressor {
    pub(crate) fn new(p: &Params) -> Self {
        Self {
            detector: Rms::new(),
            threshold: Smooth::new(p["threshold_db"]),
            ratio: Smooth::new(p["ratio"]),
            attack: Smooth::new(time_coefficient(p["attack_ms"])),
            release: Smooth::new(time_coefficient(p["release_ms"])),
            makeup: Smooth::new(db_to_gain(p["makeup_db"])),
            gain_db: 0.0,
        }
    }
}
impl Filter for Compressor {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (x, y) in input.iter().zip(output) {
            let level_db = 20.0 * self.detector.next(*x).max(1e-12).log10();
            let distance = level_db - self.threshold.next();
            let slope = 1.0 / self.ratio.next() - 1.0;
            let target_db = if distance <= -3.0 {
                0.0
            } else if distance >= 3.0 {
                slope * distance
            } else {
                slope * (distance + 3.0).powi(2) / 12.0
            };
            let attack = self.attack.next();
            let release = self.release.next();
            let coefficient = if target_db < self.gain_db {
                attack
            } else {
                release
            };
            self.gain_db = target_db + (self.gain_db - target_db) * coefficient;
            *y = *x * db_to_gain(self.gain_db) * self.makeup.next();
        }
    }
    fn set_params(&mut self, p: &Params) {
        self.threshold.set(p["threshold_db"]);
        self.ratio.set(p["ratio"]);
        self.attack.set(time_coefficient(p["attack_ms"]));
        self.release.set(time_coefficient(p["release_ms"]));
        self.makeup.set(db_to_gain(p["makeup_db"]));
    }
}

pub(crate) struct Equalizer {
    low: DirectForm1<f32>,
    mid: DirectForm1<f32>,
    high: DirectForm1<f32>,
    low_gain: Smooth,
    low_freq: Smooth,
    mid_gain: Smooth,
    mid_freq: Smooth,
    mid_q: Smooth,
    high_gain: Smooth,
    high_freq: Smooth,
    tick: usize,
    dirty: bool,
}
impl Equalizer {
    fn coefficients(kind: Type<f32>, frequency: f32, q: f32) -> Coefficients<f32> {
        Coefficients::from_params(kind, RATE.hz(), frequency.hz(), q)
            .expect("validated equalizer parameters")
    }
    pub(crate) fn new(p: &Params) -> Self {
        Self {
            low: DirectForm1::new(Self::coefficients(
                Type::LowShelf(p["low_gain_db"]),
                p["low_freq"],
                Q_BUTTERWORTH_F32,
            )),
            mid: DirectForm1::new(Self::coefficients(
                Type::PeakingEQ(p["mid_gain_db"]),
                p["mid_freq"],
                p["mid_q"],
            )),
            high: DirectForm1::new(Self::coefficients(
                Type::HighShelf(p["high_gain_db"]),
                p["high_freq"],
                Q_BUTTERWORTH_F32,
            )),
            low_gain: Smooth::new(p["low_gain_db"]),
            low_freq: Smooth::new(p["low_freq"]),
            mid_gain: Smooth::new(p["mid_gain_db"]),
            mid_freq: Smooth::new(p["mid_freq"]),
            mid_q: Smooth::new(p["mid_q"]),
            high_gain: Smooth::new(p["high_gain_db"]),
            high_freq: Smooth::new(p["high_freq"]),
            tick: 0,
            dirty: false,
        }
    }
}
impl Filter for Equalizer {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (x, y) in input.iter().zip(output) {
            let low_gain = self.low_gain.next();
            let low_freq = self.low_freq.next();
            let mid_gain = self.mid_gain.next();
            let mid_freq = self.mid_freq.next();
            let mid_q = self.mid_q.next();
            let high_gain = self.high_gain.next();
            let high_freq = self.high_freq.next();
            if self.dirty && self.tick % 16 == 0 {
                self.low.update_coefficients(Self::coefficients(
                    Type::LowShelf(low_gain),
                    low_freq,
                    Q_BUTTERWORTH_F32,
                ));
                self.mid.update_coefficients(Self::coefficients(
                    Type::PeakingEQ(mid_gain),
                    mid_freq,
                    mid_q,
                ));
                self.high.update_coefficients(Self::coefficients(
                    Type::HighShelf(high_gain),
                    high_freq,
                    Q_BUTTERWORTH_F32,
                ));
                self.dirty = self.low_gain.active()
                    || self.low_freq.active()
                    || self.mid_gain.active()
                    || self.mid_freq.active()
                    || self.mid_q.active()
                    || self.high_gain.active()
                    || self.high_freq.active();
            }
            self.tick = self.tick.wrapping_add(1);
            *y = self.high.run(self.mid.run(self.low.run(*x)));
        }
    }
    fn set_params(&mut self, p: &Params) {
        self.low_gain.set(p["low_gain_db"]);
        self.low_freq.set(p["low_freq"]);
        self.mid_gain.set(p["mid_gain_db"]);
        self.mid_freq.set(p["mid_freq"]);
        self.mid_q.set(p["mid_q"]);
        self.high_gain.set(p["high_gain_db"]);
        self.high_freq.set(p["high_freq"]);
        self.dirty = true;
    }
}

pub(crate) struct VoiceShift {
    processor: Stretch,
    shift: Smooth,
    formant: bool,
    paired_formant: Option<Smooth>,
    activity: Smooth,
    dry: Delay,
    latency: usize,
}
impl VoiceShift {
    pub(crate) fn new(
        formant: bool,
        semitones: f32,
        paired_formant: Option<f32>,
    ) -> Result<Self, String> {
        let mut processor = Stretch::new(1, VOICE_WINDOW, VOICE_HOP);
        // The wrapper's formant-base setter changes the shift; retain the library's default analysis.
        if formant {
            processor.set_formant_factor_semitones(semitones, false);
        } else {
            processor.set_formant_factor_semitones(paired_formant.unwrap_or(0.0), true);
            processor.set_transpose_factor_semitones(semitones, None);
        }
        let latency = processor.input_latency() + processor.output_latency();
        if latency > MAX_FILTER_LATENCY {
            return Err("变调算法延迟超过补偿容量".into());
        }
        Ok(Self {
            processor,
            shift: Smooth::new(semitones),
            formant,
            paired_formant: paired_formant.map(Smooth::new),
            activity: Smooth::new(
                if semitones != 0.0 || paired_formant.unwrap_or(0.0) != 0.0 {
                    1.0
                } else {
                    0.0
                },
            ),
            dry: Delay::new(latency),
            latency,
        })
    }
}
impl Filter for VoiceShift {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (x, y) in input.chunks(32).zip(output.chunks_mut(32)) {
            if self.shift.active() {
                let mut semitones = 0.0;
                for _ in x {
                    semitones = self.shift.next();
                }
                if self.formant {
                    self.processor
                        .set_formant_factor_semitones(semitones, false);
                } else {
                    self.processor
                        .set_transpose_factor_semitones(semitones, None);
                }
            }
            if let Some(shift) = &mut self.paired_formant {
                if shift.active() {
                    let mut semitones = 0.0;
                    for _ in x {
                        semitones = shift.next();
                    }
                    self.processor.set_formant_factor_semitones(semitones, true);
                }
            }
            self.processor.process(x, &mut *y);
            for (x, y) in x.iter().zip(y) {
                let dry = self.dry.next(*x);
                *y = dry + (*y - dry) * self.activity.next();
            }
        }
    }
    fn set_params(&mut self, p: &Params) {
        self.shift.set(p["semitones"]);
        if let Some(shift) = &mut self.paired_formant {
            shift.set(p["formant_semitones"]);
        }
        self.activity.set(
            if p["semitones"] != 0.0 || p.get("formant_semitones").copied().unwrap_or(0.0) != 0.0 {
                1.0
            } else {
                0.0
            },
        );
    }
    fn latency(&self) -> usize {
        self.latency
    }
}

struct Comb {
    samples: Vec<f32>,
    cursor: usize,
    damped: f32,
}
impl Comb {
    fn new(length: usize) -> Self {
        Self {
            samples: vec![0.0; length],
            cursor: 0,
            damped: 0.0,
        }
    }
    fn next(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let delayed = self.samples[self.cursor];
        self.damped = delayed * (1.0 - damping) + self.damped * damping;
        self.samples[self.cursor] = input + self.damped * feedback;
        self.cursor = (self.cursor + 1) % self.samples.len();
        delayed
    }
}
struct Allpass {
    samples: Vec<f32>,
    cursor: usize,
}
impl Allpass {
    fn new(length: usize) -> Self {
        Self {
            samples: vec![0.0; length],
            cursor: 0,
        }
    }
    fn next(&mut self, input: f32) -> f32 {
        let delayed = self.samples[self.cursor];
        self.samples[self.cursor] = input + delayed * 0.5;
        self.cursor = (self.cursor + 1) % self.samples.len();
        delayed - input * 0.5
    }
}
pub(crate) struct Reverb {
    combs: [Comb; 4],
    allpasses: [Allpass; 2],
    room: Smooth,
    damping: Smooth,
    mix: Smooth,
}
impl Reverb {
    pub(crate) fn new(p: &Params) -> Self {
        Self {
            combs: [1559, 1613, 1699, 1789].map(Comb::new),
            allpasses: [241, 113].map(Allpass::new),
            room: Smooth::new(p["room"] / 100.0),
            damping: Smooth::new(p["damping"] / 100.0),
            mix: Smooth::new(p["mix"] / 100.0),
        }
    }
}
impl Filter for Reverb {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (x, y) in input.iter().zip(output) {
            let feedback = 0.2 + self.room.next() * 0.65;
            let damping = 0.05 + self.damping.next() * 0.9;
            let mut wet = 0.0;
            for comb in &mut self.combs {
                wet += comb.next(*x * 0.5, feedback, damping);
            }
            wet *= 0.25;
            for allpass in &mut self.allpasses {
                wet = allpass.next(wet);
            }
            *y = *x + (wet - *x) * self.mix.next();
        }
    }
    fn set_params(&mut self, p: &Params) {
        self.room.set(p["room"] / 100.0);
        self.damping.set(p["damping"] / 100.0);
        self.mix.set(p["mix"] / 100.0);
    }
}
