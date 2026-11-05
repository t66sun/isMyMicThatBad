use mic_engine::{
    dsp::{Chain, FilterConfig, BLOCK},
    settings::{Preset, Session},
};
use serde_json::json;
use std::{
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

fn read_samples(path: &Path) -> Vec<f32> {
    let bytes = fs::read(path).unwrap();
    assert!(!bytes.is_empty() && bytes.len() % 4 == 0);
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}

#[test]
#[ignore = "relative DSP timing; run in release mode"]
fn neutral_frequency_avoids_the_active_fir_cost() {
    let input = std::array::from_fn::<_, BLOCK, _>(|i| (i as f32 * 0.11).sin() * 0.4);
    let elapsed = |shift| {
        let config = super::config("frequency", &[("shift_hz", shift)]);
        let mut chain = Chain::new(&[config]).unwrap();
        let mut out = [0.; BLOCK];
        let start = Instant::now();
        for _ in 0..2000 {
            chain.process(&input, &mut out);
            std::hint::black_box(&out);
        }
        start.elapsed()
    };
    let neutral = elapsed(0.);
    let active = elapsed(80.);
    println!("neutral {neutral:?}, active {active:?}");
    assert!(neutral * 2 < active, "a settled zero shift should skip the FIR");
}

fn measure(root: &Path, label: &str, filters: &[FilterConfig], signal: &[f32]) {
    let mut chain = Chain::new(filters).unwrap();
    let mut output =
        BufWriter::new(fs::File::create_new(root.join(format!("{label}.f32"))).unwrap());
    let mut times = Vec::new();
    let mut block = [0.; BLOCK];
    for input in signal.chunks(BLOCK) {
        let result = &mut block[..input.len()];
        let start = Instant::now();
        chain.process(input, result);
        times.push(start.elapsed().as_nanos() as u64);
        for &v in result.iter() {
            assert!(v.is_finite());
            output.write_all(&v.to_le_bytes()).unwrap();
        }
    }
    output.flush().unwrap();
    fs::write(
        root.join(format!("{label}.json")),
        json!({"algorithmic_frames":chain.latency,"dsp_ns":times}).to_string(),
    )
    .unwrap();
}

#[test]
#[ignore = "offline release measurement; provide a fresh MIC_TEST_QUALITY_DIR with dry.f32, rvc.f32, session.json and presets.json"]
fn render_matching_inputs_at_each_card() {
    let dir = PathBuf::from(std::env::var_os("MIC_TEST_QUALITY_DIR").unwrap());
    let session: Session =
        serde_json::from_slice(&fs::read(dir.join("session.json")).unwrap()).unwrap();
    let presets: Vec<Preset> =
        serde_json::from_slice(&fs::read(dir.join("presets.json")).unwrap()).unwrap();
    let wet = read_samples(&dir.join("rvc.f32"));
    let dry = read_samples(&dir.join("dry.f32"));
    let filters = &session.chain[1..];
    assert_eq!(session.chain[0].kind, "rvc");
    assert_eq!(
        filters.iter().map(|f| f.kind.as_str()).collect::<Vec<_>>(),
        ["highpass", "distortion", "gain", "bitcrusher", "frequency"]
    );
    for end in 0..=filters.len() {
        measure(&dir, &format!("stage-{end}"), &filters[..end], &wet);
    }
    for (index, filter) in filters.iter().enumerate() {
        let mut ablation = filters.to_vec();
        ablation.remove(index);
        measure(&dir, &format!("omit-{}", filter.kind), &ablation, &wet);
    }
    for (id, label) in [
        ("builtin-female", "female"),
        ("builtin-ethereal", "ethereal"),
    ] {
        let preset = presets.iter().find(|p| p.id == id).unwrap();
        measure(&dir, label, &preset.chain, &dry);
    }
}

#[test]
#[ignore = "offline release measurement; provide fresh dry.f32 and cases.json in MIC_TEST_QUALITY_DIR"]
fn render_voice_cases() {
    let dir = PathBuf::from(std::env::var_os("MIC_TEST_QUALITY_DIR").unwrap());
    let cases: Vec<Preset> =
        serde_json::from_slice(&fs::read(dir.join("cases.json")).unwrap()).unwrap();
    let signal = read_samples(&dir.join("dry.f32"));
    for case in cases {
        measure(&dir, &case.id, &case.chain, &signal);
    }
}

#[test]
#[ignore = "export the actual editor presets and set MIC_TEST_PRESETS_FILE"]
fn speech_presets_preserve_a_soft_onset_and_fading_word() {
    let presets: Vec<Preset> = serde_json::from_slice(
        &fs::read(std::env::var_os("MIC_TEST_PRESETS_FILE").unwrap()).unwrap(),
    )
    .unwrap();
    let input: Vec<f32> = (0..48000)
        .map(|i| {
            let db = -56.0 - 10.0 * ((i as f32 - 28800.0) / 19200.0).max(0.0);
            (std::f32::consts::TAU * 150.0 * i as f32 / 48000.0).sin()
                * 2.0_f32.sqrt()
                * 10.0_f32.powf(db / 20.0)
        })
        .collect();
    for id in ["builtin-female", "builtin-ethereal"] {
        let preset = presets.iter().find(|p| p.id == id).unwrap();
        let gate = preset.chain.iter().find(|c| c.kind == "gate").unwrap();
        let output = super::render(std::slice::from_ref(gate), &input, BLOCK);
        for (start, end) in [(960, 9600), (43200, 48000)] {
            assert!(
                super::rms(&output[start..end]) > super::rms(&input[start..end]) * 0.7,
                "{} loses quiet speech at {} ms",
                preset.name,
                start / 48
            );
        }
    }
}
