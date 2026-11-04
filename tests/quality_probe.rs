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
