use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use mic_engine::{audio::Controller, dsp::FilterConfig, settings::Session};
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
    let config = device.default_input_config().unwrap();
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
    let (mut producer, consumer) = RingBuffer::new(48000 * 16);
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

fn correlate(reference: &[Frame], cable: &[Frame]) -> Vec<(f64, f64)> {
    let mut results = Vec::new();
    for window in reference.windows(512).step_by(2400).skip(5) {
        let mean = window.iter().map(|x| x.value as f64).sum::<f64>() / 512.0;
        let x: Vec<_> = window.iter().map(|x| x.value as f64 - mean).collect();
        let energy = x.iter().map(|x| x * x).sum::<f64>();
        if energy < 1e-16 {
            continue;
        }
        let origin = cable.partition_point(|x| x.time < window[0].time);
        let mut best = (0.0, 0.0);
        // Search beyond the acceptance limit so slower routes cannot pass by clipping.
        for offset in 0..1800 {
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
#[ignore = "uses real microphone and VB-CABLE; stop other audio senders first"]
fn microphone_to_cable_p95_is_under_150ms() {
    let host = cpal::default_host();
    let physical = if let Ok(id) = std::env::var("MIC_TEST_INPUT_ID") {
        host.input_devices()
            .unwrap()
            .find(|d| d.id().unwrap().to_string() == id)
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
        let (reference_stream, reference) = record(&physical, Arc::clone(&errors));
        let (cable_stream, received) = record(&cable, Arc::clone(&errors));
        reference_stream.play().unwrap();
        cable_stream.play().unwrap();
        let mut controller = Controller::default();
        controller
            .start(&Session {
                input_id: physical.id().unwrap().to_string(),
                output_id: output.id().unwrap().to_string(),
                buffer_ms: 5,
                chain,
                ..Session::default()
            })
            .unwrap();
        std::thread::sleep(Duration::from_secs(12));
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
        assert!(p95 <= 150.0, "hardware latency exceeds 150 ms: {p95:.2}");
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
    let results = correlate(&reduce(reference), &reduce(received));
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
