use mic_engine::dsp::{Chain, FilterConfig};
use std::collections::BTreeMap;
fn config(kind: &str, params: &[(&str, f32)]) -> FilterConfig {
    FilterConfig {
        id: kind.into(),
        kind: kind.into(),
        enabled: true,
        params: params
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect::<BTreeMap<_, _>>(),
    }
}
#[test]
fn gain_and_empty_chain_obey_audio_interface() {
    let input = vec![0.1; 256];
    let mut output = vec![0.0; 256];
    Chain::new(&[]).unwrap().process(&input, &mut output);
    assert_eq!(input, output);
    Chain::new(&[config("gain", &[("gain_db", -6.0206)])])
        .unwrap()
        .process(&input, &mut output);
    assert!(
        (output[255] - 0.05).abs() < 0.0001,
        "gain must apply declared dB"
    );
}

fn render(configs: &[FilterConfig], input: &[f32], block: usize) -> Vec<f32> {
    let mut chain = Chain::new(configs).unwrap();
    let mut output = vec![0.0; input.len()];
    for (x, y) in input.chunks(block).zip(output.chunks_mut(block)) {
        chain.process(x, y);
    }
    output
}
fn tone(frequency: f32, length: usize) -> Vec<f32> {
    (0..length)
        .map(|i| 0.15 * (std::f32::consts::TAU * frequency * i as f32 / 48000.0).sin())
        .collect()
}
fn magnitude(samples: &[f32], frequency: f32) -> f32 {
    let (mut real, mut imag) = (0.0_f64, 0.0_f64);
    for (i, x) in samples.iter().enumerate() {
        let phase = std::f64::consts::TAU * frequency as f64 * i as f64 / 48000.0;
        real += *x as f64 * phase.cos();
        imag += *x as f64 * phase.sin();
    }
    (2.0 * (real * real + imag * imag).sqrt() / samples.len() as f64) as f32
}
#[test]
fn frequency_shift_is_additive_and_continuous_across_blocks() {
    let input = tone(440.0, 60000);
    for (shift, expected) in [(100.0, 540.0), (-100.0, 340.0)] {
        let configs = [config("frequency", &[("shift_hz", shift)])];
        let output = render(&configs, &input, 256);
        assert!(
            magnitude(&output[12000..], expected) > 0.12,
            "frequency must shift by fixed Hz"
        );
        assert!(
            magnitude(&output[12000..], 440.0) < 0.005,
            "carrier should be suppressed"
        );
        let small = render(&configs, &input, 31);
        assert!(
            output
                .iter()
                .zip(small)
                .all(|(a, b)| (a - b).abs() < 0.000001),
            "block boundaries must preserve phase"
        );
    }
}

#[test]
fn basic_filters_and_order_have_audible_effects() {
    let input = tone(200.0, 60000);
    let high = render(&[config("highpass", &[("cutoff", 2000.0)])], &input, 256);
    assert!(magnitude(&high[12000..], 200.0) < 0.004);
    let bright = tone(8000.0, 60000);
    let low = render(&[config("lowpass", &[("cutoff", 1000.0)])], &bright, 256);
    assert!(magnitude(&low[12000..], 8000.0) < 0.004);
    let drive = config("distortion", &[("drive_db", 24.0), ("mix", 100.0)]);
    let gain = config("gain", &[("gain_db", -12.0)]);
    let mut drive2 = drive.clone();
    drive2.id = "drive2".into();
    let first = render(&[gain.clone(), drive.clone()], &input, 256);
    let second = render(&[drive2, gain], &input, 256);
    assert!(
        first.iter().zip(second).any(|(a, b)| (a - b).abs() > 0.05),
        "order must affect noncommuting filters"
    );
    assert!(first.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
}

#[test]
fn resampling_preserves_voice_frequency_and_tracks_independent_clocks() {
    use mic_engine::clock::{Converter, Drift};
    for rate in [44100, 48000, 96000] {
        let mut resampler = Converter::new(rate, 48000).unwrap();
        let mut input_position = 0;
        let mut rendered = Vec::new();
        for _ in 0..220 {
            let n = resampler.input_frames();
            let input: Vec<f32> = (input_position..input_position + n)
                .map(|i| {
                    0.15 * (std::f64::consts::TAU * 1000.0 * i as f64 / rate as f64).sin() as f32
                })
                .collect();
            input_position += n;
            let mut block = [0.0; 256];
            resampler.process(&input, &mut block).unwrap();
            rendered.extend(block);
        }
        assert!(
            magnitude(&rendered[8320..56320], 1000.0) > 0.14,
            "rate conversion must preserve frequency"
        );
    }
    for ppm in [-200.0, 200.0] {
        let mut drift = Drift::new();
        let mut fill = 480.0_f64;
        for _ in 0..10000 {
            fill += 256.0 * (1.0 + ppm * 0.000001);
            let ratio = drift.update(fill as usize, 736);
            fill -= 256.0 / ratio;
        }
        assert!(
            (fill - 480.0).abs() < 120.0,
            "independent clocks must not drain/overflow the FIFO"
        );
    }
}

#[test]
fn settings_roundtrip_and_corruption_preserve_user_data() {
    use mic_engine::settings::{Session, Store};
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("mic-settings-test-{nonce}"));
    let store = Store::new(directory.clone());
    let session = Session {
        chain: vec![config("gain", &[("gain_db", -6.0)])],
        ..Session::default()
    };
    store.save(&session).unwrap();
    assert_eq!(store.load().unwrap().session.chain, session.chain);
    std::fs::write(directory.join("settings.json"), "broken original data").unwrap();
    let loaded = store.load().unwrap();
    assert!(loaded.warning.is_some());
    assert!(loaded.session.chain.is_empty());
    assert!(std::fs::read_dir(&directory)
        .unwrap()
        .any(|entry| std::fs::read_to_string(entry.unwrap().path())
            .ok()
            .as_deref()
            == Some("broken original data")));
}

#[test]
fn running_graph_smooths_parameters_bypass_and_structure_changes() {
    use mic_engine::graph::{Graph, Runner};
    let input = [0.1; 256];
    let mut output = [0.0; 256];
    let initial = config("gain", &[("gain_db", 0.0)]);
    let mut runner = Runner::new(Graph::new(Chain::new(&[initial]).unwrap()));
    for _ in 0..4 {
        runner.process(&input, &mut output);
    }
    let lower = config("gain", &[("gain_db", -6.0206)]);
    runner.submit(Graph::new(Chain::new(&[lower.clone()]).unwrap()));
    runner.process(&input, &mut output);
    assert!(
        output[0] > 0.099 && output[255] < 0.08,
        "parameters should ramp instead of jumping"
    );
    runner.process(&input, &mut output);
    assert!((output[255] - 0.05).abs() < 0.0001);
    let mut bypass = lower.clone();
    bypass.enabled = false;
    runner.submit(Graph::new(Chain::new(&[bypass]).unwrap()));
    runner.process(&input, &mut output);
    assert!(
        output[0] < 0.051 && output[255] > 0.075,
        "bypass should ramp"
    );
    runner.process(&input, &mut output);
    assert!((output[255] - 0.1).abs() < 0.0001);
    let frequency = config("frequency", &[("shift_hz", 0.0)]);
    runner.submit(Graph::new(Chain::new(&[frequency, lower]).unwrap()));
    for _ in 0..12 {
        runner.process(&input, &mut output);
        assert!(output
            .iter()
            .all(|v| v.is_finite() && *v > 0.049 && *v <= 0.101));
    }
    assert!(!runner.transitioning());
    assert!((output[255] - 0.05).abs() < 0.0001);
}
