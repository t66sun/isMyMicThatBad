//! Optional bounded observation of the production audio path. Each producer has one writer.
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    #[cfg(feature = "acceptance-replay")]
    ReplayInput {
        capture_ns: u64,
        offset: u64,
        frames: usize,
    },
    Config {
        input_hz: u32,
        output_hz: u32,
        period: usize,
        target: usize,
        startup: usize,
    },
    Render {
        monitor: bool,
        peak: f32,
        clipped: usize,
    },
    Underrun {
        monitor: bool,
        available: usize,
        needed: usize,
    },
    Dsp {
        elapsed_us: u64,
        capture_frames: usize,
        render_frames: usize,
    },
    AiQueue {
        input_frames: usize,
        playable_frames: usize,
    },
    AiRequest {
        hop: u64,
        input_frames: usize,
    },
    AiReply {
        hop: u64,
        inference_us: u64,
        roundtrip_us: u64,
        playable_frames: usize,
    },
    Fault {
        reason: &'static str,
    },
}

#[derive(Serialize)]
pub struct Sample {
    pub t_us: u64,
    #[serde(flatten)]
    pub event: Event,
}

struct Shared {
    start: Instant,
    readers: Mutex<Vec<Consumer<Sample>>>,
    lost: AtomicU64,
}

#[derive(Clone)]
pub struct Trace(Arc<Shared>);

impl Default for Trace {
    fn default() -> Self {
        Self(Arc::new(Shared {
            start: Instant::now(),
            readers: Mutex::new(Vec::new()),
            lost: AtomicU64::new(0),
        }))
    }
}

impl Trace {
    pub fn now_us(&self) -> u64 {
        self.0.start.elapsed().as_micros() as u64
    }
    pub(crate) fn writer(&self) -> Writer {
        let (producer, reader) = RingBuffer::new(4096);
        self.0.readers.lock().unwrap().push(reader);
        Writer {
            producer,
            trace: self.clone(),
        }
    }
    /// Drain on the controller thread. All timestamps use one monotonic clock.
    pub fn drain(&self) -> Vec<Sample> {
        let mut samples = Vec::new();
        self.0.readers.lock().unwrap().retain_mut(|reader| {
            let finished = reader.is_abandoned();
            while let Ok(sample) = reader.pop() {
                samples.push(sample);
            }
            !finished
        });
        samples.sort_by_key(|s| s.t_us);
        samples
    }
    pub fn lost(&self) -> u64 {
        self.0.lost.load(Ordering::Relaxed)
    }
}

pub(crate) struct Writer {
    producer: Producer<Sample>,
    trace: Trace,
}
impl Writer {
    pub fn push(&mut self, event: Event) {
        let sample = Sample {
            t_us: self.trace.now_us(),
            event,
        };
        if self.producer.push(sample).is_err() {
            self.trace.0.lost.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_trace_reports_loss_and_keeps_the_final_events_of_retired_writers() {
        let trace = Trace::default();
        let mut writer = trace.writer();
        for _ in 0..4097 {
            writer.push(Event::Fault { reason: "test" });
        }
        drop(writer);
        assert_eq!(trace.lost(), 1);
        let events = trace.drain();
        assert_eq!(events.len(), 4096);
        assert!(events.windows(2).all(|p| p[0].t_us <= p[1].t_us));
        assert!(trace.drain().is_empty());
    }
}
