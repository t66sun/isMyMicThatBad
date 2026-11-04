use super::*;
#[derive(Clone, Debug)]
pub struct WorkerError {
    pub(super) kind: &'static str,
    pub(super) message: Arc<str>,
}
impl WorkerError {
    pub(super) fn timeout() -> Self {
        Self {
            kind: "ai_worker_timeout",
            message: "AI 推理响应超时，处理已停止".into(),
        }
    }
    pub(super) fn disconnected() -> Self {
        Self {
            kind: "ai_worker_exit",
            message: "AI 推理进程已断开，请查看 worker.log".into(),
        }
    }
    pub(super) fn io(error: std::io::Error) -> Self {
        if matches!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::UnexpectedEof
        ) || matches!(error.raw_os_error(), Some(109 | 232 | 233))
        {
            Self::disconnected()
        } else {
            error.to_string().into()
        }
    }
}
impl From<&str> for WorkerError {
    fn from(message: &str) -> Self {
        Self {
            kind: "ai_worker_error",
            message: message.into(),
        }
    }
}
impl From<String> for WorkerError {
    fn from(message: String) -> Self {
        Self {
            kind: "ai_worker_error",
            message: message.into(),
        }
    }
}
impl From<WorkerError> for String {
    fn from(error: WorkerError) -> Self {
        error.message.to_string()
    }
}
#[derive(Clone, Debug)]
pub enum Failure {
    InputFull,
    Starved,
    Worker(WorkerError),
}
impl Failure {
    pub fn message(&self) -> &str {
        match self {
            Self::InputFull => "AI 输入队列溢出，处理已停止",
            Self::Starved => "AI 推理未赶上实时音频，处理已停止",
            Self::Worker(error) => &error.message,
        }
    }
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::InputFull => "ai_input_full",
            Self::Starved => "ai_output_empty",
            Self::Worker(error) => error.kind,
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    pub(super) stopped: AtomicBool,
    pub(super) cause: AtomicU8,
    worker_error: OnceLock<WorkerError>,
}
impl State {
    const INPUT_FULL: u8 = 1;
    const OUTPUT_EMPTY: u8 = 2;
    const WORKER_FAILED: u8 = 3;

    pub(super) fn input_full(&self) {
        self.record(Self::INPUT_FULL);
    }
    pub(super) fn output_empty(&self) {
        self.record(Self::OUTPUT_EMPTY);
    }
    fn record(&self, cause: u8) {
        let _ = self
            .cause
            .compare_exchange(0, cause, Ordering::Release, Ordering::Relaxed);
    }
    pub(super) fn fail(&self, error: WorkerError) {
        let _ = self.worker_error.set(error);
        self.record(Self::WORKER_FAILED);
    }
    pub(super) fn failure(&self) -> Option<Failure> {
        match self.cause.load(Ordering::Acquire) {
            Self::INPUT_FULL => Some(Failure::InputFull),
            Self::OUTPUT_EMPTY => Some(Failure::Starved),
            Self::WORKER_FAILED => self.worker_error.get().cloned().map(Failure::Worker),
            _ => None,
        }
    }
}
