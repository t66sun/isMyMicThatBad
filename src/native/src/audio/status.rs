use super::*;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub running: bool,
    pub bypassed: bool,
    pub monitoring: bool,
    pub input_peak: f32,
    pub output_peak: f32,
    pub underruns: u64,
    pub overflows: u64,
    pub device_xruns: u64,
    pub failure_kind: Option<&'static str>,
    pub failure_after_start_ms: Option<u64>,
    pub error: Option<String>,
    pub sample_rate: u32,
}
enum StopReason {
    Filter(crate::ai::Failure),
    Device(String),
}
#[derive(Clone, Copy)]
struct Counters {
    underruns: u64,
    overflows: u64,
    device_xruns: u64,
}
struct Terminal {
    at: Instant,
    reason: StopReason,
    counters: Counters,
}
#[derive(Default)]
pub(super) struct Stats {
    #[cfg(feature = "acceptance-replay")]
    pub(super) replay: Option<(Arc<[f32]>, Trace)>,
    pub(super) stop: AtomicBool,
    pub(super) ready: AtomicBool,
    pub(super) monitoring: AtomicBool,
    pub(super) bypassed: AtomicBool,
    pub(super) monitor_epoch: AtomicU64,
    pub(super) input_peak: AtomicU32,
    pub(super) output_peak: AtomicU32,
    pub(super) underruns: AtomicU64,
    pub(super) overflows: AtomicU64,
    pub(super) device_xruns: AtomicU64,
    claimed: AtomicBool,
    terminal: OnceLock<Terminal>,
    pub(super) started: OnceLock<Instant>,
    pub(super) error: Mutex<Option<String>>,
    pub(super) worker: OnceLock<thread::Thread>,
}
impl Stats {
    fn counters(&self) -> Counters {
        Counters {
            underruns: self.underruns.load(Ordering::Relaxed),
            overflows: self.overflows.load(Ordering::Relaxed),
            device_xruns: self.device_xruns.load(Ordering::Relaxed),
        }
    }
    fn finish(&self, reason: StopReason) {
        if !self.claimed.swap(true, Ordering::AcqRel) {
            let _ = self.terminal.set(Terminal {
                at: Instant::now(),
                reason,
                counters: self.counters(),
            });
            self.stop.store(true, Ordering::Release);
            self.wake();
        }
    }
    pub(super) fn filter_failed(&self, fault: crate::ai::Failure) {
        self.finish(StopReason::Filter(fault));
    }
    pub(super) fn fail(&self, error: String, monitor: bool) {
        if monitor {
            if let Ok(mut slot) = self.error.lock() {
                *slot = Some(format!(
                    "监听设备错误：{error}。主输出继续，请停止后重新选择耳机。"
                ));
            }
            self.monitoring.store(false, Ordering::Relaxed);
            self.wake();
        } else {
            self.finish(StopReason::Device(error));
        }
    }
    pub(super) fn snapshot(&self, running: bool) -> Status {
        let terminal = self.terminal.get();
        let counters = terminal
            .map(|t| t.counters)
            .unwrap_or_else(|| self.counters());
        let (kind, error) = match terminal.map(|t| &t.reason) {
            Some(StopReason::Filter(f)) => (Some(f.kind()), Some(f.message().to_owned())),
            Some(StopReason::Device(e)) => (
                Some("audio_error"),
                Some(format!(
                    "音频设备错误：{e}。处理已停止，请检查设备后重新开始。"
                )),
            ),
            None => (None, self.error.lock().ok().and_then(|e| e.clone())),
        };
        Status {
            running,
            bypassed: running && self.bypassed.load(Ordering::Relaxed),
            monitoring: running && self.monitoring.load(Ordering::Relaxed),
            input_peak: if running {
                f32::from_bits(self.input_peak.load(Ordering::Relaxed))
            } else {
                0.0
            },
            output_peak: if running {
                f32::from_bits(self.output_peak.load(Ordering::Relaxed))
            } else {
                0.0
            },
            underruns: counters.underruns,
            overflows: counters.overflows,
            device_xruns: counters.device_xruns,
            failure_kind: kind,
            failure_after_start_ms: terminal.and_then(|t| {
                self.started
                    .get()
                    .map(|start| t.at.saturating_duration_since(*start).as_millis() as u64)
            }),
            error,
            sample_rate: RATE as u32,
        }
    }
    pub(super) fn wake(&self) {
        if let Some(worker) = self.worker.get() {
            worker.unpark();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_fatal_cause_and_queue_counters_survive_followup_notifications() {
        let stats = Stats::default();
        stats.started.set(Instant::now()).unwrap();
        stats.underruns.store(4, Ordering::Relaxed);
        stats.filter_failed(crate::ai::Failure::Starved);
        stats.underruns.fetch_add(1, Ordering::Relaxed);
        stats.fail("later error".into(), false);
        stats.fail("headphones".into(), true);
        let terminal = stats.snapshot(false);
        assert_eq!(terminal.failure_kind, Some("ai_output_empty"));
        assert_eq!(terminal.underruns, 4);
        assert!(terminal.failure_after_start_ms.is_some());
        assert!(terminal.error.unwrap().contains("未赶上"));
    }
}
