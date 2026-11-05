use crate::{
    clock::{Converter, Drift},
    dsp::{self, Chain, FilterConfig, BLOCK, RATE},
    graph::{Graph, Runner},
    settings::Session,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig, SupportedStreamConfig,
};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    mpsc::{sync_channel, Receiver, SyncSender, TrySendError},
    Arc, Mutex, OnceLock,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
mod startup;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub input: bool,
    pub is_default: bool,
}
mod status;
use crate::trace::{Event, Trace};
use status::Stats;
pub use status::Status;
fn endpoint_id(device: &Device) -> Result<String, String> {
    device
        .id()
        .map(|id| id.to_string())
        .map_err(|e| e.to_string())
}
fn endpoint_name(device: &Device) -> Result<String, String> {
    device
        .description()
        .map(|d| d.name().to_string())
        .map_err(|e| e.to_string())
}
pub fn list_devices() -> Result<Vec<DeviceInfo>, String> {
    let host = cpal::default_host();
    let defaults = [
        host.default_input_device()
            .and_then(|d| endpoint_id(&d).ok()),
        host.default_output_device()
            .and_then(|d| endpoint_id(&d).ok()),
    ];
    let mut result = Vec::new();
    for input in [true, false] {
        let devices: Vec<Device> = if input {
            host.input_devices().map_err(|e| e.to_string())?.collect()
        } else {
            host.output_devices().map_err(|e| e.to_string())?.collect()
        };
        for device in devices {
            let id = endpoint_id(&device)?;
            let name = endpoint_name(&device)?;
            let is_default = defaults[if input { 0 } else { 1 }].as_ref() == Some(&id);
            result.push(DeviceInfo {
                id,
                name,
                input,
                is_default,
            });
        }
    }
    Ok(result)
}
fn find(id: &str, input: bool) -> Result<Device, String> {
    let host = cpal::default_host();
    let loopback = input && id.starts_with("loopback:");
    let id = if loopback { &id[9..] } else { id };
    let devices: Vec<Device> = if input && !loopback {
        host.input_devices().map_err(|e| e.to_string())?.collect()
    } else {
        host.output_devices().map_err(|e| e.to_string())?.collect()
    };
    devices
        .into_iter()
        .find(|d| endpoint_id(d).ok().as_deref() == Some(id))
        .ok_or_else(|| "所选音频设备不可用，请重新选择".into())
}
fn configuration(device: &Device, input: bool) -> Result<SupportedStreamConfig, String> {
    let default = if input {
        device.default_input_config()
    } else {
        device.default_output_config()
    }
    .map_err(|e| e.to_string())?;
    let ranges: Vec<_> = if input {
        device
            .supported_input_configs()
            .map_err(|e| e.to_string())?
            .collect()
    } else {
        device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .collect()
    };
    Ok(ranges
        .into_iter()
        .find(|r| {
            r.channels() == default.channels()
                && r.sample_format() == SampleFormat::F32
                && r.min_sample_rate() <= 48000
                && r.max_sample_rate() >= 48000
        })
        .map(|r| r.with_sample_rate(48000))
        .unwrap_or(default))
}
fn stream_error(stats: Arc<Stats>, monitor: bool) -> impl FnMut(cpal::Error) + Send + 'static {
    move |error| {
        match error.kind() {
            cpal::ErrorKind::Xrun => {
                stats.device_xruns.fetch_add(1, Ordering::Relaxed);
                return;
            }
            cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied => return,
            _ => {}
        }
        stats.fail(error.to_string(), monitor);
    }
}
fn capture<T>(
    device: &Device,
    config: StreamConfig,
    mut producer: Producer<f32>,
    stats: Arc<Stats>,
) -> Result<Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let errors = Arc::clone(&stats);
    #[cfg(feature = "acceptance-replay")]
    let mut replay = stats
        .replay
        .as_ref()
        .map(|(samples, trace)| (samples.clone(), trace.writer(), 0u64));
    #[cfg(feature = "acceptance-replay")]
    if replay.is_some() && config.sample_rate != 48000 {
        return Err("Clocked replay requires 48 kHz capture".into());
    }
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data, _info| {
                #[cfg(feature = "acceptance-replay")]
                if let Some((_, writer, offset)) = &mut replay {
                    writer.push(Event::ReplayInput {
                        capture_ns: _info.timestamp().capture.as_nanos() as u64,
                        offset: *offset,
                        frames: data.len() / channels,
                    });
                }
                let mut peak = 0.0_f32;
                for frame in data.chunks_exact(channels) {
                    let microphone = || {
                        frame.iter().map(|v| v.to_sample::<f32>()).sum::<f32>() / channels as f32
                    };
                    #[cfg(not(feature = "acceptance-replay"))]
                    let value = microphone();
                    #[cfg(feature = "acceptance-replay")]
                    let value = if let Some((samples, _, offset)) = &mut replay {
                        let sample = samples[*offset as usize % samples.len()];
                        *offset += 1;
                        sample
                    } else {
                        microphone()
                    };
                    let value = if value.is_finite() { value } else { 0.0 };
                    peak = peak.max(value.abs());
                    if producer.push(value).is_err() {
                        stats.overflows.fetch_add(1, Ordering::Relaxed);
                    }
                }
                stats.input_peak.store(peak.to_bits(), Ordering::Relaxed);
                stats.wake();
            },
            stream_error(errors, false),
            Some(Duration::from_secs(2)),
        )
        .map_err(|e| e.to_string())
}
struct Reader {
    trace: Option<crate::trace::Writer>,
    source: Consumer<f32>,
    converter: Converter,
    drift: Drift,
    input: Vec<f32>,
    output: [f32; BLOCK],
    position: usize,
    target: usize,
    stats: Arc<Stats>,
    monitor: bool,
    epoch: u64,
    warming: bool,
    starving: bool,
}
impl Reader {
    fn new(
        source: Consumer<f32>,
        rate: u32,
        target: usize,
        stats: Arc<Stats>,
        monitor: bool,
    ) -> Result<Self, String> {
        let converter = Converter::new(48000, rate)?;
        let target = target.min(
            source
                .buffer()
                .capacity()
                .saturating_sub(converter.max_input_frames()),
        );
        let input = vec![0.0; converter.max_input_frames()];
        Ok(Self {
            trace: None,
            source,
            converter,
            drift: Drift::new(),
            input,
            output: [0.0; BLOCK],
            position: BLOCK,
            target,
            stats,
            monitor,
            epoch: 0,
            warming: monitor,
            starving: false,
        })
    }
    fn observed(mut self, trace: Option<&Trace>) -> Self {
        self.trace = trace.map(Trace::writer);
        self
    }
    fn next(&mut self) -> f32 {
        if self.stats.stop.load(Ordering::Relaxed) || !self.stats.ready.load(Ordering::Relaxed) {
            return 0.0;
        }
        if self.monitor {
            if !self.stats.monitoring.load(Ordering::Relaxed) {
                return 0.0;
            }
            let epoch = self.stats.monitor_epoch.load(Ordering::Relaxed);
            if epoch != self.epoch {
                self.epoch = epoch;
                while self.source.pop().is_ok() {}
                self.converter.reset();
                self.position = BLOCK;
                self.warming = true;
            }
            if self.warming {
                if self.source.slots() < self.target + self.converter.input_frames() {
                    return 0.0;
                }
                self.warming = false;
            }
        }
        if self.position == BLOCK {
            if self.monitor {
                let ratio = self.drift.update(
                    self.source.slots(),
                    self.target + self.converter.input_frames(),
                );
                if let Err(error) = self.converter.set_relative(ratio) {
                    self.stats.fail(error, true);
                    return 0.0;
                }
            }
            let needed = self.converter.input_frames();
            if self.source.slots() < needed {
                if !self.starving {
                    if let Some(writer) = &mut self.trace {
                        writer.push(Event::Underrun {
                            monitor: self.monitor,
                            available: self.source.slots(),
                            needed,
                        });
                    }
                    self.stats.underruns.fetch_add(1, Ordering::Relaxed);
                    self.starving = true;
                }
                return 0.0;
            }
            for value in &mut self.input[..needed] {
                let sample = self.source.pop().unwrap_or(0.0);
                // Inaudible tails otherwise incur denormal penalties in the sinc callback.
                *value = if sample.abs() < 1e-20 { 0.0 } else { sample };
            }
            if let Err(error) = self
                .converter
                .process(&self.input[..needed], &mut self.output)
            {
                self.stats.fail(error, self.monitor);
                return 0.0;
            }
            if let Some(writer) = &mut self.trace {
                let peak = self
                    .output
                    .iter()
                    .fold(0.0_f32, |peak, x| peak.max(x.abs()));
                let clipped = self.output.iter().filter(|x| x.abs() > 1.).count();
                writer.push(Event::Render {
                    monitor: self.monitor,
                    peak,
                    clipped,
                });
            }
            self.position = 0;
            self.starving = false;
        }
        let value = self.output[self.position];
        self.position += 1;
        if value.is_finite() {
            value.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }
}
fn output<T>(
    device: &Device,
    config: StreamConfig,
    mut reader: Reader,
    stats: Arc<Stats>,
    monitor: bool,
) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let errors = Arc::clone(&stats);
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data, _| {
                for frame in data.chunks_exact_mut(channels) {
                    let value = reader.next();
                    #[cfg(feature = "acceptance-replay")]
                    let value = if monitor && reader.stats.replay.is_some() {
                        0.
                    } else {
                        value
                    };
                    for sample in frame {
                        *sample = T::from_sample(value);
                    }
                }
                if !monitor {
                    reader.stats.wake();
                }
            },
            stream_error(errors, monitor),
            Some(Duration::from_secs(2)),
        )
        .map_err(|e| e.to_string())
}
fn build_capture(
    device: &Device,
    config: SupportedStreamConfig,
    producer: Producer<f32>,
    stats: Arc<Stats>,
) -> Result<Stream, String> {
    let format = config.sample_format();
    let config = config.config();
    match format {
        SampleFormat::F32 => capture::<f32>(device, config, producer, stats),
        SampleFormat::F64 => capture::<f64>(device, config, producer, stats),
        SampleFormat::I16 => capture::<i16>(device, config, producer, stats),
        SampleFormat::I32 => capture::<i32>(device, config, producer, stats),
        SampleFormat::U16 => capture::<u16>(device, config, producer, stats),
        _ => Err(format!("不支持输入格式 {format:?}")),
    }
}
fn build_output(
    device: &Device,
    config: SupportedStreamConfig,
    reader: Reader,
    stats: Arc<Stats>,
    monitor: bool,
) -> Result<Stream, String> {
    let format = config.sample_format();
    let config = config.config();
    match format {
        SampleFormat::F32 => output::<f32>(device, config, reader, stats, monitor),
        SampleFormat::F64 => output::<f64>(device, config, reader, stats, monitor),
        SampleFormat::I16 => output::<i16>(device, config, reader, stats, monitor),
        SampleFormat::I32 => output::<i32>(device, config, reader, stats, monitor),
        SampleFormat::U16 => output::<u16>(device, config, reader, stats, monitor),
        _ => Err(format!("不支持输出格式 {format:?}")),
    }
}
fn retire(graph: Graph, sender: &SyncSender<Graph>) -> Option<Graph> {
    match sender.try_send(graph) {
        Ok(()) => None,
        Err(TrySendError::Full(graph)) | Err(TrySendError::Disconnected(graph)) => Some(graph),
    }
}
enum Update {
    Graph(Graph),
    Params(Vec<FilterConfig>),
}
struct Runtime {
    capture: Stream,
    output: Stream,
    monitor: Option<Stream>,
    worker: Option<JoinHandle<()>>,
    stats: Arc<Stats>,
    pending: Arc<Mutex<Option<Update>>>,
    retired: Receiver<Graph>,
    configs: Vec<FilterConfig>,
}
impl Runtime {
    fn new(
        settings: &Session,
        ai_root: Option<&Path>,
        trace: Option<&Trace>,
        stats: Arc<Stats>,
    ) -> Result<Self, String> {
        settings.validate()?;
        let input = find(&settings.input_id, true)?;
        let main = find(&settings.output_id, false)?;
        #[cfg(feature = "acceptance-replay")]
        if stats.replay.is_some() && !endpoint_name(&main)?.starts_with("CABLE Input") {
            return Err("Clocked replay requires the virtual CABLE Input destination".into());
        }
        if endpoint_id(&input)? == settings.output_id
            || (endpoint_name(&input)?.contains("CABLE")
                && endpoint_name(&main)?.contains("CABLE In"))
        {
            return Err("不能将输出回送到自身，请选择不同的输入与输出设备".into());
        }
        // WASAPI captures render endpoints in loopback mode for deterministic WAV acceptance.
        let input_config = configuration(&input, !settings.input_id.starts_with("loopback:"))?;
        let input_rate = input_config.sample_rate();
        let main_config = configuration(&main, false)?;
        let target = (settings.buffer_ms as usize * 48).max(BLOCK);
        let goal = target + BLOCK;
        let input_target = (settings.buffer_ms as usize * input_rate as usize / 1000).max(BLOCK);
        stats
            .monitoring
            .store(settings.monitoring, Ordering::Relaxed);
        let (capture_producer, mut capture_consumer) =
            RingBuffer::new((input_rate as usize / 10).max(BLOCK * 8));
        let capacity = (goal * 4).max(4800).div_ceil(BLOCK) * BLOCK;
        let (mut main_producer, main_consumer) = RingBuffer::new(capacity);
        let (mut monitor_producer, monitor_consumer) = RingBuffer::new(capacity);
        let mut converter = Converter::new(input_rate, 48000)?;
        let mut input_buffer = vec![0.0; converter.max_input_frames()];
        let capture = build_capture(&input, input_config, capture_producer, Arc::clone(&stats))?;
        let reader = Reader::new(
            main_consumer,
            main_config.sample_rate(),
            target,
            Arc::clone(&stats),
            false,
        )?
        .observed(trace);
        let output_rate = main_config.sample_rate();
        let output = build_output(&main, main_config, reader, Arc::clone(&stats), false)?;
        let period = output.buffer_size().map_err(|e| e.to_string())? as usize;
        // Three periods cover larger WASAPI callbacks and worker jitter; leave space for sinc priming.
        let goal = goal
            .max((period * 3 * 48000 / output_rate as usize) + BLOCK)
            .min(capacity - BLOCK);
        let startup = (goal + BLOCK + period * 48000 / output_rate as usize).min(capacity);
        let monitor =
            if !settings.monitor_id.is_empty() && settings.monitor_id != settings.output_id {
                let attempt = (|| {
                    let device = find(&settings.monitor_id, false)?;
                    let config = configuration(&device, false)?;
                    build_output(
                        &device,
                        config.clone(),
                        Reader::new(
                            monitor_consumer,
                            config.sample_rate(),
                            goal,
                            Arc::clone(&stats),
                            true,
                        )?
                        .observed(trace),
                        Arc::clone(&stats),
                        true,
                    )
                })();
                if settings.monitoring {
                    Some(attempt?)
                } else {
                    attempt.ok()
                }
            } else {
                if settings.monitoring {
                    return Err("监听需要选择不同于主输出的耳机设备".into());
                }
                None
            };
        let mut runner = Runner::new(Graph::new(Chain::observed(
            &settings.chain,
            ai_root,
            trace,
        )?));
        let pending = Arc::new(Mutex::new(None::<Update>));
        let changes = Arc::clone(&pending);
        let (retired_sender, retired) = sync_channel(32);
        let worker_stats = Arc::clone(&stats);
        let mut timing = trace.map(Trace::writer);
        if let Some(writer) = &mut timing {
            writer.push(Event::Config {
                input_hz: input_rate,
                output_hz: output_rate,
                period,
                target: goal,
                startup,
            });
        }
        let worker = thread::Builder::new()
            .name("mic-dsp".into())
            .spawn(move || {
                let priority = match audio_thread_priority::promote_current_thread_to_real_time(
                    BLOCK as u32,
                    RATE as u32,
                ) {
                    Ok(handle) => Some(handle),
                    Err(error) => {
                        eprintln!("DSP thread scheduling unavailable: {error}");
                        None
                    }
                };
                let mut input = [0.0; BLOCK];
                let mut processed = [0.0; BLOCK];
                let mut drift = Drift::new();
                let mut deferred = None;
                let mut warming = true;
                while !worker_stats.stop.load(Ordering::Relaxed) {
                    if let Some(graph) = deferred.take() {
                        deferred = retire(graph, &retired_sender);
                    }
                    if deferred.is_none() && !runner.transitioning() {
                        if let Ok(mut slot) = changes.try_lock() {
                            if let Some(update) = slot.take() {
                                match update {
                                    Update::Graph(graph) => {
                                        deferred = runner
                                            .submit(graph)
                                            .and_then(|g| retire(g, &retired_sender))
                                    }
                                    Update::Params(configs) => runner.update_configs(&configs),
                                }
                            }
                        }
                    }
                    // Prime sinc history and retain one device period for initial capture jitter.
                    let demand = if worker_stats.ready.load(Ordering::Relaxed) {
                        goal
                    } else {
                        startup
                    };
                    if capacity - main_producer.slots() >= demand {
                        worker_stats.ready.store(true, Ordering::Relaxed);
                        thread::park_timeout(Duration::from_millis(20));
                        continue;
                    }
                    let filled = capture_consumer.slots();
                    if warming && filled < input_target + converter.input_frames() {
                        thread::park_timeout(Duration::from_millis(20));
                        continue;
                    }
                    if filled < converter.input_frames() {
                        thread::park_timeout(Duration::from_millis(20));
                        continue;
                    }
                    let ratio = drift.update(filled, input_target + converter.input_frames());
                    if let Err(error) = converter.set_relative(ratio) {
                        worker_stats.fail(error, false);
                        break;
                    }
                    let needed = converter.input_frames();
                    if filled < needed {
                        thread::park_timeout(Duration::from_millis(20));
                        continue;
                    }
                    warming = false;
                    for value in &mut input_buffer[..needed] {
                        *value = capture_consumer.pop().unwrap_or(0.0);
                    }
                    if let Err(error) = converter.process(&input_buffer[..needed], &mut input) {
                        worker_stats.fail(error, false);
                        break;
                    }
                    runner.set_bypassed(worker_stats.bypassed.load(Ordering::Relaxed));
                    let block_start = timing.as_ref().map(|_| Instant::now());
                    if let Some(graph) = runner.process(&input, &mut processed) {
                        deferred = retire(graph, &retired_sender);
                    }
                    if let (Some(t), Some(writer)) = (block_start, &mut timing) {
                        writer.push(Event::Dsp {
                            elapsed_us: t.elapsed().as_micros() as u64,
                            capture_frames: filled,
                            render_frames: capacity - main_producer.slots(),
                        });
                    }
                    if let Some(fault) = runner.failure() {
                        if let Some(writer) = &mut timing {
                            writer.push(Event::Fault {
                                reason: fault.kind(),
                            });
                        }
                        worker_stats.filter_failed(fault);
                        break;
                    }
                    let peak = processed.iter().fold(0.0_f32, |a, x| a.max(x.abs()));
                    worker_stats
                        .output_peak
                        .store(peak.to_bits(), Ordering::Relaxed);
                    for value in processed {
                        if main_producer.push(value).is_err() {
                            worker_stats.overflows.fetch_add(1, Ordering::Relaxed);
                        }
                        if worker_stats.monitoring.load(Ordering::Relaxed)
                            && monitor_producer.push(value).is_err()
                        {
                            worker_stats.overflows.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                if let Some(handle) = priority {
                    let _ = audio_thread_priority::demote_current_thread_from_real_time(handle);
                }
            })
            .map_err(|e| e.to_string())?;
        let _ = stats.worker.set(worker.thread().clone());
        let runtime = Self {
            capture,
            output,
            monitor,
            worker: Some(worker),
            stats,
            pending,
            retired,
            configs: settings.chain.clone(),
        };
        // Start the render driver with silence before capture/output priming, not after it.
        let _ = runtime.stats.started.set(Instant::now());
        startup::start(
            || runtime.output.play().map_err(|e| e.to_string()),
            || {
                if let Some(stream) = runtime.monitor.as_ref().filter(|_| settings.monitoring) {
                    stream.play().map_err(|e| e.to_string())?;
                }
                Ok(())
            },
            || runtime.capture.play().map_err(|e| e.to_string()),
        )?;
        let deadline = Instant::now() + Duration::from_millis(750);
        while !runtime.stats.ready.load(Ordering::Relaxed)
            && !runtime.stats.stop.load(Ordering::Relaxed)
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(5));
        }
        if !runtime.stats.ready.load(Ordering::Relaxed) {
            return Err(runtime
                .stats
                .snapshot(false)
                .error
                .unwrap_or_else(|| "未收到麦克风音频，请检查设备和 Windows 麦克风权限".into()));
        }
        Ok(runtime)
    }
    fn set_monitoring(&mut self, enabled: bool) -> Result<(), String> {
        let stream = self
            .monitor
            .as_ref()
            .ok_or("监听设备不可用，请停止后重新选择耳机")?;
        if self.stats.monitoring.load(Ordering::Relaxed) != enabled {
            self.stats.monitor_epoch.fetch_add(1, Ordering::Relaxed);
        }
        self.stats.monitoring.store(enabled, Ordering::Relaxed);
        if let Err(error) = if enabled {
            stream.play()
        } else {
            stream.pause()
        } {
            self.stats.monitoring.store(!enabled, Ordering::Relaxed);
            return Err(error.to_string());
        }
        Ok(())
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.stats.stop.store(true, Ordering::Relaxed);
        self.stats.wake();
        let _ = self.capture.pause();
        let _ = self.output.pause();
        if let Some(m) = &self.monitor {
            let _ = m.pause();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        for _ in self.retired.try_iter() {}
    }
}
#[derive(Default)]
pub struct Controller {
    #[cfg(feature = "acceptance-replay")]
    replay: Option<Arc<[f32]>>,
    runtime: Option<Runtime>,
    last_status: Option<Status>,
    trace: Option<Trace>,
    ai_root: Option<PathBuf>,
}
impl Controller {
    pub fn with_ai(root: PathBuf) -> Self {
        Self {
            ai_root: Some(root),
            ..Self::default()
        }
    }
    pub fn observe(&mut self, trace: Trace) {
        self.trace = Some(trace);
    }
    #[cfg(feature = "acceptance-replay")]
    pub fn replay_capture(&mut self, samples: Arc<[f32]>, trace: Trace) -> Result<(), String> {
        if samples.is_empty() || self.runtime.is_some() {
            return Err("Set a nonempty replay before start".into());
        }
        self.trace = Some(trace);
        self.replay = Some(samples);
        Ok(())
    }
    pub fn start(&mut self, settings: &Session) -> Result<(), String> {
        if self.runtime.is_some() {
            return Err("处理已经运行".into());
        }
        self.last_status = None;
        let stats = Stats::default();
        #[cfg(feature = "acceptance-replay")]
        let stats = {
            let mut stats = stats;
            stats.replay = self.replay.clone().zip(self.trace.clone());
            stats
        };
        let stats = Arc::new(stats);
        match Runtime::new(
            settings,
            self.ai_root.as_deref(),
            self.trace.as_ref(),
            Arc::clone(&stats),
        ) {
            Ok(runtime) => {
                self.runtime = Some(runtime);
                Ok(())
            }
            Err(error) => {
                let mut status = stats.snapshot(false);
                status.error.get_or_insert_with(|| error.clone());
                self.last_status = Some(status);
                Err(error)
            }
        }
    }
    pub fn stop(&mut self) {
        self.runtime.take();
        self.last_status = None;
    }
    pub fn set_chain(&mut self, filters: &[FilterConfig]) -> Result<(), String> {
        dsp::validate(filters)?;
        if let Some(runtime) = &mut self.runtime {
            let old_model = runtime
                .configs
                .iter()
                .find(|c| c.kind == "rvc")
                .map(|c| &c.model);
            let new_model = filters.iter().find(|c| c.kind == "rvc").map(|c| &c.model);
            if old_model.is_some() && new_model.is_some() && old_model != new_model {
                return Err("切换 AI 模型前请先停止处理".into());
            }
            if dsp::same_structure(&runtime.configs, filters) {
                let mut pending = runtime.pending.lock().map_err(|_| "音频控制状态不可用")?;
                if let Some(Update::Graph(graph)) = pending.as_mut() {
                    graph.chain.update_configs(filters);
                } else {
                    *pending = Some(Update::Params(filters.to_vec()));
                }
            } else {
                let chain = Chain::observed(filters, self.ai_root.as_deref(), self.trace.as_ref())
                    .map_err(|error| {
                        if old_model.is_some() || new_model.is_some() {
                            runtime.stats.fail(error.clone(), false);
                        }
                        error
                    })?;
                *runtime.pending.lock().map_err(|_| "音频控制状态不可用")? =
                    Some(Update::Graph(Graph::new(chain)));
            }
            runtime.configs = filters.to_vec();
        }
        Ok(())
    }
    pub fn set_monitoring(&mut self, enabled: bool) -> Result<(), String> {
        if let Some(runtime) = &mut self.runtime {
            runtime.set_monitoring(enabled)
        } else {
            Ok(())
        }
    }
    pub fn set_bypassed(&mut self, bypassed: bool) -> Result<(), String> {
        let runtime = self.runtime.as_ref().ok_or("请先开始处理再对比原音")?;
        runtime.stats.bypassed.store(bypassed, Ordering::Relaxed);
        runtime.stats.wake();
        Ok(())
    }
    pub fn status(&mut self) -> Status {
        if let Some(runtime) = &self.runtime {
            if !runtime.stats.stop.load(Ordering::Acquire) {
                for _ in runtime.retired.try_iter() {}
                return runtime.stats.snapshot(true);
            }
            self.last_status = Some(runtime.stats.snapshot(false));
            self.runtime.take();
        }
        self.last_status
            .clone()
            .unwrap_or_else(|| Stats::default().snapshot(false))
    }
}

#[cfg(test)]
mod tests;
