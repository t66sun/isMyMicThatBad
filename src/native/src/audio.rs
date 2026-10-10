use crate::{
    clock::{Converter, Drift},
    dsp::{Chain, FilterConfig, BLOCK, RATE},
    graph::{Graph, Runner},
    settings::Session,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig, SupportedStreamConfig,
};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    mpsc::{sync_channel, Receiver, SyncSender, TrySendError},
    Arc, Mutex, OnceLock,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub input: bool,
    pub is_default: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub running: bool,
    pub monitoring: bool,
    pub input_peak: f32,
    pub output_peak: f32,
    pub underruns: u64,
    pub overflows: u64,
    pub device_xruns: u64,
    pub error: Option<String>,
    pub sample_rate: u32,
}
#[derive(Default)]
struct Stats {
    stop: AtomicBool,
    ready: AtomicBool,
    monitoring: AtomicBool,
    monitor_epoch: AtomicU64,
    input_peak: AtomicU32,
    output_peak: AtomicU32,
    underruns: AtomicU64,
    overflows: AtomicU64,
    device_xruns: AtomicU64,
    error: Mutex<Option<String>>,
    worker: OnceLock<thread::Thread>,
}
impl Stats {
    fn fail(&self, error: String, monitor: bool) {
        if let Ok(mut slot) = self.error.lock() {
            *slot = Some(if monitor {
                format!("监听设备错误：{error}。主输出继续，请停止后重新选择耳机。")
            } else {
                format!("音频设备错误：{error}。处理已停止，请检查设备后重新开始。")
            });
        }
        if monitor {
            self.monitoring.store(false, Ordering::Relaxed);
        } else {
            self.stop.store(true, Ordering::Relaxed);
        }
        self.wake();
    }
    fn wake(&self) {
        if let Some(worker) = self.worker.get() {
            worker.unpark();
        }
    }
}
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
    let devices: Vec<Device> = if input {
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
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data, _| {
                let mut peak = 0.0_f32;
                for frame in data.chunks_exact(channels) {
                    let value =
                        frame.iter().map(|v| v.to_sample::<f32>()).sum::<f32>() / channels as f32;
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
        let input = vec![0.0; converter.max_input_frames()];
        Ok(Self {
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
    fn next(&mut self) -> f32 {
        if self.stats.stop.load(Ordering::Relaxed) {
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
                    self.stats.underruns.fetch_add(1, Ordering::Relaxed);
                    self.starving = true;
                }
                return 0.0;
            }
            for value in &mut self.input[..needed] {
                *value = self.source.pop().unwrap_or(0.0);
            }
            if let Err(error) = self
                .converter
                .process(&self.input[..needed], &mut self.output)
            {
                self.stats.fail(error, self.monitor);
                return 0.0;
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
struct Runtime {
    capture: Stream,
    output: Stream,
    monitor: Option<Stream>,
    worker: Option<JoinHandle<()>>,
    stats: Arc<Stats>,
    pending: Arc<Mutex<Option<Graph>>>,
    retired: Receiver<Graph>,
}
impl Runtime {
    fn new(settings: &Session) -> Result<Self, String> {
        settings.validate()?;
        let input = find(&settings.input_id, true)?;
        let main = find(&settings.output_id, false)?;
        if endpoint_name(&input)?.contains("CABLE Output")
            && endpoint_name(&main)?.contains("CABLE In")
        {
            return Err("不能将虚拟线缆回送到自身，请选择真实麦克风".into());
        }
        let input_config = configuration(&input, true)?;
        let input_rate = input_config.sample_rate();
        let main_config = configuration(&main, false)?;
        let target = (settings.buffer_ms as usize * 48).max(BLOCK);
        let goal = target + BLOCK;
        let input_target = (settings.buffer_ms as usize * input_rate as usize / 1000).max(BLOCK);
        let stats = Arc::new(Stats::default());
        stats
            .monitoring
            .store(settings.monitoring, Ordering::Relaxed);
        let (capture_producer, mut capture_consumer) =
            RingBuffer::new((input_rate as usize / 10).max(BLOCK * 8));
        let capacity = (goal * 4).max(4800);
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
        )?;
        let output_rate = main_config.sample_rate();
        let output = build_output(&main, main_config, reader, Arc::clone(&stats), false)?;
        let period = output.buffer_size().map_err(|e| e.to_string())? as usize;
        let goal = goal.max((period * 2 * 48000 / output_rate as usize) + BLOCK);
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
                        )?,
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
        let mut runner = Runner::new(Graph::new(Chain::new(&settings.chain)?));
        let pending = Arc::new(Mutex::new(None::<Graph>));
        let changes = Arc::clone(&pending);
        let (retired_sender, retired) = sync_channel(32);
        let worker_stats = Arc::clone(&stats);
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
                            if let Some(graph) = slot.take() {
                                deferred = runner
                                    .submit(graph)
                                    .and_then(|g| retire(g, &retired_sender));
                            }
                        }
                    }
                    // The output resampler needs one extra block when priming its sinc history.
                    let demand = goal
                        + if worker_stats.ready.load(Ordering::Relaxed) {
                            0
                        } else {
                            BLOCK
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
                    if let Some(graph) = runner.process(&input, &mut processed) {
                        deferred = retire(graph, &retired_sender);
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
        let mut runtime = Self {
            capture,
            output,
            monitor,
            worker: Some(worker),
            stats,
            pending,
            retired,
        };
        runtime.capture.play().map_err(|e| e.to_string())?;
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
                .error
                .lock()
                .ok()
                .and_then(|error| error.clone())
                .unwrap_or_else(|| "未收到麦克风音频，请检查设备和 Windows 麦克风权限".into()));
        }
        runtime.output.play().map_err(|e| e.to_string())?;
        if settings.monitoring {
            runtime.set_monitoring(true)?;
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
    runtime: Option<Runtime>,
    last_error: Option<String>,
}
impl Controller {
    pub fn start(&mut self, settings: &Session) -> Result<(), String> {
        if self.runtime.is_some() {
            return Err("处理已经运行".into());
        }
        self.last_error = None;
        match Runtime::new(settings) {
            Ok(runtime) => {
                self.runtime = Some(runtime);
                Ok(())
            }
            Err(error) => {
                self.last_error = Some(error.clone());
                Err(error)
            }
        }
    }
    pub fn stop(&mut self) {
        self.runtime.take();
        self.last_error = None;
    }
    pub fn set_chain(&mut self, filters: &[FilterConfig]) -> Result<(), String> {
        let graph = Graph::new(Chain::new(filters)?);
        if let Some(runtime) = &self.runtime {
            *runtime.pending.lock().map_err(|_| "音频控制状态不可用")? = Some(graph);
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
    pub fn status(&mut self) -> Status {
        if self
            .runtime
            .as_ref()
            .is_some_and(|r| r.stats.stop.load(Ordering::Relaxed))
        {
            self.last_error = self
                .runtime
                .as_ref()
                .and_then(|r| r.stats.error.lock().ok().and_then(|v| v.clone()))
                .or_else(|| Some("音频处理已停止，请检查设备后重新开始".into()));
            self.runtime.take();
        }
        if let Some(runtime) = &self.runtime {
            for _ in runtime.retired.try_iter() {}
            Status {
                running: true,
                monitoring: runtime.stats.monitoring.load(Ordering::Relaxed),
                input_peak: f32::from_bits(runtime.stats.input_peak.load(Ordering::Relaxed)),
                output_peak: f32::from_bits(runtime.stats.output_peak.load(Ordering::Relaxed)),
                underruns: runtime.stats.underruns.load(Ordering::Relaxed),
                overflows: runtime.stats.overflows.load(Ordering::Relaxed),
                device_xruns: runtime.stats.device_xruns.load(Ordering::Relaxed),
                error: runtime.stats.error.lock().ok().and_then(|s| s.clone()),
                sample_rate: RATE as u32,
            }
        } else {
            Status {
                running: false,
                monitoring: false,
                input_peak: 0.0,
                output_peak: 0.0,
                underruns: 0,
                overflows: 0,
                device_xruns: 0,
                error: self.last_error.clone(),
                sample_rate: RATE as u32,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitoring_failure_preserves_main_output() {
        let stats = Arc::new(Stats::default());
        stats.monitoring.store(true, Ordering::Relaxed);
        stream_error(Arc::clone(&stats), true)(cpal::ErrorKind::DeviceNotAvailable.into());
        assert!(!stats.monitoring.load(Ordering::Relaxed));
        assert!(!stats.stop.load(Ordering::Relaxed));
        assert!(stats
            .error
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .contains("主输出继续"));
    }
    #[test]
    fn recoverable_device_notifications_keep_audio_running() {
        let stats = Arc::new(Stats::default());
        let mut callback = stream_error(Arc::clone(&stats), false);
        callback(cpal::ErrorKind::Xrun.into());
        callback(cpal::ErrorKind::DeviceChanged.into());
        callback(cpal::ErrorKind::RealtimeDenied.into());
        assert!(!stats.stop.load(Ordering::Relaxed));
        assert_eq!(stats.device_xruns.load(Ordering::Relaxed), 1);
        callback(cpal::ErrorKind::DeviceNotAvailable.into());
        assert!(stats.stop.load(Ordering::Relaxed));
        assert!(stats.error.lock().unwrap().is_some());
    }
    #[test]
    fn starvation_is_one_event_and_reader_recovers_after_refill() {
        let stats = Arc::new(Stats::default());
        let (mut producer, consumer) = RingBuffer::new(2048);
        let mut reader = Reader::new(consumer, 48000, 480, Arc::clone(&stats), false).unwrap();
        for _ in 0..100 {
            assert_eq!(reader.next(), 0.0);
        }
        assert_eq!(stats.underruns.load(Ordering::Relaxed), 1);
        for _ in 0..1024 {
            producer.push(0.1).unwrap();
        }
        for _ in 0..512 {
            assert!(reader.next().is_finite());
        }
        assert!(!reader.starving);
        while reader.source.pop().is_ok() {}
        reader.position = BLOCK;
        reader.next();
        assert_eq!(stats.underruns.load(Ordering::Relaxed), 2);
    }
}
