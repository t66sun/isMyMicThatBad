use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
pub const BLOCK: usize = 256;
pub const RATE: f32 = 48_000.0;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FilterConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub enabled: bool,
    pub params: BTreeMap<String, f32>,
}
#[derive(Deserialize)]
struct Definition {
    #[serde(rename = "type")]
    kind: String,
    parameters: Vec<Parameter>,
}
#[derive(Deserialize)]
struct Parameter {
    key: String,
    min: f32,
    max: f32,
}
pub fn validate(configs: &[FilterConfig]) -> Result<(), String> {
    // shortcut: 64 instances bound configuration memory, raise after profiling long chains.
    if configs.len() > 64 {
        return Err("最多支持 64 个滤镜".into());
    }
    let definitions: Vec<Definition> =
        serde_json::from_str(include_str!("../../src/filters.json")).map_err(|e| e.to_string())?;
    let mut ids = HashSet::new();
    for config in configs {
        if config.id.is_empty() || !ids.insert(&config.id) {
            return Err("滤镜实例 ID 无效或重复".into());
        }
        let definition = definitions
            .iter()
            .find(|d| d.kind == config.kind)
            .ok_or("未知滤镜")?;
        if config.params.len() != definition.parameters.len() {
            return Err("滤镜参数不完整".into());
        }
        for parameter in &definition.parameters {
            let value = *config.params.get(&parameter.key).ok_or("缺少滤镜参数")?;
            if !value.is_finite() || value < parameter.min || value > parameter.max {
                return Err("滤镜参数超出范围".into());
            }
        }
    }
    Ok(())
}
pub trait Filter: Send {
    fn process(&mut self, input: &[f32], output: &mut [f32]);
    fn set_params(&mut self, params: &BTreeMap<String, f32>);
    fn latency(&self) -> usize {
        0
    }
}
pub(crate) struct Smooth {
    value: f32,
    target: f32,
    step: f32,
    remaining: usize,
}
impl Smooth {
    pub(crate) fn new(value: f32) -> Self {
        Self {
            value,
            target: value,
            step: 0.0,
            remaining: 0,
        }
    }
    pub(crate) fn set(&mut self, target: f32) {
        if target == self.target {
            return;
        }
        self.target = target;
        self.remaining = 480;
        self.step = (target - self.value) / 480.0;
    }
    pub(crate) fn next(&mut self) -> f32 {
        if self.remaining > 0 {
            self.value += self.step;
            self.remaining -= 1;
            if self.remaining == 0 {
                self.value = self.target;
            }
        }
        self.value
    }
}
struct Gain {
    gain: Smooth,
}
impl Filter for Gain {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (x, y) in input.iter().zip(output) {
            *y = *x * self.gain.next();
        }
    }
    fn set_params(&mut self, p: &BTreeMap<String, f32>) {
        self.gain.set(10.0_f32.powf(p["gain_db"] / 20.0));
    }
}
pub struct Delay {
    samples: Vec<f32>,
    cursor: usize,
}
impl Delay {
    pub fn new(frames: usize) -> Self {
        Self {
            samples: vec![0.0; frames],
            cursor: 0,
        }
    }
    pub fn next(&mut self, value: f32) -> f32 {
        if self.samples.is_empty() {
            return value;
        }
        let old = self.samples[self.cursor];
        self.samples[self.cursor] = value;
        self.cursor = (self.cursor + 1) % self.samples.len();
        old
    }
}
struct Node {
    processor: Box<dyn Filter>,
    enabled: Smooth,
    delay: Delay,
}
pub struct Chain {
    pub configs: Vec<FilterConfig>,
    nodes: Vec<Node>,
    a: [f32; BLOCK],
    b: [f32; BLOCK],
    pub latency: usize,
}
impl Chain {
    pub fn new(configs: &[FilterConfig]) -> Result<Self, String> {
        validate(configs)?;
        let mut nodes = Vec::with_capacity(configs.len());
        let mut latency = 0;
        for config in configs {
            let processor: Box<dyn Filter> = match config.kind.as_str() {
                "gain" => Box::new(Gain {
                    gain: Smooth::new(10.0_f32.powf(config.params["gain_db"] / 20.0)),
                }),
                "frequency" => Box::new(Frequency::new(config.params["shift_hz"])),
                "highpass" => Box::new(Cutoff::new(config.params["cutoff"], true)),
                "lowpass" => Box::new(Cutoff::new(config.params["cutoff"], false)),
                "distortion" => Box::new(Distortion {
                    drive: Smooth::new(10.0_f32.powf(config.params["drive_db"] / 20.0)),
                    mix: Smooth::new(config.params["mix"] / 100.0),
                }),
                _ => return Err("未知滤镜".into()),
            };
            let frames = processor.latency();
            latency += frames;
            nodes.push(Node {
                processor,
                enabled: Smooth::new(if config.enabled { 1.0 } else { 0.0 }),
                delay: Delay::new(frames),
            });
        }
        Ok(Self {
            configs: configs.to_vec(),
            nodes,
            a: [0.0; BLOCK],
            b: [0.0; BLOCK],
            latency,
        })
    }
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        assert_eq!(input.len(), output.len());
        assert!(input.len() <= BLOCK);
        let len = input.len();
        self.a[..len].copy_from_slice(input);
        for node in &mut self.nodes {
            node.processor.process(&self.a[..len], &mut self.b[..len]);
            for i in 0..len {
                let dry = node.delay.next(self.a[i]);
                self.b[i] = dry + (self.b[i] - dry) * node.enabled.next();
            }
            std::mem::swap(&mut self.a, &mut self.b);
        }
        output.copy_from_slice(&self.a[..len]);
    }
    pub fn same_structure(&self, other: &Self) -> bool {
        self.configs.len() == other.configs.len()
            && self
                .configs
                .iter()
                .zip(&other.configs)
                .all(|(a, b)| a.id == b.id && a.kind == b.kind)
    }
    pub fn update_from(&mut self, other: &Self) {
        for ((node, config), next) in self
            .nodes
            .iter_mut()
            .zip(&mut self.configs)
            .zip(&other.configs)
        {
            node.enabled.set(if next.enabled { 1.0 } else { 0.0 });
            config.enabled = next.enabled;
            if config.params != next.params {
                node.processor.set_params(&next.params);
                for (key, value) in &mut config.params {
                    *value = next.params[key];
                }
            }
        }
    }
}

use biquad::{Biquad, Coefficients, DirectForm1, ToHertz, Type, Q_BUTTERWORTH_F32};
struct Frequency {
    history: [f32; 255],
    coefficients: [f32; 255],
    cursor: usize,
    phase: f64,
    shift: Smooth,
    activity: Smooth,
    raw: Delay,
    lowpass: DirectForm1<f32>,
}
impl Frequency {
    fn new(shift: f32) -> Self {
        let mut coefficients = [0.0; 255];
        for (k, c) in coefficients.iter_mut().enumerate() {
            let n = k as i32 - 127;
            if n % 2 != 0 {
                let window = 0.42 - 0.5 * (std::f64::consts::TAU * k as f64 / 254.0).cos()
                    + 0.08 * (2.0 * std::f64::consts::TAU * k as f64 / 254.0).cos();
                *c = (2.0 / (std::f64::consts::PI * n as f64) * window) as f32;
            }
        }
        let coefficients_lp = Coefficients::<f32>::from_params(
            Type::LowPass,
            RATE.hz(),
            20000.0.hz(),
            Q_BUTTERWORTH_F32,
        )
        .expect("fixed valid filter");
        Self {
            history: [0.0; 255],
            coefficients,
            cursor: 0,
            phase: 0.0,
            shift: Smooth::new(shift),
            activity: Smooth::new(if shift == 0.0 { 0.0 } else { 1.0 }),
            raw: Delay::new(127),
            lowpass: DirectForm1::new(coefficients_lp),
        }
    }
}
impl Filter for Frequency {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (x, y) in input.iter().zip(output) {
            let dry = self.raw.next(*x);
            self.history[self.cursor] = self.lowpass.run(*x);
            let real = self.history[(self.cursor + 255 - 127) % 255];
            let mut imaginary = 0.0;
            for k in (0..255).step_by(2) {
                imaginary += self.coefficients[k] * self.history[(self.cursor + 255 - k) % 255];
            }
            let shifted = real * self.phase.cos() as f32 - imaginary * self.phase.sin() as f32;
            let mix = self.activity.next();
            *y = dry + (shifted - dry) * mix;
            self.phase = (self.phase
                + std::f64::consts::TAU * self.shift.next() as f64 / RATE as f64)
                .rem_euclid(std::f64::consts::TAU);
            self.cursor = (self.cursor + 1) % 255;
        }
    }
    fn set_params(&mut self, p: &BTreeMap<String, f32>) {
        self.shift.set(p["shift_hz"]);
        self.activity
            .set(if p["shift_hz"] == 0.0 { 0.0 } else { 1.0 });
    }
    fn latency(&self) -> usize {
        127
    }
}

struct Cutoff {
    filter: DirectForm1<f32>,
    cutoff: Smooth,
    highpass: bool,
    tick: usize,
}
impl Cutoff {
    fn new(cutoff: f32, highpass: bool) -> Self {
        Self {
            filter: DirectForm1::new(Self::coefficients(cutoff, highpass)),
            cutoff: Smooth::new(cutoff),
            highpass,
            tick: 0,
        }
    }
    fn coefficients(cutoff: f32, highpass: bool) -> Coefficients<f32> {
        Coefficients::from_params(
            if highpass {
                Type::HighPass
            } else {
                Type::LowPass
            },
            RATE.hz(),
            cutoff.clamp(20.0, 20000.0).hz(),
            Q_BUTTERWORTH_F32,
        )
        .expect("validated cutoff")
    }
}
impl Filter for Cutoff {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (x, y) in input.iter().zip(output) {
            let frequency = self.cutoff.next();
            if self.tick % 16 == 0 {
                self.filter
                    .update_coefficients(Self::coefficients(frequency, self.highpass));
            }
            self.tick = self.tick.wrapping_add(1);
            *y = self.filter.run(*x);
        }
    }
    fn set_params(&mut self, p: &BTreeMap<String, f32>) {
        self.cutoff.set(p["cutoff"]);
    }
}
struct Distortion {
    drive: Smooth,
    mix: Smooth,
}
impl Filter for Distortion {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (x, y) in input.iter().zip(output) {
            let drive = self.drive.next();
            let mix = self.mix.next();
            let wet = (*x * drive).tanh() / drive.tanh();
            *y = *x + (wet - *x) * mix;
        }
    }
    fn set_params(&mut self, p: &BTreeMap<String, f32>) {
        self.drive.set(10.0_f32.powf(p["drive_db"] / 20.0));
        self.mix.set(p["mix"] / 100.0);
    }
}
