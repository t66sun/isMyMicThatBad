use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use mic_engine::{
    audio::Controller,
    dsp::{Chain, FilterConfig, BLOCK},
    settings::Session,
};
use rtrb::{Consumer, RingBuffer};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Copy)]
struct Frame {
    time: f64,
    value: f32,
}

fn record(
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

fn wav_samples(bytes: &[u8]) -> Vec<f32> {
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

fn play_stimulus(errors: Arc<Mutex<Vec<String>>>) -> Option<cpal::Stream> {
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

#[test]
fn stimulus_wav_reads_pcm_and_rejects_truncation_or_wrong_format() {
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend(42_u32.to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16_u32.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(48000_u32.to_le_bytes());
    wav.extend(96000_u32.to_le_bytes());
    wav.extend(2_u16.to_le_bytes());
    wav.extend(16_u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(6_u32.to_le_bytes());
    for value in [32767_i16, 0, -32768] {
        wav.extend(value.to_le_bytes());
    }
    assert_eq!(wav_samples(&wav), [32767.0 / 32768.0, 0.0, -1.0]);
    assert!(std::panic::catch_unwind(|| wav_samples(&wav[..wav.len() - 1])).is_err());
    wav[22] = 2;
    assert!(std::panic::catch_unwind(|| wav_samples(&wav)).is_err());
}

fn reduce(mut source: Consumer<Frame>) -> Vec<Frame> {
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

fn correlate_at_rate(
    reference: &[Frame],
    cable: &[Frame],
    rate: usize,
    minimum_rms: f64,
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
        // Search beyond the acceptance limit so slower routes cannot pass by clipping.
        for offset in 0..rate * 3 / 10 {
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

fn correlate(reference: &[Frame], cable: &[Frame]) -> Vec<(f64, f64)> {
    correlate_at_rate(reference, cable, 6000, 10.0_f64.powf(-50.0 / 20.0))
}
fn envelope(mut source: Consumer<Frame>) -> Vec<Frame> {
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

fn processed_reference(mut source: Consumer<Frame>, filters: &[FilterConfig]) -> Consumer<Frame> {
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

#[test]
fn processed_reference_retains_algorithmic_and_hardware_delay() {
    fn queue(frames: &[Frame]) -> Consumer<Frame> {
        let (mut producer, consumer) = RingBuffer::new(frames.len() + 1);
        for frame in frames {
            assert!(producer.push(*frame).is_ok());
        }
        consumer
    }
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../src/app/src/filters.json")).unwrap();
    let filters: Vec<_> = [("pitch", 5.0), ("formant", 2.5)]
        .into_iter()
        .map(|(kind, shift)| {
            let definition = catalog
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["type"] == kind)
                .unwrap();
            let mut params: std::collections::BTreeMap<String, f32> = definition["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    (
                        p["key"].as_str().unwrap().into(),
                        p["default"].as_f64().unwrap() as f32,
                    )
                })
                .collect();
            params.insert("semitones".into(), shift);
            FilterConfig {
                id: kind.into(),
                kind: kind.into(),
                enabled: true,
                params,
            }
        })
        .collect();
    let mut seed = 17_u32;
    let mut level = 0.1;
    let input: Vec<_> = (0..48000 * 6)
        .map(|i| {
            if i % 7200 == 0 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                level = 0.03 + 0.15 * seed as f32 / u32::MAX as f32;
            }
            let phase = std::f32::consts::TAU * 120.0 * i as f32 / 48000.0;
            Frame {
                time: i as f64 / 48000.0,
                value: level
                    * (phase.sin() + 0.5 * (6.0 * phase).sin() + 0.25 * (11.0 * phase).sin()),
            }
        })
        .collect();
    let mut chain = Chain::new(&filters).unwrap();
    let mut processed = Vec::new();
    for frames in input.chunks(BLOCK) {
        let x: Vec<_> = frames.iter().map(|f| f.value).collect();
        let mut y = vec![0.0; x.len()];
        chain.process(&x, &mut y);
        processed.extend(frames.iter().zip(y).map(|(f, value)| Frame {
            time: f.time,
            value,
        }));
    }
    let predicted = envelope(processed_reference(queue(&input), &filters));
    for hardware_ms in [47.0, 200.0, 250.0] {
        let received: Vec<_> = processed
            .iter()
            .map(|f| Frame {
                time: f.time + hardware_ms / 1000.0,
                value: f.value,
            })
            .collect();
        let results = correlate_at_rate(
            &predicted,
            &envelope(queue(&received)),
            500,
            10.0_f64.powf(-50.0 / 20.0),
        );
        let expected = hardware_ms + chain.latency as f64 / 48.0;
        assert!(results.len() >= 5);
        assert!(
            results
                .iter()
                .all(|(delay, score)| (*delay - expected).abs() <= 2.0 && *score > 0.95),
            "{results:?}, expected {expected}"
        );
        if hardware_ms == 47.0 {
            let direct = correlate_at_rate(
                &envelope(queue(&input)),
                &envelope(queue(&received)),
                500,
                10.0_f64.powf(-50.0 / 20.0),
            );
            assert!(
                direct
                    .iter()
                    .all(|(delay, _)| (*delay - expected).abs() <= 8.0),
                "declared delay must agree with the amplitude-envelope response: {direct:?}"
            );
        }
    }
}

#[test]
fn correlation_finds_known_delay() {
    let mut seed = 17_u32;
    let reference: Vec<_> = (0..24000)
        .map(|i| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            Frame {
                time: i as f64 / 6000.0,
                value: seed as f32 / u32::MAX as f32 - 0.5,
            }
        })
        .collect();
    for expected_ms in [47.0, 200.0] {
        let cable: Vec<_> = reference
            .iter()
            .map(|x| Frame {
                time: x.time + expected_ms / 1000.0,
                value: x.value * 0.5,
            })
            .collect();
        let results = correlate(&reference, &cable);
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .all(|(delay, score)| (*delay - expected_ms).abs() < 0.2 && *score > 0.99));
    }
}

#[test]
fn microphone_correlation_rejects_quiet_matched_input() {
    let reference: Vec<_> = (0..15600)
        .map(|i| Frame {
            time: i as f64 / 6000.0,
            value: (i as f32 * 0.1).sin() * 1e-5,
        })
        .collect();
    let cable: Vec<_> = reference
        .iter()
        .map(|frame| Frame {
            time: frame.time + 0.047,
            value: frame.value,
        })
        .collect();
    assert!(!correlate_at_rate(&reference, &cable, 6000, 0.0).is_empty());
    assert!(
        correlate(&reference, &cable).is_empty(),
        "near-silent input cannot qualify microphone latency"
    );
}

#[test]
#[ignore = "uses real microphone and VB-CABLE; stop other audio senders first"]
fn microphone_to_cable_p95_is_under_150ms() {
    measure_input_latency(150.0);
}

#[test]
#[ignore = "plays WAV through a WASAPI loopback source and VB-CABLE"]
fn wav_loopback_to_cable_p95_is_under_200ms() {
    assert!(std::env::var("MIC_TEST_INPUT_ID")
        .unwrap()
        .starts_with("loopback:"));
    measure_input_latency(200.0);
}

fn measure_input_latency(budget_ms: f64) {
    let host = cpal::default_host();
    let physical = if let Ok(id) = std::env::var("MIC_TEST_INPUT_ID") {
        host.devices()
            .unwrap()
            .find(|d| d.id().unwrap().to_string() == id.strip_prefix("loopback:").unwrap_or(&id))
            .expect("requested microphone is unavailable")
    } else {
        host.default_input_device()
            .filter(|d| !d.description().unwrap().name().contains("CABLE"))
            .or_else(|| {
                host.input_devices()
                    .unwrap()
                    .find(|d| !d.description().unwrap().name().contains("CABLE"))
            })
            .expect("physical microphone")
    };
    let cable = host
        .input_devices()
        .unwrap()
        .find(|d| d.description().unwrap().name().contains("CABLE Output"))
        .expect("VB-CABLE capture");
    let output = host
        .output_devices()
        .unwrap()
        .find(|d| d.description().unwrap().name().contains("CABLE Input"))
        .expect("VB-CABLE render");
    if let Ok(input_id) = std::env::var("MIC_TEST_INPUT_ID") {
        if input_id.starts_with("loopback:") {
            let mut controller = Controller::default();
            let error = controller
                .start(&Session {
                    input_id,
                    output_id: physical.id().unwrap().to_string(),
                    ..Session::default()
                })
                .unwrap_err();
            assert!(error.contains("不能将输出回送到自身"));
            assert!(!controller.status().running);
        }
    }
    for chain in [
        vec![],
        vec![
            FilterConfig {
                id: "latency-frequency".into(),
                kind: "frequency".into(),
                enabled: true,
                params: [("shift_hz".into(), 0.0)].into_iter().collect(),
            },
            FilterConfig {
                id: "latency-gain".into(),
                kind: "gain".into(),
                enabled: true,
                params: [("gain_db".into(), -6.0)].into_iter().collect(),
            },
        ],
    ] {
        let scenario = if chain.is_empty() {
            "raw"
        } else {
            "frequency-zero-and-gain"
        };
        let errors = Arc::new(Mutex::new(Vec::new()));
        let _stimulus = play_stimulus(Arc::clone(&errors));
        let (reference_stream, reference) = record(&physical, Arc::clone(&errors));
        let (cable_stream, received) = record(&cable, Arc::clone(&errors));
        reference_stream.play().unwrap();
        cable_stream.play().unwrap();
        let mut controller = Controller::default();
        controller
            .start(&Session {
                input_id: std::env::var("MIC_TEST_INPUT_ID")
                    .unwrap_or_else(|_| physical.id().unwrap().to_string()),
                output_id: output.id().unwrap().to_string(),
                buffer_ms: 5,
                chain,
                ..Session::default()
            })
            .unwrap();
        std::thread::sleep(Duration::from_secs(16));
        let status = controller.status();
        controller.stop();
        drop(reference_stream);
        drop(cable_stream);
        assert!(
            errors.lock().unwrap().is_empty(),
            "capture errors: {:?}",
            errors.lock().unwrap()
        );
        assert!(status.running && status.error.is_none());
        let reference = reduce(reference);
        let received = reduce(received);
        let results = correlate(&reference, &received);
        let mut good: Vec<_> = results
            .iter()
            .filter(|(_, score)| *score >= 0.5)
            .map(|(delay, _)| *delay)
            .collect();
        good.sort_by(f64::total_cmp);
        println!(
            "{}",
            serde_json::json!({"scenario":scenario,"method":"WASAPI capture timestamps and 6 kHz signal correlation; audio stays in memory","reference":physical.description().unwrap().name(),"output":cable.description().unwrap().name(),"windows":results.len(),"correlated_windows":good.len(),"delays_and_scores":results,"status":status})
        );
        assert!(
            good.len() >= 20 && good.len() * 5 >= results.len() * 4,
            "insufficient correlated signal; this run cannot establish latency"
        );
        let p95 = good[(good.len() * 95).div_ceil(100) - 1];
        println!("Measured p95: {p95:.2} ms");
        assert!(
            p95 <= budget_ms,
            "hardware latency exceeds {budget_ms} ms: {p95:.2}"
        );
    }
}

#[test]
#[ignore = "isolates VB-CABLE latency using a quiet virtual-only test signal"]
fn measure_cable_without_microphone_or_dsp() {
    let host = cpal::default_host();
    let cable = host
        .input_devices()
        .unwrap()
        .find(|d| d.description().unwrap().name().contains("CABLE Output"))
        .unwrap();
    let output = host
        .output_devices()
        .unwrap()
        .find(|d| d.description().unwrap().name().contains("CABLE Input"))
        .unwrap();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let (capture, received) = record(&cable, Arc::clone(&errors));
    let config = output.default_output_config().unwrap();
    assert_eq!(config.sample_format(), cpal::SampleFormat::F32);
    assert_eq!(config.sample_rate(), 48000);
    let channels = config.channels() as usize;
    let (mut producer, reference) = RingBuffer::new(48000 * 16);
    let mut seed = 17_u32;
    let output_errors = Arc::clone(&errors);
    let stream = output
        .build_output_stream::<f32, _, _>(
            config.config(),
            move |data, info| {
                let start = info.timestamp().playback.as_nanos() as f64 / 1e9;
                for (i, frame) in data.chunks_exact_mut(channels).enumerate() {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let value = 0.001 * (seed as f32 / u32::MAX as f32 - 0.5);
                    frame.fill(value);
                    assert!(producer
                        .push(Frame {
                            time: start + i as f64 / 48000.0,
                            value
                        })
                        .is_ok());
                }
            },
            move |error| {
                if error.kind() != cpal::ErrorKind::Xrun {
                    output_errors.lock().unwrap().push(error.to_string());
                }
            },
            Some(Duration::from_secs(2)),
        )
        .unwrap();
    capture.play().unwrap();
    stream.play().unwrap();
    std::thread::sleep(Duration::from_secs(12));
    drop(stream);
    drop(capture);
    assert!(errors.lock().unwrap().is_empty());
    // The cable-only probe deliberately generates a sub-speech-level calibration signal.
    let results = correlate_at_rate(&reduce(reference), &reduce(received), 6000, 0.0);
    let mut good: Vec<_> = results
        .iter()
        .filter(|(_, score)| *score >= 0.5)
        .map(|(delay, _)| *delay)
        .collect();
    good.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"method":"predicted WASAPI render time to CABLE capture; excludes application microphone/DSP/queues","correlated_windows":good.len(),"windows":results.len(),"delays_and_scores":results})
    );
    assert!(
        good.len() >= 20 && good.len() * 5 >= results.len() * 4,
        "inconclusive cable-only correlation"
    );
    println!(
        "Cable-only p95: {:.2} ms",
        good[(good.len() * 95).div_ceil(100) - 1]
    );
}

#[test]
fn envelope_calibration_survives_carrier_and_gain_changes() {
    let mut seed = 17_u32;
    let amplitudes: Vec<_> = (0..2500)
        .map(|i| {
            if i % 47 == 0 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            }
            0.03 + 0.2 * (seed as f64 / u32::MAX as f64)
        })
        .collect();
    fn samples(amplitudes: &[f64], carrier: f64, delay: f64, compressed: bool) -> Consumer<Frame> {
        let (mut producer, consumer) = RingBuffer::new(amplitudes.len() * 96 + 1);
        for (i, amplitude) in amplitudes.iter().enumerate() {
            for j in 0..96 {
                let time = (i * 96 + j) as f64 / 48000.0;
                let level = if compressed {
                    amplitude.sqrt() * 0.1
                } else {
                    *amplitude
                };
                assert!(producer
                    .push(Frame {
                        time: time + delay,
                        value: (level * (std::f64::consts::TAU * carrier * time).sin()) as f32
                    })
                    .is_ok());
            }
        }
        consumer
    }
    // Gates remove quiet background; these windows cannot measure speech latency.
    let quiet = envelope(samples(&vec![0.0001; 2500], 1000.0, 0.0, false));
    let silent = envelope(samples(&vec![0.0; 2500], 1500.0, 0.2, true));
    assert!(correlate_at_rate(&quiet, &silent, 500, 10.0_f64.powf(-50.0 / 20.0)).is_empty());
    let reference = envelope(samples(&amplitudes, 1000.0, 0.0, false));
    for expected_ms in [47.0, 200.0, 250.0] {
        let output = envelope(samples(&amplitudes, 1500.0, expected_ms / 1000.0, true));
        let result = correlate_at_rate(&reference, &output, 500, 10.0_f64.powf(-50.0 / 20.0));
        assert!(result.len() >= 5);
        assert!(result
            .iter()
            .all(|(delay, score)| (delay - expected_ms).abs() <= 2.0 && *score > 0.95));
    }
}

#[test]
#[ignore = "requires audible microphone input and VB-CABLE for four new presets"]
fn new_presets_microphone_to_cable_p95_is_under_200ms() {
    measure_preset_latency(200.0);
}

#[test]
#[ignore = "plays WAV through a WASAPI loopback source and the shared presets"]
fn wav_presets_to_cable_p95_is_under_250ms() {
    assert!(std::env::var("MIC_TEST_INPUT_ID")
        .unwrap()
        .starts_with("loopback:"));
    measure_preset_latency(250.0);
}

fn measure_preset_latency(budget_ms: f64) {
    let host = cpal::default_host();
    let id = std::env::var("MIC_TEST_INPUT_ID").expect("Select the physical microphone explicitly");
    let physical = host
        .devices()
        .unwrap()
        .find(|d| d.id().unwrap().to_string() == id.strip_prefix("loopback:").unwrap_or(&id))
        .expect("selected microphone");
    let cable = host
        .input_devices()
        .unwrap()
        .find(|d| d.description().unwrap().name().contains("CABLE Output"))
        .unwrap();
    let output = host
        .output_devices()
        .unwrap()
        .find(|d| d.description().unwrap().name().contains("CABLE Input"))
        .unwrap();
    #[derive(serde::Deserialize)]
    struct Preset {
        name: String,
        chain: Vec<FilterConfig>,
    }
    let file = std::env::var("MIC_TEST_PRESETS_FILE")
        .expect("Export shared presets with tests/export-presets.ts");
    let presets: Vec<Preset> =
        serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(presets.len(), 4);
    let mut failures = Vec::new();
    for preset in presets {
        let filters = preset.chain.clone();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let _stimulus = play_stimulus(Arc::clone(&errors));
        let (reference_stream, reference) = record(&physical, Arc::clone(&errors));
        let (cable_stream, received) = record(&cable, Arc::clone(&errors));
        reference_stream.play().unwrap();
        cable_stream.play().unwrap();
        let mut controller = Controller::default();
        controller
            .start(&Session {
                input_id: id.clone(),
                output_id: output.id().unwrap().to_string(),
                buffer_ms: 5,
                chain: preset.chain,
                ..Session::default()
            })
            .unwrap();
        std::thread::sleep(Duration::from_secs(2));
        let startup = controller.status();
        std::thread::sleep(Duration::from_secs(12));
        let status = controller.status();
        controller.stop();
        drop(reference_stream);
        drop(cable_stream);
        assert!(errors.lock().unwrap().is_empty());
        assert!(status.running && status.error.is_none());
        let results = correlate_at_rate(
            &envelope(processed_reference(reference, &filters)),
            &envelope(received),
            500,
            10.0_f64.powf(-50.0 / 20.0),
        );
        let mut good: Vec<_> = results
            .iter()
            .filter(|(_, score)| *score >= 0.5)
            .map(|(delay, _)| *delay)
            .collect();
        good.sort_by(f64::total_cmp);
        println!(
            "{}",
            serde_json::json!({"scenario":preset.name,"method":"WASAPI timestamps; same-preset reference DSP with source timestamps minus declared algorithmic delay; 2 ms RMS correlation, reference >= -50 dB, search 0..300 ms; in-memory audio","windows":results.len(),"correlated_windows":good.len(),"delays_and_scores":results,"startup":startup,"status":status})
        );
        if good.len() < 20 || good.len() * 5 < results.len() * 4 {
            failures.push(format!(
                "{}: insufficient matched speech windows ({}/{})",
                preset.name,
                good.len(),
                results.len()
            ));
            continue;
        }
        let p95 = good[(good.len() * 95).div_ceil(100) - 1];
        println!("{} p95: {p95:.2} ms", preset.name);
        if p95 > budget_ms || status.underruns != 0 || status.overflows != 0 {
            failures.push(format!(
                "{}: p95 {p95:.2} ms, underruns {} -> {}, overflows {}",
                preset.name, startup.underruns, status.underruns, status.overflows
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}

#[test]
#[ignore = "uses real microphone and VB-CABLE; stop other audio senders first"]
fn female_preset_runs_without_underruns() {
    let id = std::env::var("MIC_TEST_INPUT_ID").expect("Select the physical microphone explicitly");
    let file = std::env::var("MIC_TEST_PRESETS_FILE").expect("Export shared presets first");
    let presets: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(
        presets[0]["name"], "女声",
        "stability fixture must use the shared female preset"
    );
    let chain: Vec<FilterConfig> = serde_json::from_value(presets[0]["chain"].clone()).unwrap();
    let output = cpal::default_host()
        .output_devices()
        .unwrap()
        .find(|d| d.description().unwrap().name().contains("CABLE Input"))
        .unwrap();
    let seconds: u64 = std::env::var("MIC_TEST_SECONDS")
        .unwrap_or_else(|_| "1800".into())
        .parse()
        .unwrap();
    assert!((10..=1800).contains(&seconds));
    let cable = cpal::default_host()
        .input_devices()
        .unwrap()
        .find(|d| d.description().unwrap().name().contains("CABLE Output"))
        .unwrap();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let _stimulus = play_stimulus(Arc::clone(&errors));
    let (cable_stream, mut received) = record(&cable, Arc::clone(&errors));
    cable_stream.play().unwrap();
    let mut controller = Controller::default();
    controller
        .start(&Session {
            input_id: id,
            output_id: output.id().unwrap().to_string(),
            buffer_ms: 5,
            chain,
            ..Session::default()
        })
        .unwrap();
    let mut device_xruns = 0;
    for second in 1..=seconds {
        std::thread::sleep(Duration::from_secs(1));
        let mut frames = 0;
        while received.pop().is_ok() {
            frames += 1;
        }
        assert!(
            frames > 0,
            "virtual microphone must deliver audio to its reader"
        );
        assert!(
            errors.lock().unwrap().is_empty(),
            "virtual microphone capture error"
        );
        let status = controller.status();
        if second == 1
            || second % 30 == 0
            || status.underruns != 0
            || status.overflows != 0
            || status.device_xruns != device_xruns
        {
            println!("{}", serde_json::json!({"second":second,"status":status}));
        }
        assert!(status.running && status.error.is_none());
        assert_eq!(status.underruns, 0, "female preset underrun at {second}s");
        assert_eq!(status.overflows, 0, "female preset overflow at {second}s");
        device_xruns = status.device_xruns;
    }
    controller.stop();
    drop(cable_stream);
}
