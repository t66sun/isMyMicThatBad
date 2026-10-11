use crate::dsp::{AiModel, Filter, FilterConfig};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const HOP: usize = 12480;
pub const LATENCY: usize = HOP * 2;
const KEYS: [&str; 4] = ["pitch", "formant", "index", "rms"];
const REVISION: &str = "81eed5e8f68b6bed1789f682fe78cdd324495afc";

pub struct Preparation {
    root: PathBuf,
    child: Option<Child>,
    error: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationStatus {
    pub ready: bool,
    pub preparing: bool,
    pub error: Option<String>,
    pub model_path: String,
    pub index_path: String,
}
fn ready(root: &Path) -> bool {
    let marker = fs::read(root.join("ready.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    marker.is_some_and(|v| v["revision"] == REVISION && v["torch"] == "2.7.1+cu118")
        && [
            "python/python.exe",
            "rvc/infer/rtrvc.py",
            "rvc/assets/hubert_base/pytorch_model.bin",
            "rvc/assets/rmvpe/rmvpe.pt",
            "models/saisho.pth",
        ]
        .iter()
        .all(|path| root.join(path).is_file())
}
impl Preparation {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            child: None,
            error: None,
        }
    }
    pub fn status(&mut self) -> Result<PreparationStatus, String> {
        if let Some(child) = &mut self.child {
            if let Some(exit) = child.try_wait().map_err(|e| e.to_string())? {
                self.child = None;
                if !exit.success() || !ready(&self.root) {
                    self.error = Some(format!(
                        "AI 环境准备失败，请查看 {}",
                        self.root.join("setup.log").display()
                    ));
                }
            }
        }
        Ok(PreparationStatus {
            ready: ready(&self.root),
            preparing: self.child.is_some(),
            error: self.error.clone(),
            model_path: self.root.join("models/saisho.pth").to_string_lossy().into(),
            index_path: self
                .root
                .join("models/saisho.index")
                .to_string_lossy()
                .into(),
        })
    }
    pub fn start(&mut self) -> Result<(), String> {
        let status = self.status()?;
        if status.preparing || status.ready {
            return Ok(());
        }
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        fs::write(self.root.join("setup.ps1"), include_str!("../ai/setup.ps1"))
            .map_err(|e| e.to_string())?;
        let log = File::create(self.root.join("setup.log")).map_err(|e| e.to_string())?;
        self.child = Some(
            Command::new("powershell.exe")
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(self.root.join("setup.ps1"))
                .arg("-Root")
                .arg(&self.root)
                .creation_flags(0x08000000)
                .stdin(Stdio::null())
                .stdout(log.try_clone().map_err(|e| e.to_string())?)
                .stderr(log)
                .spawn()
                .map_err(|e| e.to_string())?,
        );
        self.error = None;
        Ok(())
    }
}
impl Drop for Preparation {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = Command::new("taskkill.exe")
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .creation_flags(0x08000000)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn PeekNamedPipe(
        handle: *mut std::ffi::c_void,
        buffer: *mut std::ffi::c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        remaining: *mut u32,
    ) -> i32;
}
fn read_pipe(pipe: &mut ChildStdout, data: &mut [u8], deadline: Instant) -> Result<(), String> {
    let mut offset = 0;
    while offset < data.len() {
        let mut available = 0;
        // Peek before reading so a hung Python worker cannot block shutdown indefinitely.
        let result = unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            return Err("AI 推理进程已断开，请查看 worker.log".into());
        }
        if Instant::now() >= deadline {
            return Err("AI 推理响应超时，处理已停止".into());
        }
        if available == 0 {
            thread::sleep(Duration::from_millis(1));
            continue;
        }
        let count = (available as usize).min(data.len() - offset);
        let read = pipe
            .read(&mut data[offset..offset + count])
            .map_err(|e| e.to_string())?;
        if read == 0 {
            return Err("AI 推理进程已退出".into());
        }
        offset += read;
    }
    Ok(())
}
struct Wire {
    input: ChildStdin,
    output: ChildStdout,
}
impl Wire {
    fn reply(&mut self, timeout: Duration) -> Result<Value, String> {
        let deadline = Instant::now() + timeout;
        let mut header = Vec::new();
        loop {
            let mut byte = [0];
            read_pipe(&mut self.output, &mut byte, deadline)?;
            if byte[0] == b'\n' {
                break;
            }
            header.push(byte[0]);
            if header.len() > 4096 {
                return Err("AI 响应格式无效".into());
            }
        }
        let response: Value = serde_json::from_slice(&header).map_err(|e| e.to_string())?;
        if response["ok"] != true {
            return Err(format!(
                "AI 推理失败：{}",
                response["error"].as_str().unwrap_or("未知错误")
            ));
        }
        Ok(response)
    }
    fn request(
        &mut self,
        header: Value,
        samples: &[u8],
        timeout: Duration,
    ) -> Result<Value, String> {
        serde_json::to_writer(&mut self.input, &header).map_err(|e| e.to_string())?;
        self.input
            .write_all(b"\n")
            .and_then(|_| self.input.write_all(samples))
            .and_then(|_| self.input.flush())
            .map_err(|e| e.to_string())?;
        self.reply(timeout)
    }
}
struct Service {
    child: Arc<Mutex<Child>>,
    wire: Mutex<Wire>,
    deadline: Arc<Mutex<Option<Instant>>>,
    stopped: Arc<AtomicBool>,
    watchdog: Option<JoinHandle<()>>,
}
impl Service {
    fn new(child: Child, wire: Wire) -> Self {
        let child = Arc::new(Mutex::new(child));
        let deadline: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let stopped = Arc::new(AtomicBool::new(false));
        let timer = Arc::clone(&deadline);
        let process = Arc::clone(&child);
        let stop = Arc::clone(&stopped);
        let watchdog = thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                if timer
                    .lock()
                    .ok()
                    .is_some_and(|limit| limit.is_some_and(|limit| Instant::now() >= limit))
                {
                    if let Ok(mut child) = process.lock() {
                        let _ = child.kill();
                    }
                }
                thread::sleep(Duration::from_millis(25));
            }
        });
        Self {
            child,
            wire: Mutex::new(wire),
            deadline,
            stopped,
            watchdog: Some(watchdog),
        }
    }
    fn call(
        &self,
        header: Value,
        bytes: &mut [u8],
        audio: bool,
        timeout: Duration,
    ) -> Result<Value, String> {
        let mut wire = self.wire.lock().map_err(|_| "AI 通信不可用")?;
        *self.deadline.lock().map_err(|_| "AI 超时控制不可用")? = Some(Instant::now() + timeout);
        let result = wire
            .request(header, if audio { bytes } else { &[] }, timeout)
            .and_then(|response| {
                if audio {
                    if response["frames"] != HOP {
                        return Err("AI 输出采样块无效".into());
                    }
                    read_pipe(&mut wire.output, bytes, Instant::now() + timeout)?;
                }
                Ok(response)
            });
        if let Ok(mut deadline) = self.deadline.lock() {
            *deadline = None;
        }
        if result.is_err() {
            if let Ok(mut child) = self.child.lock() {
                let _ = child.kill();
            }
        }
        result
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(watchdog) = self.watchdog.take() {
            let _ = watchdog.join();
        }
    }
}
type Cache = Option<(PathBuf, Option<AiModel>, Weak<Service>)>;
static SERVICE: OnceLock<Mutex<Cache>> = OnceLock::new();
static SEQUENCE: AtomicU64 = AtomicU64::new(1);
fn service(root: &Path, model: &Option<AiModel>) -> Result<Arc<Service>, String> {
    let mut cache = SERVICE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "AI 控制状态不可用")?;
    if let Some((path, previous, weak)) = cache.as_ref() {
        if path == root && previous == model {
            if let Some(service) = weak.upgrade() {
                return Ok(service);
            }
        }
        if weak.upgrade().is_some() {
            return Err("切换 AI 模型前请先停止处理".into());
        }
    }
    if !ready(root) {
        return Err("请先准备 AI 运行环境与测试模型".into());
    }
    fs::write(root.join("worker.py"), include_str!("../ai/worker.py"))
        .map_err(|e| e.to_string())?;
    let log = File::create(root.join("worker.log")).map_err(|e| e.to_string())?;
    let mut child = Command::new(root.join("python/python.exe"))
        .arg("-I")
        .arg("-u")
        .arg(root.join("worker.py"))
        .arg("--root")
        .arg(root)
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(log)
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut wire = Wire {
        input: child.stdin.take().unwrap(),
        output: child.stdout.take().unwrap(),
    };
    if let Err(error) = wire.reply(Duration::from_secs(60)) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let shared = Arc::new(Service::new(child, wire));
    *cache = Some((root.to_path_buf(), model.clone(), Arc::downgrade(&shared)));
    Ok(shared)
}
#[derive(Default)]
struct State {
    stopped: AtomicBool,
    failed: AtomicBool,
    error: Mutex<Option<String>>,
}
impl State {
    fn fail(&self, error: String) {
        if !self.failed.load(Ordering::Acquire) {
            if let Ok(mut slot) = self.error.lock() {
                *slot = Some(error);
            }
            self.failed.store(true, Ordering::Release);
        }
    }
}
pub(crate) struct Rvc {
    input: Producer<f32>,
    output: Consumer<f32>,
    state: Arc<State>,
    parameters: Arc<[AtomicU32; 4]>,
    worker: Option<JoinHandle<()>>,
}
impl Rvc {
    pub(crate) fn new(root: &Path, config: &FilterConfig) -> Result<Self, String> {
        let shared = service(root, &config.model)?;
        let id = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let model = config.model.as_ref();
        let path = model.map(|m| m.path.clone()).unwrap_or_default();
        let index = model
            .map(|m| m.index.clone())
            .unwrap_or_else(|| root.join("models/saisho.index").to_string_lossy().into());
        let reply = shared.call(
            json!({"op":"open","id":id,"path":path,"index":index}),
            &mut [],
            false,
            Duration::from_secs(120),
        )?;
        if reply["frames"] != HOP {
            return Err("AI 采样块格式不匹配".into());
        }
        let (input, mut source) = RingBuffer::<f32>::new(HOP * 4);
        let (mut sink, output) = RingBuffer::new(HOP * 4);
        for _ in 0..LATENCY {
            sink.push(0.0).unwrap();
        }
        let state = Arc::new(State::default());
        let status = Arc::clone(&state);
        let parameters = Arc::new(std::array::from_fn(|i| {
            AtomicU32::new(config.params[KEYS[i]].to_bits())
        }));
        let params = Arc::clone(&parameters);
        let worker = thread::Builder::new()
            .name("mic-rvc-bridge".into())
            .spawn(move || {
                let mut bytes = vec![0; HOP * 4];
                while !status.stopped.load(Ordering::Acquire)
                    && !status.failed.load(Ordering::Acquire)
                {
                    if source.slots() < HOP {
                        thread::park_timeout(Duration::from_millis(10));
                        continue;
                    }
                    for sample in bytes.chunks_exact_mut(4) {
                        sample.copy_from_slice(&source.pop().unwrap().to_le_bytes());
                    }
                    let values: BTreeMap<_, _> = KEYS
                        .iter()
                        .enumerate()
                        .map(|(i, key)| (*key, f32::from_bits(params[i].load(Ordering::Relaxed))))
                        .collect();
                    let result = (|| -> Result<(), String> {
                        shared.call(
                            json!({"op":"process","id":id,"params":values}),
                            &mut bytes,
                            true,
                            Duration::from_secs(2),
                        )?;
                        if sink.slots() < HOP {
                            return Err("AI 输出队列溢出，处理已停止".into());
                        }
                        for value in bytes.chunks_exact(4) {
                            let value = f32::from_le_bytes(value.try_into().unwrap());
                            if !value.is_finite() {
                                return Err("AI 输出包含无效采样".into());
                            }
                            sink.push(value).map_err(|_| "AI 输出队列溢出")?;
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        status.fail(error);
                        break;
                    }
                }
                let _ = shared.call(
                    json!({"op":"close","id":id}),
                    &mut [],
                    false,
                    Duration::from_secs(2),
                );
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            input,
            output,
            state,
            parameters,
            worker: Some(worker),
        })
    }
}
impl Filter for Rvc {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        output.fill(0.0);
        if self.state.failed.load(Ordering::Acquire) {
            return;
        }
        for value in input {
            if self.input.push(*value).is_err() {
                self.state.fail("AI 输入队列溢出，处理已停止".into());
                return;
            }
        }
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        for value in output.iter_mut() {
            match self.output.pop() {
                Ok(sample) => *value = sample,
                Err(_) => {
                    output.fill(0.0);
                    self.state.fail("AI 推理未赶上实时音频，处理已停止".into());
                    return;
                }
            }
        }
    }
    fn set_params(&mut self, params: &BTreeMap<String, f32>) {
        for (i, key) in KEYS.iter().enumerate() {
            self.parameters[i].store(params[*key].to_bits(), Ordering::Relaxed);
        }
    }
    fn latency(&self) -> usize {
        LATENCY
    }
    fn error(&self) -> Option<String> {
        if self.state.failed.load(Ordering::Acquire) {
            self.state.error.lock().ok().and_then(|error| error.clone())
        } else {
            None
        }
    }
}
impl Drop for Rvc {
    fn drop(&mut self) {
        self.state.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rvc_without_worker() -> (Rvc, Consumer<f32>, Producer<f32>) {
        rvc_without_worker_with_tail(0)
    }

    fn rvc_without_worker_with_tail(nonzero_tail: usize) -> (Rvc, Consumer<f32>, Producer<f32>) {
        let (input, captured_input) = RingBuffer::new(HOP * 4);
        let (mut prepared_output, output) = RingBuffer::new(HOP * 4);
        for index in 0..LATENCY {
            let sample = if index >= LATENCY - nonzero_tail {
                1.0
            } else {
                0.0
            };
            prepared_output.push(sample).unwrap();
        }
        let state = Arc::new(State::default());
        let parameters = Arc::new(std::array::from_fn(|_| AtomicU32::new(0)));
        (
            Rvc {
                input,
                output,
                state,
                parameters,
                worker: None,
            },
            captured_input,
            prepared_output,
        )
    }

    #[test]
    fn exhausted_latency_reserve_silences_and_permanently_fails() {
        assert_eq!(LATENCY, 24_960);
        const BLOCK: usize = 256;
        let (mut rvc, captured_input, _prepared_output) =
            rvc_without_worker_with_tail(LATENCY % BLOCK);

        for _ in 0..LATENCY / BLOCK {
            let mut output = vec![1.0; BLOCK];
            rvc.process(&[0.25; BLOCK], &mut output);
            assert!(output.iter().all(|sample| *sample == 0.0));
        }

        let mut failed_output = vec![1.0; BLOCK];
        rvc.process(&[0.5; BLOCK], &mut failed_output);
        assert!(failed_output.iter().all(|sample| *sample == 0.0));
        assert_eq!(
            rvc.error().as_deref(),
            Some("AI 推理未赶上实时音频，处理已停止")
        );
        let queued_input_after_failure = captured_input.slots();

        let mut later_output = vec![1.0; BLOCK];
        rvc.process(&[0.75; BLOCK], &mut later_output);
        assert!(later_output.iter().all(|sample| *sample == 0.0));
        assert_eq!(captured_input.slots(), queued_input_after_failure);
        assert_eq!(
            rvc.error().as_deref(),
            Some("AI 推理未赶上实时音频，处理已停止")
        );
    }

    #[test]
    fn prefailed_worker_state_silences_without_queueing_raw_input() {
        let (mut rvc, captured_input, _prepared_output) = rvc_without_worker();
        rvc.state.fail("fake worker failed".into());

        let mut output = vec![1.0; HOP];
        rvc.process(&[0.75; HOP], &mut output);

        assert!(output.iter().all(|sample| *sample == 0.0));
        assert_eq!(captured_input.slots(), 0);
        assert_eq!(rvc.error().as_deref(), Some("fake worker failed"));
    }

    #[test]
    fn wire_reply_stops_waiting_at_its_deadline() {
        let mut child = Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 5"])
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start sleeping fake worker");
        let mut wire = Wire {
            input: child.stdin.take().unwrap(),
            output: child.stdout.take().unwrap(),
        };

        let started = Instant::now();
        let result = wire.reply(Duration::from_millis(250));
        let elapsed = started.elapsed();
        drop(wire);
        let _ = child.kill();
        let _ = child.wait();

        let error = result.expect_err("a worker that never replies must time out");
        assert!(error.contains("超时"), "unexpected worker error: {error}");
        assert!(
            elapsed < Duration::from_secs(1),
            "reply blocked for {elapsed:?}"
        );
    }

    #[test]
    #[ignore = "self-spawns a sleeping test worker to verify the stdin watchdog"]
    fn hung_worker_ignores_stdin() {
        const CHILD_MODE: &str = "MIC_TEST_HUNG_RVC_WORKER";
        if std::env::var_os(CHILD_MODE).is_some() {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(b"MIC_TEST_HUNG_WORKER_READY\n").unwrap();
            stdout.flush().unwrap();
            thread::sleep(Duration::from_secs(5));
            return;
        }

        let mut child = Command::new(std::env::current_exe().expect("current test executable"))
            .args([
                "--ignored",
                "--exact",
                "ai::tests::hung_worker_ignores_stdin",
            ])
            .env(CHILD_MODE, "1")
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sleeping test worker");
        let mut output = child.stdout.take().unwrap();
        let ready = (|| -> Result<(), String> {
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut line = Vec::new();
            loop {
                let mut byte = [0];
                read_pipe(&mut output, &mut byte, deadline)?;
                if byte[0] == b'\n' {
                    if line.ends_with(b"MIC_TEST_HUNG_WORKER_READY") {
                        return Ok(());
                    }
                    line.clear();
                } else {
                    line.push(byte[0]);
                    if line.len() > 4096 {
                        return Err("test worker wrote an oversized startup line".into());
                    }
                }
            }
        })();
        if let Err(error) = ready {
            let _ = child.kill();
            let _ = child.wait();
            panic!("sleeping test worker did not become ready: {error}");
        }
        let wire = Wire {
            input: child.stdin.take().unwrap(),
            output,
        };
        let service = Service::new(child, wire);
        let process = Arc::clone(&service.child);
        let mut audio = vec![0_u8; HOP * 4];
        let started = Instant::now();
        let result = service.call(
            serde_json::json!({
                "op":"process",
                "id":1,
                "params":{"pitch":0,"formant":0,"index":50,"rms":50}
            }),
            &mut audio,
            true,
            Duration::from_millis(250),
        );
        let elapsed = started.elapsed();
        let exit = process
            .lock()
            .unwrap()
            .wait()
            .expect("wait for killed worker");

        assert!(result.is_err(), "hung worker unexpectedly replied");
        assert!(
            elapsed >= Duration::from_millis(200),
            "request failed before the watchdog deadline: {elapsed:?}; result={result:?}; worker exit={exit}"
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "request blocked for {elapsed:?}"
        );
        assert!(
            !exit.success(),
            "the sleeping worker exited normally instead of being killed: {exit}"
        );
        drop(service);
    }
}
