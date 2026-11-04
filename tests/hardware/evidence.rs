use super::*;
use mic_engine::trace::Trace;
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::BufWriter,
    path::PathBuf,
    time::Instant,
};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

fn device(id: &str) -> cpal::Device {
    cpal::default_host()
        .devices()
        .unwrap()
        .find(|d| d.id().unwrap().to_string() == id)
        .unwrap_or_else(|| panic!("unavailable endpoint {id}"))
}

struct Meter {
    stream: cpal::Stream,
    source: Consumer<Frame>,
    frames: Vec<Frame>,
    rate: u32,
}
fn meter_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut sink: rtrb::Producer<Frame>,
    errors: Arc<Mutex<Vec<String>>>,
) -> cpal::Stream
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let rate = config.sample_rate;
    let channels = config.channels as usize;
    let (mut phase, mut count, mut power, mut start) = (0u32, 0u32, 0f64, 0f64);
    device
        .build_input_stream(
            config,
            move |data: &[T], info| {
                let timestamp = info.timestamp().capture.as_nanos() as f64 / 1e9;
                for (i, frame) in data.chunks_exact(channels).enumerate() {
                    if count == 0 {
                        start = timestamp + i as f64 / rate as f64;
                    }
                    let mono = frame
                        .iter()
                        .map(|x| cpal::Sample::to_sample::<f32>(*x) as f64)
                        .sum::<f64>()
                        / channels as f64;
                    power += mono * mono;
                    count += 1;
                    phase += 500;
                    if phase >= rate {
                        assert!(
                            sink.push(Frame {
                                time: start,
                                value: (power / count as f64).sqrt() as f32
                            })
                            .is_ok(),
                            "envelope observer overflow"
                        );
                        phase -= rate;
                        count = 0;
                        power = 0.;
                    }
                }
            },
            move |e| {
                if e.kind() != cpal::ErrorKind::Xrun {
                    errors.lock().unwrap().push(e.to_string());
                }
            },
            Some(Duration::from_secs(2)),
        )
        .unwrap()
}
impl Meter {
    fn new(id: &str, errors: &Arc<Mutex<Vec<String>>>) -> Self {
        let device = device(id);
        let config = device
            .default_input_config()
            .or_else(|e| {
                if e.kind() == cpal::ErrorKind::UnsupportedOperation {
                    device.default_output_config()
                } else {
                    Err(e)
                }
            })
            .unwrap();
        let rate = config.sample_rate();
        let (sink, source) = RingBuffer::new(500 * 200);
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                meter_stream::<f32>(&device, config.config(), sink, errors.clone())
            }
            cpal::SampleFormat::I16 => {
                meter_stream::<i16>(&device, config.config(), sink, errors.clone())
            }
            cpal::SampleFormat::I32 => {
                meter_stream::<i32>(&device, config.config(), sink, errors.clone())
            }
            f => panic!("unsupported observer format {f}"),
        };
        stream.play().unwrap();
        Self {
            stream,
            source,
            frames: Vec::new(),
            rate,
        }
    }
    fn poll(&mut self) -> (usize, f32) {
        let start = self.frames.len();
        while let Ok(frame) = self.source.pop() {
            self.frames.push(frame);
        }
        let new = &self.frames[start..];
        (new.len(), new.iter().map(|f| f.value).fold(0., f32::max))
    }
    fn pause(&self) {
        self.stream.pause().unwrap();
    }
}
fn calibration(reference: &[Frame], output: &[Frame], maximum_ms: usize) -> Value {
    // Full 90 s trials; sample the start, middle and end of a long soak.
    let mut pairs = Vec::new();
    let starts = if reference.len() > 500 * 100 {
        vec![0, reference.len() / 2, reference.len().saturating_sub(9000)]
    } else {
        vec![0]
    };
    for start in starts {
        let end = if reference.len() > 500 * 100 {
            (start + 9000).min(reference.len())
        } else {
            reference.len()
        };
        pairs.extend(correlate_with_max_delay(
            &reference[start..end],
            output,
            500,
            10f64.powf(-50. / 20.),
            maximum_ms,
        ));
    }
    let mut matched: Vec<_> = pairs
        .iter()
        .filter(|(_, s)| *s >= 0.5)
        .map(|p| p.0)
        .collect();
    matched.sort_by(f64::total_cmp);
    let qualified = pairs.len() >= 20 && matched.len() * 5 >= pairs.len() * 4;
    let p95 = qualified.then(|| matched[(matched.len() * 95).div_ceil(100) - 1]);
    json!({"qualified":qualified,"windows":pairs.len(),"matched":matched.len(),"p95_ms":p95,"raw":pairs})
}

#[test]
#[ignore = "plays a synthetic WAV through selected hardware and captures signal envelopes"]
fn fixed_chain() {
    let dir = PathBuf::from(env("MIC_TEST_RUN_DIR"));
    let session: Session =
        serde_json::from_slice(&fs::read(env("MIC_TEST_SESSION")).unwrap()).unwrap();
    let seconds: u64 = env("MIC_TEST_SECONDS").parse().unwrap();
    assert!((1..=1800).contains(&seconds));
    let trace = Trace::default();
    let mut engine = Controller::with_ai(env("MIC_TEST_AI_ROOT").into());
    engine.observe(trace.clone());
    let mut timeline = BufWriter::new(File::create_new(dir.join("timeline.jsonl")).unwrap());
    let mut levels = BufWriter::new(File::create_new(dir.join("levels.jsonl")).unwrap());
    let errors = Arc::new(Mutex::new(Vec::new()));
    let input_id = session.input_id.trim_start_matches("loopback:");
    let speaker_id = env("MIC_TEST_SPEAKER_ID");
    let mut input = Meter::new(input_id, &errors);
    let mut cable = Meter::new(&env("MIC_TEST_CABLE_READER_ID"), &errors);
    let mut speaker = (input_id != speaker_id).then(|| Meter::new(&speaker_id, &errors));
    let mut monitor = session
        .monitoring
        .then(|| Meter::new(&session.monitor_id, &errors));
    let stimulus = play_stimulus(errors.clone()).unwrap();
    // Open and start the independent observers before the engine's first real-time hop.
    // The bounded preroll covers the 60 s ready and 120 s model-open deadlines.
    let start = Instant::now();
    if let Err(e) = engine.start(&session) {
        fs::write(
            dir.join("result.json"),
            json!({"outcome":"start_failed","error":e,"status":engine.status()}).to_string(),
        )
        .unwrap();
        panic!("start failed: {e}");
    }
    for meter in [&mut input, &mut cable]
        .into_iter()
        .chain(speaker.iter_mut())
        .chain(monitor.iter_mut())
    {
        meter.poll();
        meter.frames.clear();
    }
    let play = Instant::now();
    let mut first = None;
    let mut baseline = None;
    let mut last_source = None;
    let mut last_cable = None;
    let mut last_monitor = None;
    let mut outage = None;
    let status = loop {
        std::thread::sleep(Duration::from_millis(100));
        let now = Instant::now();
        let status = engine.status();
        let incoming = input.poll();
        let received = cable.poll();
        let heard = monitor.as_mut().map(Meter::poll);
        let played = speaker.as_mut().map(Meter::poll);
        let floor = 10f32.powf(-50. / 20.);
        if incoming.1 >= floor {
            last_source = Some(now);
        }
        if received.1 >= floor {
            last_cable = Some(now);
        }
        if heard.is_some_and(|m| m.1 >= floor) {
            last_monitor = Some(now);
        }
        if first.is_none() && received.1 >= floor {
            first = Some(now);
            baseline = Some(status.clone());
        }
        for event in trace.drain() {
            writeln!(timeline, "{}", serde_json::to_string(&event).unwrap()).unwrap();
        }
        writeln!(levels,"{}",json!({"t_us":trace.now_us(),"input":incoming,"cable":received,"monitor":heard,"stimulus":played,"status":status})).unwrap();
        if !status.running || status.error.is_some() || !errors.lock().unwrap().is_empty() {
            break status;
        }
        if let Some(t) = first {
            if last_source.is_some_and(|s| now.duration_since(s).as_millis() < 300)
                && now.duration_since(t).as_secs() >= 3
            {
                if last_cable.is_none_or(|c| now.duration_since(c).as_secs() >= 3) {
                    outage = Some("cable_silent");
                    break status;
                }
                if session.monitoring
                    && last_monitor.is_none_or(|c| now.duration_since(c).as_secs() >= 3)
                {
                    outage = Some("monitor_silent");
                    break status;
                }
            }
            if now.duration_since(t).as_secs() >= seconds {
                break status;
            }
        } else if play.elapsed().as_secs() >= 15 {
            break status;
        }
    };
    let end = Instant::now();
    input.pause();
    cable.pause();
    if let Some(m) = &speaker {
        m.pause();
    }
    if let Some(m) = &monitor {
        m.pause();
    }
    drop(stimulus);
    engine.stop();
    for event in trace.drain() {
        writeln!(timeline, "{}", serde_json::to_string(&event).unwrap()).unwrap();
    }
    timeline.flush().unwrap();
    levels.flush().unwrap();
    let latency = calibration(&input.frames, &cable.frames, 1500);
    let acoustic = speaker
        .as_ref()
        .map(|s| calibration(&s.frames, &input.frames, 300));
    let monitor_match = monitor
        .as_ref()
        .map(|s| calibration(&input.frames, &s.frames, 1500));
    let callbacks = errors.lock().unwrap().clone();
    let outcome = if !status.running {
        "main_stopped"
    } else if session.monitoring && !status.monitoring {
        "monitor_stopped"
    } else if !callbacks.is_empty() {
        "capture_failed"
    } else if status.error.is_some() {
        "engine_error"
    } else if first.is_none() {
        "no_valid_output"
    } else if let Some(o) = outage {
        o
    } else if acoustic.as_ref().is_some_and(|a| a["qualified"] != true) {
        "unqualified_stimulus"
    } else if latency["qualified"] != true {
        "unqualified_output"
    } else if monitor_match
        .as_ref()
        .is_some_and(|a| a["qualified"] != true)
    {
        "unqualified_monitor"
    } else if baseline.as_ref().is_some_and(|s| {
        status.underruns > s.underruns
            || status.overflows > s.overflows
            || status.device_xruns > s.device_xruns
    }) {
        "counters_grew"
    } else if trace.lost() != 0 {
        "incomplete_trace"
    } else {
        "stable"
    };
    let result = json!({"outcome":outcome,"observer_rates":{"input":input.rate,"cable":cable.rate,"speaker":speaker.as_ref().map(|m|m.rate),"monitor":monitor.as_ref().map(|m|m.rate)},"engine":status,"baseline":baseline,"first_output_s":first.map(|t|t.duration_since(play).as_secs_f64()),"seconds_after_output":first.map(|t|end.duration_since(t).as_secs_f64()),"engine_setup_s":play.duration_since(start).as_secs_f64(),"latency":latency,"stimulus_to_mic":acoustic,"monitor_match":monitor_match,"trace_lost":trace.lost(),"callback_errors":callbacks});
    fs::write(
        dir.join("result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    assert_eq!(outcome, "stable", "see {}", dir.display());
}
