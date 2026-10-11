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

#[test]
fn tremolo_controls_rate_depth_and_preserves_phase_across_blocks() {
    let input = vec![0.2; 48000];
    for rate in [0.1, 5.0, 20.0] {
        for depth in [0.0, 50.0, 100.0] {
            let configs = [config("tremolo", &[("rate_hz", rate), ("depth", depth)])];
            let output = render(&configs, &input, 256);
            let small = render(&configs, &input, 31);
            assert_eq!(output, small, "LFO phase survives block boundaries");
            for (i, value) in output.iter().enumerate() {
                let phase = std::f64::consts::TAU * rate as f64 * i as f64 / 48000.0;
                let expected = 0.2 * (1.0 - depth / 100.0 * (0.5 - 0.5 * phase.cos() as f32));
                assert!(
                    (value - expected).abs() < 1e-6,
                    "rate {rate}, depth {depth}, sample {i}"
                );
                assert!(value.is_finite() && (0.0..=0.2).contains(value));
            }
        }
    }
    let mut configs = vec![config("tremolo", &[("rate_hz", 5.0), ("depth", 100.0)])];
    let mut chain = Chain::new(&configs).unwrap();
    assert_eq!(chain.latency, 0);
    let input = [0.2; 256];
    let mut output = [0.0; 256];
    for _ in 0..18 {
        chain.process(&input, &mut output);
    }
    let previous = output[255];
    configs[0].params.insert("depth".into(), 0.0);
    chain.update_from(&Chain::new(&configs).unwrap());
    chain.process(&input, &mut output);
    assert!(
        (output[0] - previous).abs() < 0.001,
        "parameter edits must be smoothed"
    );
    chain.process(&input, &mut output);
    assert!((output[255] - 0.2).abs() < 1e-6);
    configs[0].enabled = false;
    configs[0].params.insert("depth".into(), 100.0);
    chain.update_from(&Chain::new(&configs).unwrap());
    for _ in 0..4 {
        chain.process(&input, &mut output);
    }
    assert_eq!(output, input, "bypass remains dry");
    let mut second = configs[0].clone();
    second.id = "second".into();
    second.enabled = true;
    configs[0].enabled = true;
    let single = render(&configs, &vec![0.2; 9600], 256);
    let paired = render(&[configs[0].clone(), second], &vec![0.2; 9600], 256);
    for (a, b) in single.iter().zip(paired) {
        assert!((b - a * a / 0.2).abs() < 1e-6);
    }
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
    let drive = config(
        "distortion",
        &[("drive_db", 24.0), ("mix", 100.0), ("clip", 0.0)],
    );
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
        close_to_tray: true,
        chain: vec![config("gain", &[("gain_db", -6.0)])],
        ..Session::default()
    };
    store.save(&session).unwrap();
    assert_eq!(store.load().unwrap().session.chain, session.chain);
    assert!(store.load().unwrap().session.close_to_tray);
    let mut legacy = serde_json::to_value(&session).unwrap();
    legacy.as_object_mut().unwrap().remove("closeToTray");
    let restored: Session = serde_json::from_value(legacy.clone()).unwrap();
    assert!(!restored.close_to_tray);
    for value in [
        serde_json::json!(null),
        serde_json::json!(1),
        serde_json::json!("true"),
    ] {
        legacy["closeToTray"] = value;
        assert!(serde_json::from_value::<Session>(legacy.clone()).is_err());
    }
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

fn defaults(kind: &str, overrides: &[(&str, f32)]) -> FilterConfig {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../src/app/src/filters.json")).unwrap();
    let definition = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["type"] == kind)
        .unwrap();
    let mut result = config(kind, &[]);
    for parameter in definition["parameters"].as_array().unwrap() {
        result.params.insert(
            parameter["key"].as_str().unwrap().into(),
            parameter["default"].as_f64().unwrap() as f32,
        );
    }
    for (key, value) in overrides {
        result.params.insert((*key).into(), *value);
    }
    result
}
fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt() as f32
}
#[test]
fn gate_closes_noise_holds_pauses_and_reopens_voice() {
    let mut input = vec![0.001; 24000];
    input.extend(vec![0.1; 24000]);
    input.extend(vec![0.001; 72000]);
    input.extend(vec![0.1; 24000]);
    let output = render(&[defaults("gate", &[])], &input, 256);
    assert!(rms(&output[..24000]) < 1e-8);
    assert!(rms(&output[36000..48000]) > 0.099);
    assert!(
        rms(&output[48000..48960]) > 0.0009,
        "hold should preserve short pauses"
    );
    assert!(
        rms(&output[108000..120000]) < 1e-7,
        "release should close background noise"
    );
    assert!(rms(&output[132000..]) > 0.099);
}
#[test]
fn compressor_obeys_ratio_knee_and_makeup() {
    let input = vec![0.5; 96000];
    let output = render(&[defaults("compressor", &[])], &input, 256);
    let input_db = 20.0 * 0.5_f32.log10();
    let expected = 10.0_f32.powf((-18.0 + (input_db + 18.0) / 3.0) / 20.0);
    assert!((rms(&output[48000..]) - expected).abs() < 0.002);
    let unity = render(&[defaults("compressor", &[("ratio", 1.0)])], &input, 31);
    assert!(unity.iter().zip(&input).all(|(a, b)| (a - b).abs() < 1e-6));
    let makeup = render(
        &[defaults("compressor", &[("makeup_db", 6.0)])],
        &input,
        256,
    );
    assert!(
        (rms(&makeup[48000..]) / rms(&output[48000..]) - 10.0_f32.powf(6.0 / 20.0)).abs() < 0.01
    );
}
#[test]
fn equalizer_is_flat_by_default_and_shapes_all_three_bands() {
    let input: Vec<_> = (0..96000)
        .map(|i| {
            [80.0, 1200.0, 12000.0]
                .iter()
                .map(|f| 0.05 * (std::f32::consts::TAU * f * i as f32 / 48000.0).sin())
                .sum()
        })
        .collect();
    let flat = render(&[defaults("eq", &[])], &input, 256);
    assert!(flat.iter().zip(&input).all(|(a, b)| (a - b).abs() < 1e-5));
    let changed = render(
        &[defaults(
            "eq",
            &[
                ("low_gain_db", 6.0),
                ("mid_gain_db", -6.0),
                ("high_gain_db", -6.0),
            ],
        )],
        &input,
        31,
    );
    assert!(magnitude(&changed[24000..], 80.0) > 0.08);
    assert!(magnitude(&changed[24000..], 1200.0) < 0.03);
    assert!(magnitude(&changed[24000..], 12000.0) < 0.03);
}
#[test]
fn pitch_transposes_without_changing_duration_or_block_continuity() {
    let input = tone(440.0, 96000);
    let cfg = defaults("pitch", &[("semitones", 3.0)]);
    let output = render(&[cfg.clone()], &input, 256);
    let shifted = 440.0 * 2.0_f32.powf(3.0 / 12.0);
    // A short-window phase vocoder is approximate; measure its peak, not one exact Fourier bin.
    let peak = (-30_i32..=30)
        .map(|c| {
            let frequency = shifted * 2.0_f32.powf(c as f32 / 1200.0);
            (c, magnitude(&output[24000..], frequency))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    assert!(
        peak.0.abs() <= 10 && peak.1 > 0.08,
        "pitch error/amplitude: {peak:?}"
    );
    assert!(magnitude(&output[24000..], 440.0) < 0.015);
    assert_eq!(input.len(), output.len());
    let small = render(&[cfg], &input, 31);
    assert!((rms(&small[24000..]) - rms(&output[24000..])).abs() < 0.002);
    assert!(small.windows(2).all(|v| (v[1] - v[0]).abs() < 0.1));
}
#[test]
fn formant_moves_spectral_envelope_without_transposing_harmonics() {
    let input: Vec<_> = (0..96000)
        .map(|i| {
            (1..=80)
                .map(|harmonic| {
                    let frequency = 120.0 * harmonic as f32;
                    let envelope = (-0.5 * ((frequency - 720.0) / 100.0).powi(2)).exp()
                        + 0.8 * (-0.5 * ((frequency - 1320.0) / 160.0).powi(2)).exp();
                    (0.02 + envelope) / harmonic as f32
                        * 0.08
                        * (std::f32::consts::TAU * frequency * i as f32 / 48000.0).sin()
                })
                .sum()
        })
        .collect();
    let neutral = render(&[defaults("formant", &[])], &input, 256);
    let shifted = render(&[defaults("formant", &[("semitones", 3.0)])], &input, 256);
    let old_ratio = magnitude(&neutral[24000..], 960.0) / magnitude(&neutral[24000..], 720.0);
    let new_ratio = magnitude(&shifted[24000..], 960.0) / magnitude(&shifted[24000..], 720.0);
    assert!(
        new_ratio > old_ratio * 1.5,
        "spectral envelope must move: {old_ratio} -> {new_ratio}"
    );
    let harmonic_energy = (1..=30)
        .map(|n| magnitude(&shifted[24000..], 120.0 * n as f32).powi(2))
        .sum::<f32>();
    let offgrid_energy = (1..=30)
        .map(|n| magnitude(&shifted[24000..], 120.0 * n as f32 + 60.0).powi(2))
        .sum::<f32>();
    assert!(
        harmonic_energy > offgrid_energy * 100.0,
        "original harmonic spacing must remain"
    );
}
#[test]
fn reverb_has_a_decaying_tail_and_zero_mix_is_dry() {
    let mut impulse = vec![0.0; 144000];
    impulse[0] = 0.5;
    let output = render(
        &[defaults("reverb", &[("room", 70.0), ("mix", 100.0)])],
        &impulse,
        31,
    );
    assert!(rms(&output[1500..12000]) > 0.0001);
    assert!(rms(&output[120000..]) < rms(&output[1500..12000]) * 0.01);
    assert!(output.iter().all(|v| v.is_finite()));
    let dry = render(&[defaults("reverb", &[("mix", 0.0)])], &impulse, 256);
    assert_eq!(dry, impulse);
}
#[test]
fn clipping_zero_preserves_soft_distortion_and_full_clipping_limits_peaks() {
    let input: Vec<_> = (0..48000)
        .map(|i| (i as f32 / 47999.0 * 4.0) - 2.0)
        .collect();
    let soft = render(
        &[defaults(
            "distortion",
            &[("drive_db", 18.0), ("mix", 100.0), ("clip", 0.0)],
        )],
        &input,
        256,
    );
    let drive = 10.0_f32.powf(18.0 / 20.0);
    assert!(soft
        .iter()
        .zip(&input)
        .all(|(y, x)| (*y - (*x * drive).tanh() / drive.tanh()).abs() < 1e-6));
    let hard = render(
        &[defaults(
            "distortion",
            &[("drive_db", 18.0), ("mix", 100.0), ("clip", 100.0)],
        )],
        &input,
        31,
    );
    assert!(hard
        .iter()
        .zip(&input)
        .all(|(y, x)| (*y - (*x * drive).clamp(-1.0, 1.0)).abs() < 1e-6));
}
#[test]
fn legacy_distortion_migrates_session_and_preset_and_preserves_original() {
    use mic_engine::settings::{Preset, Session, Store};
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("mic-migration-{nonce}"));
    std::fs::create_dir_all(&directory).unwrap();
    let old = config("distortion", &[("drive_db", 6.0), ("mix", 25.0)]);
    let session = Session {
        chain: vec![old.clone()],
        presets: vec![Preset {
            id: "user-preset".into(),
            name: "旧预设".into(),
            chain: vec![old],
        }],
        ..Session::default()
    };
    let raw = serde_json::to_string_pretty(&session).unwrap();
    let path = directory.join("settings.json");
    std::fs::write(&path, &raw).unwrap();
    let store = Store::new(directory.clone());
    let loaded = store.load().unwrap();
    assert!(loaded.warning.is_some());
    assert_eq!(loaded.session.chain[0].params["clip"], 0.0);
    assert_eq!(loaded.session.presets[0].chain[0].params["clip"], 0.0);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
    let backups: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("settings.pre-v0.3-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read_to_string(backups[0].path()).unwrap(), raw);
    store.save(&loaded.session).unwrap();
    assert!(store.load().unwrap().warning.is_none());
    let mut bad = loaded.session;
    bad.chain[0].params.insert("clip".into(), 101.0);
    assert!(store.save(&bad).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn spectral_chain_delay_can_exceed_the_old_compensation_capacity() {
    use mic_engine::graph::{Graph, Runner};
    let filters: Vec<_> = (0..9)
        .map(|i| {
            let mut node = defaults("pitch", &[]);
            node.id = format!("pitch-{i}");
            node
        })
        .collect();
    let chain = Chain::new(&filters).unwrap();
    assert!(chain.latency > 64 * 127);
    let mut runner = Runner::new(Graph::new(Chain::new(&[]).unwrap()));
    runner.submit(Graph::new(chain));
    for _ in 0..128 {
        let mut output = [0.0; 256];
        runner.process(&[0.0; 256], &mut output);
        assert!(output.iter().all(|v| v.is_finite()));
    }
    assert!(!runner.transitioning());
    runner.submit(Graph::new(Chain::new(&[]).unwrap()));
    for _ in 0..128 {
        let mut output = [0.0; 256];
        runner.process(&[0.0; 256], &mut output);
    }
    assert!(!runner.transitioning());
}
#[test]
fn every_new_filter_retains_state_during_parameter_and_bypass_updates() {
    use mic_engine::graph::{Graph, Runner};
    for kind in ["gate", "eq", "compressor", "pitch", "formant", "reverb"] {
        let initial = defaults(kind, &[]);
        let mut changed = initial.clone();
        let key = match kind {
            "gate" => "threshold_db",
            "eq" => "mid_gain_db",
            "compressor" => "ratio",
            "pitch" | "formant" => "semitones",
            _ => "room",
        };
        let value = match kind {
            "gate" => -60.0,
            "eq" => 6.0,
            "compressor" => 6.0,
            "pitch" | "formant" => 2.0,
            _ => 70.0,
        };
        changed.params.insert(key.into(), value);
        let mut runner = Runner::new(Graph::new(Chain::new(&[initial]).unwrap()));
        let input = tone(220.0, 128 * 256);
        for block in 0..128 {
            if block == 32 {
                runner.submit(Graph::new(Chain::new(&[changed.clone()]).unwrap()));
            }
            if block == 64 {
                changed.enabled = false;
                runner.submit(Graph::new(Chain::new(&[changed.clone()]).unwrap()));
            }
            if block == 96 {
                changed.enabled = true;
                runner.submit(Graph::new(Chain::new(&[changed.clone()]).unwrap()));
            }
            let mut output = [0.0; 256];
            runner.process(&input[block * 256..(block + 1) * 256], &mut output);
            assert!(
                output.iter().all(|v| v.is_finite() && v.abs() < 2.0),
                "{kind} live edit generated invalid output"
            );
            assert!(
                !runner.transitioning(),
                "parameter/bypass updates must reuse the running processor"
            );
        }
    }
}

#[test]
fn adjacent_voice_cards_share_latency_and_keep_independent_live_bypass() {
    let configs = vec![
        defaults("pitch", &[("semitones", 3.0)]),
        defaults("formant", &[("semitones", 2.0)]),
    ];
    let mut chain = Chain::new(&configs).unwrap();
    assert_eq!(chain.latency, 2048);
    let input = tone(440.0, 96000);
    for (pitch_enabled, formant_enabled) in
        [(true, false), (false, true), (false, false), (true, true)]
    {
        let mut next = configs.clone();
        next[0].enabled = pitch_enabled;
        next[1].enabled = formant_enabled;
        next[1].params.insert("semitones".into(), 3.0);
        chain.update_from(&Chain::new(&next).unwrap());
        let mut output = vec![0.0; input.len()];
        for (x, y) in input.chunks(256).zip(output.chunks_mut(256)) {
            chain.process(x, y);
        }
        assert!(output.iter().all(|v| v.is_finite()));
        if !pitch_enabled {
            assert!(magnitude(&output[24000..], 440.0) > 0.05);
            if !formant_enabled {
                assert!(output[24000..]
                    .iter()
                    .zip(&input[24000 - 2048..input.len() - 2048])
                    .all(|(a, b)| (a - b).abs() < 0.0001));
            }
        } else {
            assert!(magnitude(&output[24000..], 440.0) < 0.015);
        }
    }
}

#[test]
#[ignore = "machine-dependent DSP budget; export shared presets and set MIC_TEST_PRESETS_FILE"]
fn shared_presets_have_realtime_processing_headroom() {
    #[derive(serde::Deserialize)]
    struct Preset {
        name: String,
        chain: Vec<FilterConfig>,
    }
    let presets: Vec<Preset> = serde_json::from_str(
        &std::fs::read_to_string(
            std::env::var("MIC_TEST_PRESETS_FILE").expect("Export shared presets first"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(presets.len(), 4);
    let input: Vec<_> = (0..48000 * 4)
        .map(|i| {
            let phase = std::f32::consts::TAU * 120.0 * i as f32 / 48000.0;
            (phase.sin() + 0.5 * (2.0 * phase).sin() + 0.25 * (6.0 * phase).sin()) * 0.1
        })
        .collect();
    for preset in presets {
        let start = std::time::Instant::now();
        let output = render(&preset.chain, &input, 256);
        let seconds = start.elapsed().as_secs_f64();
        println!(
            "{}: four seconds of audio processed in {seconds:.4} s",
            preset.name
        );
        assert!(output.iter().all(|v| v.is_finite()));
        assert!(rms(&output[48000..]) > 0.001);
        assert!(
            seconds < 2.0,
            "DSP must leave at least half the real-time budget free"
        );
        let mut chain = Chain::new(&preset.chain).unwrap();
        let mut output = [0.0; 256];
        for block in input.chunks(256) {
            chain.process(block, &mut output);
        }
        for _ in 0..2000 {
            chain.process(&[0.0; 256], &mut output);
        }
        let start = std::time::Instant::now();
        for _ in 0..750 {
            chain.process(&[0.0; 256], &mut output);
            std::hint::black_box(&output);
        }
        let seconds = start.elapsed().as_secs_f64();
        println!(
            "{}: four seconds of quiet tail processed in {seconds:.4} s",
            preset.name
        );
        assert!(output.iter().all(|v| v.is_finite()));
        assert!(
            seconds < 2.0,
            "quiet tails must also leave half the real-time budget free"
        );
    }
}
