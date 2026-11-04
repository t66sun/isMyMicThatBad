use super::*;

#[derive(Clone, Copy)]
pub(super) struct Frame {
    pub(super) time: f64,
    pub(super) value: f32,
}

pub(super) fn record(
    device: &cpal::Device,
    errors: Arc<Mutex<Vec<String>>>,
) -> (cpal::Stream, Consumer<Frame>) {
    let config = device
        .default_input_config()
        .or_else(|error| {
            if error.kind() == cpal::ErrorKind::UnsupportedOperation {
                device.default_output_config()
            } else {
                Err(error)
            }
        })
        .unwrap();
    assert_eq!(
        config.sample_format(),
        cpal::SampleFormat::F32,
        "calibration requires native f32"
    );
    assert_eq!(
        config.sample_rate(),
        48000,
        "calibration requires both devices at 48 kHz"
    );
    let channels = config.channels() as usize;
    let (mut producer, consumer) = RingBuffer::new(48000 * 24);
    let stream = device
        .build_input_stream::<f32, _, _>(
            config.config(),
            move |data, info| {
                let start = info.timestamp().capture.as_nanos() as f64 / 1e9;
                for (i, values) in data.chunks_exact(channels).enumerate() {
                    let frame = Frame {
                        time: start + i as f64 / 48000.0,
                        value: values.iter().sum::<f32>() / channels as f32,
                    };
                    assert!(producer.push(frame).is_ok(), "calibration capture overflow");
                }
            },
            move |error| {
                if error.kind() != cpal::ErrorKind::Xrun {
                    errors.lock().unwrap().push(error.to_string());
                }
            },
            Some(Duration::from_secs(2)),
        )
        .unwrap();
    (stream, consumer)
}

pub(super) fn wav_samples(bytes: &[u8]) -> Vec<f32> {
    assert!(
        bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
        "stimulus must be a RIFF WAV"
    );
    let mut offset = 12;
    let mut format_valid = false;
    let mut samples = None;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let end = offset + 8 + size;
        let chunk = bytes
            .get(offset + 8..end)
            .expect("truncated stimulus WAV chunk");
        match &bytes[offset..offset + 4] {
            b"fmt " => {
                assert!(
                    chunk.len() >= 16
                        && chunk[..2] == 1_u16.to_le_bytes()
                        && chunk[2..4] == 1_u16.to_le_bytes()
                        && chunk[4..8] == 48000_u32.to_le_bytes()
                        && chunk[12..14] == 2_u16.to_le_bytes()
                        && chunk[14..16] == 16_u16.to_le_bytes(),
                    "stimulus requires mono PCM16 at 48 kHz"
                );
                format_valid = true;
            }
            b"data" => {
                assert!(
                    size > 0 && size % 2 == 0,
                    "stimulus WAV has invalid samples"
                );
                samples = Some(
                    chunk
                        .chunks_exact(2)
                        .map(|sample| {
                            i16::from_le_bytes(sample.try_into().unwrap()) as f32 / 32768.0
                        })
                        .collect::<Vec<_>>(),
                );
            }
            _ => {}
        }
        offset = end + size % 2;
    }
    assert!(format_valid, "stimulus WAV is missing its format");
    samples.expect("stimulus WAV is missing its samples")
}

pub(super) fn play_stimulus(errors: Arc<Mutex<Vec<String>>>) -> Option<cpal::Stream> {
    let path = std::env::var_os("MIC_TEST_STIMULUS_WAV")?;
    let mut samples = wav_samples(&std::fs::read(&path).expect("read stimulus WAV"));
    let peak = samples
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(peak > 0.0, "stimulus WAV is silent");
    for sample in &mut samples {
        *sample *= 0.25 / peak;
    }
    let id = std::env::var("MIC_TEST_SPEAKER_ID").expect("select physical playback speakers");
    let speaker = cpal::default_host()
        .output_devices()
        .unwrap()
        .find(|device| device.id().unwrap().to_string() == id)
        .expect("selected speakers are unavailable");
    let name = speaker.description().unwrap().name().to_owned();
    assert!(
        !name.contains("CABLE"),
        "stimulus must play through physical speakers"
    );
    let config = speaker.default_output_config().unwrap();
    assert_eq!(
        config.sample_format(),
        cpal::SampleFormat::F32,
        "stimulus playback requires native f32"
    );
    assert_eq!(
        config.sample_rate(),
        48000,
        "stimulus playback requires 48 kHz"
    );
    let channels = config.channels() as usize;
    let mut cursor = 0;
    let stream = speaker
        .build_output_stream::<f32, _, _>(
            config.config(),
            move |data, _| {
                for frame in data.chunks_exact_mut(channels) {
                    frame.fill(samples[cursor]);
                    cursor = (cursor + 1) % samples.len();
                }
            },
            move |error| {
                if error.kind() != cpal::ErrorKind::Xrun {
                    errors
                        .lock()
                        .unwrap()
                        .push(format!("stimulus playback: {error}"));
                }
            },
            Some(Duration::from_secs(2)),
        )
        .unwrap();
    println!(
        "{}",
        serde_json::json!({"stimulus":path.to_string_lossy(),"speaker":name,"method":"WAV playback; input is explicitly selected by MIC_TEST_INPUT_ID","peak":0.25})
    );
    stream.play().unwrap();
    Some(stream)
}

pub(super) fn reduce(mut source: Consumer<Frame>) -> Vec<Frame> {
    let mut result = Vec::new();
    while source.slots() >= 8 {
        let first = source.pop().unwrap();
        let mut sum = first.value;
        for _ in 1..8 {
            sum += source.pop().unwrap().value;
        }
        result.push(Frame {
            time: first.time,
            value: sum / 8.0,
        });
    }
    result
}

pub(super) fn correlate_at_rate(
    reference: &[Frame],
    cable: &[Frame],
    rate: usize,
    minimum_rms: f64,
) -> Vec<(f64, f64)> {
    correlate_with_max_delay(reference, cable, rate, minimum_rms, 300)
}

pub(super) fn correlate_with_max_delay(
    reference: &[Frame],
    cable: &[Frame],
    rate: usize,
    minimum_rms: f64,
    max_delay_ms: usize,
) -> Vec<(f64, f64)> {
    let mut results = Vec::new();
    for window in reference.windows(512).step_by(rate * 2 / 5).skip(5) {
        let mean = window.iter().map(|x| x.value as f64).sum::<f64>() / 512.0;
        let x: Vec<_> = window.iter().map(|x| x.value as f64 - mean).collect();
        let energy = x.iter().map(|x| x * x).sum::<f64>();
        if energy < 1e-16 || energy / 512.0 + mean * mean < minimum_rms * minimum_rms {
            continue;
        }
        let origin = cable.partition_point(|x| x.time < window[0].time);
        let mut best = (0.0, 0.0);
        for offset in 0..rate * max_delay_ms / 1000 {
            let begin = origin + offset;
            if begin + 512 >= cable.len() {
                break;
            }
            let y = &cable[begin..begin + 512];
            let ym = y.iter().map(|x| x.value as f64).sum::<f64>() / 512.0;
            let mut dot = 0.0;
            let mut norm = 0.0;
            for (a, b) in x.iter().zip(y) {
                let b = b.value as f64 - ym;
                dot += a * b;
                norm += b * b;
            }
            let score = dot / (energy * norm).sqrt();
            if score > best.1 {
                best = ((y[0].time - window[0].time) * 1000.0, score);
            }
        }
        results.push(best);
    }
    results
}

pub(super) fn correlate(reference: &[Frame], cable: &[Frame]) -> Vec<(f64, f64)> {
    correlate_at_rate(reference, cable, 6000, 10.0_f64.powf(-50.0 / 20.0))
}

pub(super) fn envelope(mut source: Consumer<Frame>) -> Vec<Frame> {
    let mut result = Vec::new();
    while source.slots() >= 96 {
        let first = source.pop().unwrap();
        let mut power = (first.value as f64).powi(2);
        for _ in 1..96 {
            power += (source.pop().unwrap().value as f64).powi(2);
        }
        result.push(Frame {
            time: first.time,
            value: (power / 96.0).sqrt() as f32,
        });
    }
    result
}

pub(super) fn processed_reference(
    mut source: Consumer<Frame>,
    filters: &[FilterConfig],
) -> Consumer<Frame> {
    let mut chain = Chain::new(filters).unwrap();
    let delay = chain.latency as f64 / 48000.0;
    let (mut producer, consumer) = RingBuffer::new(source.slots() + 1);
    let mut input = [0.0; BLOCK];
    let mut output = [0.0; BLOCK];
    let mut times = [0.0; BLOCK];
    while source.slots() > 0 {
        let count = source.slots().min(BLOCK);
        for i in 0..count {
            let frame = source.pop().unwrap();
            input[i] = frame.value;
            times[i] = frame.time;
        }
        chain.process(&input[..count], &mut output[..count]);
        for i in 0..count {
            // Attribute predicted output to its source time, retaining algorithmic delay in the measurement.
            assert!(producer
                .push(Frame {
                    time: times[i] - delay,
                    value: output[i]
                })
                .is_ok());
        }
    }
    consumer
}

pub(super) fn default_filter(kind: &str, id: &str) -> FilterConfig {
    let definitions: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../src/app/src/filters.json")).unwrap();
    let definition = definitions
        .iter()
        .find(|definition| definition["type"] == kind)
        .unwrap_or_else(|| panic!("missing {kind} filter definition"));
    let params = definition["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|parameter| {
            (
                parameter["key"].as_str().unwrap().to_owned(),
                parameter["default"].as_f64().unwrap() as f32,
            )
        })
        .collect();
    FilterConfig {
        id: id.into(),
        kind: kind.into(),
        enabled: true,
        params,
        model: None,
    }
}

pub(super) fn run_ai_capture_stage(
    stage: &str,
    expected_bypassed: bool,
    controller: &mut Controller,
    received: &mut Consumer<Frame>,
    errors: &Arc<Mutex<Vec<String>>>,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut frames = 0usize;
    let mut peak = 0.0_f32;
    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        while let Ok(frame) = received.pop() {
            assert!(
                frame.value.is_finite(),
                "{stage}: CABLE capture was non-finite"
            );
            peak = peak.max(frame.value.abs());
            frames += 1;
        }
        let status = controller.status();
        assert!(
            status.running,
            "{stage}: pipeline stopped (error: {:?})",
            status.error.as_deref()
        );
        assert!(
            status.error.is_none(),
            "{stage}: pipeline error: {:?}",
            status.error.as_deref()
        );
        assert_eq!(status.underruns, 0, "{stage}: pipeline underrun");
        assert_eq!(status.overflows, 0, "{stage}: pipeline overflow");
        assert_eq!(status.bypassed, expected_bypassed, "{stage}: bypass state");
        let callback_errors = errors.lock().unwrap().clone();
        assert!(
            callback_errors.is_empty(),
            "{stage}: capture errors {callback_errors:?}"
        );
    }
    while let Ok(frame) = received.pop() {
        assert!(
            frame.value.is_finite(),
            "{stage}: CABLE capture was non-finite"
        );
        peak = peak.max(frame.value.abs());
        frames += 1;
    }
    let status = controller.status();
    assert!(
        status.running,
        "{stage}: pipeline stopped (error: {:?})",
        status.error.as_deref()
    );
    assert!(
        status.error.is_none(),
        "{stage}: pipeline error: {:?}",
        status.error.as_deref()
    );
    assert_eq!(status.underruns, 0, "{stage}: pipeline underrun");
    assert_eq!(status.overflows, 0, "{stage}: pipeline overflow");
    assert_eq!(status.bypassed, expected_bypassed, "{stage}: bypass state");
    let callback_errors = errors.lock().unwrap().clone();
    assert!(
        callback_errors.is_empty(),
        "{stage}: capture errors {callback_errors:?}"
    );
    assert!(frames > 0, "{stage}: no frames reached CABLE Output");
    assert!(peak > 1e-3, "{stage}: CABLE capture peak was only {peak}");
    println!(
        "{}",
        serde_json::json!({"stage":stage,"duration_seconds":3,"cable_frames":frames,"cable_peak":peak,"status":status})
    );
}
