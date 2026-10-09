use crate::dsp::{Chain, BLOCK};
const HISTORY: usize = 64 * 127 + 1;
struct Padding {
    samples: Box<[f32]>,
    cursor: usize,
    delay: usize,
    previous_delay: usize,
    fade: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::FilterConfig;
    #[test]
    fn adding_a_delayed_filter_preserves_waveform_continuity() {
        let mut runner = Runner::new(Graph::new(Chain::new(&[]).unwrap()));
        let mut position = 0;
        let mut previous = 0.0;
        for block in 0..48 {
            if block == 32 {
                runner.submit(Graph::new(
                    Chain::new(&[FilterConfig {
                        id: "delay".into(),
                        kind: "frequency".into(),
                        enabled: true,
                        params: [("shift_hz".into(), 0.0)].into_iter().collect(),
                    }])
                    .unwrap(),
                ));
            }
            let input = std::array::from_fn::<_, BLOCK, _>(|i| {
                0.15 * (std::f32::consts::TAU * 500.0 * (position + i) as f32 / 48000.0).sin()
            });
            let mut output = [0.0; BLOCK];
            runner.process(&input, &mut output);
            for value in output {
                assert!(
                    (value - previous).abs() < 0.012,
                    "delay insertion must not jump between waveform positions"
                );
                previous = value;
            }
            position += BLOCK;
        }
    }
}
impl Padding {
    fn new() -> Self {
        Self {
            samples: vec![0.0; HISTORY].into_boxed_slice(),
            cursor: 0,
            delay: 0,
            previous_delay: 0,
            fade: 960,
        }
    }
    fn set_delay(&mut self, delay: usize) {
        if delay != self.delay {
            self.previous_delay = self.delay;
            self.delay = delay;
            self.fade = 0;
        }
    }
    fn process(&mut self, input: &mut [f32]) {
        for sample in input {
            self.samples[self.cursor] = *sample;
            let delayed = self.samples[(self.cursor + HISTORY - self.delay) % HISTORY];
            let previous = self.samples[(self.cursor + HISTORY - self.previous_delay) % HISTORY];
            let mix = (self.fade as f32 / 960.0).min(1.0);
            *sample = previous + (delayed - previous) * mix;
            self.fade = (self.fade + 1).min(960);
            self.cursor = (self.cursor + 1) % HISTORY;
        }
    }
}
pub struct Graph {
    pub chain: Box<Chain>,
    padding: Padding,
}
impl Graph {
    pub fn new(chain: Chain) -> Self {
        Self {
            chain: Box::new(chain),
            padding: Padding::new(),
        }
    }
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        self.chain.process(input, output);
        self.padding.process(output);
    }
}
pub struct Runner {
    current: Graph,
    next: Option<Graph>,
    warmup: usize,
    fade: usize,
    latency: usize,
    old: [f32; BLOCK],
    new: [f32; BLOCK],
}
impl Runner {
    pub fn new(graph: Graph) -> Self {
        let latency = graph.chain.latency;
        Self {
            current: graph,
            next: None,
            warmup: 0,
            fade: 0,
            latency,
            old: [0.0; BLOCK],
            new: [0.0; BLOCK],
        }
    }
    pub fn transitioning(&self) -> bool {
        self.next.is_some()
    }
    pub fn submit(&mut self, mut graph: Graph) -> Option<Graph> {
        if self.current.chain.same_structure(&graph.chain) {
            self.current.chain.update_from(&graph.chain);
            return Some(graph);
        }
        // shortcut: retain peak FIR delay until stop/start, reset when reducing live chain latency matters.
        let padding_changed = graph.chain.latency > self.latency;
        self.latency = self.latency.max(graph.chain.latency);
        self.current
            .padding
            .set_delay(self.latency - self.current.chain.latency);
        graph.padding.delay = self.latency - graph.chain.latency;
        self.warmup = self.latency + BLOCK + if padding_changed { 960 } else { 0 };
        self.fade = 0;
        self.next = Some(graph);
        None
    }
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) -> Option<Graph> {
        self.current.process(input, &mut self.old[..input.len()]);
        if let Some(next) = &mut self.next {
            next.process(input, &mut self.new[..input.len()]);
            if self.warmup > 0 {
                self.warmup = self.warmup.saturating_sub(input.len());
                output.copy_from_slice(&self.old[..input.len()]);
                return None;
            }
            for i in 0..input.len() {
                let mix = (self.fade as f32 / 960.0).min(1.0);
                output[i] = self.old[i] * (1.0 - mix) + self.new[i] * mix;
                self.fade += 1;
            }
            if self.fade >= 960 {
                let replacement = self.next.take().expect("transition exists");
                return Some(std::mem::replace(&mut self.current, replacement));
            }
        } else {
            output.copy_from_slice(&self.old[..input.len()]);
        }
        None
    }
}
