use super::*;
use crate::dsp::BLOCK;
use std::io::BufRead;

const ROOT_ENV: &str = "MIC_TEST_RVC_REPLY_GATE";
const READY: &[u8] = b"CONTROLLED_RVC_READY";

fn wait_for(check: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !check() {
        assert!(
            Instant::now() < deadline,
            "controlled worker did not reach its barrier"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
#[ignore = "private child of the controlled pipe test"]
fn controlled_worker() {
    let Some(root) = std::env::var_os(ROOT_ENV).map(PathBuf::from) else {
        return;
    };
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    output.write_all(READY).unwrap();
    output.write_all(b"\n").unwrap();
    output.flush().unwrap();
    let mut audio = vec![0; HOP * 4];
    loop {
        let mut line = String::new();
        if input.read_line(&mut line).unwrap() == 0 {
            return;
        }
        let request: Value = serde_json::from_str(&line).unwrap();
        match request["op"].as_str().unwrap() {
            "open" => writeln!(output, "{}", json!({"ok":true,"frames":HOP})).unwrap(),
            "process" => {
                input.read_exact(&mut audio).unwrap();
                fs::write(root.join("arrived"), []).unwrap();
                wait_for(|| root.join("release").exists());
                for sample in audio.chunks_exact_mut(4) {
                    sample.copy_from_slice(&0.5_f32.to_le_bytes());
                }
                writeln!(
                    output,
                    "{}",
                    json!({"ok":true,"frames":HOP,"milliseconds":1})
                )
                .unwrap();
                output.write_all(&audio).unwrap();
            }
            "close" => {
                writeln!(output, "{}", json!({"ok":true})).unwrap();
                output.flush().unwrap();
                return;
            }
            op => panic!("unexpected operation {op}"),
        }
        output.flush().unwrap();
    }
}

fn service_with_gate(root: &Path) -> Arc<Service> {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "ai::bridge_tests::controlled_worker",
            "--nocapture",
        ])
        .env(ROOT_ENV, root)
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut wire = Wire {
        input: child.stdin.take().unwrap(),
        output: child.stdout.take().unwrap(),
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut line = Vec::new();
    loop {
        let mut byte = [0];
        read_pipe(&mut wire.output, &mut byte, deadline).unwrap();
        if byte[0] == b'\n' {
            if line.ends_with(READY) {
                break;
            }
            line.clear();
        } else {
            line.push(byte[0]);
        }
    }
    Arc::new(Service::new(child, wire))
}

#[test]
fn delayed_pipe_reply_on_either_side_of_the_playable_reserve() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    for late in [false, true] {
        let root =
            std::env::temp_dir().join(format!("mic-reply-{}-{stamp}-{late}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let config = FilterConfig {
            id: "probe".into(),
            kind: "rvc".into(),
            enabled: true,
            model: None,
            params: KEYS.iter().map(|key| (key.to_string(), 0.)).collect(),
        };
        let mut filter = Rvc::open(service_with_gate(&root), &root, &config, None).unwrap();
        let mut block = [0.; BLOCK];
        for _ in 0..LATENCY / BLOCK {
            filter.process(&[0.25; BLOCK], &mut block);
        }
        wait_for(|| root.join("arrived").exists());
        if late {
            filter.process(&[0.25; BLOCK], &mut block);
        }
        fs::write(root.join("release"), []).unwrap();
        wait_for(|| filter.output.slots() >= HOP);
        filter.process(&[0.25; BLOCK], &mut block);
        if late {
            filter.state.fail(WorkerError::disconnected());
            assert_eq!(filter.failure().unwrap().kind(), "ai_output_empty");
            assert_eq!(block, [0.; BLOCK]);
        } else {
            assert!(filter.failure().is_none());
            assert!(block[..LATENCY % BLOCK].iter().all(|&x| x == 0.));
            assert!(block[LATENCY % BLOCK..].iter().all(|&x| x == 0.5));
        }
        drop(filter);
        fs::remove_file(root.join("arrived")).unwrap();
        fs::remove_file(root.join("release")).unwrap();
        fs::remove_dir(root).unwrap();
    }
}

#[test]
fn game_inference_spike_fits_the_playable_reserve() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("mic-game-spike-{}-{stamp}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let config = FilterConfig {
        id: "spike".into(),
        kind: "rvc".into(),
        enabled: true,
        model: None,
        params: KEYS.iter().map(|key| (key.to_string(), 0.)).collect(),
    };
    let mut filter = Rvc::open(service_with_gate(&root), &root, &config, None).unwrap();
    let mut block = [0.; BLOCK];
    // Advance the audio clock through 200 ms collection plus the measured
    // 370 ms game-contention reply. A barrier keeps wall-clock jitter out.
    let consumed = (HOP + 370 * 48) / BLOCK * BLOCK;
    for _ in 0..consumed / BLOCK {
        filter.process(&[0.25; BLOCK], &mut block);
    }
    wait_for(|| root.join("arrived").exists());
    let before_reply = filter.failure();
    fs::write(root.join("release"), []).unwrap();
    wait_for(|| filter.output.slots() >= HOP);
    if before_reply.is_none() {
        for _ in 0..LATENCY.saturating_sub(consumed) / BLOCK + 2 {
            filter.process(&[0.25; BLOCK], &mut block);
        }
    }
    let after_reply = filter.failure();
    drop(filter);
    fs::remove_file(root.join("arrived")).unwrap();
    fs::remove_file(root.join("release")).unwrap();
    fs::remove_dir(root).unwrap();
    assert!(before_reply.is_none(), "370 ms reply exhausted the reserve");
    assert!(after_reply.is_none());
    assert_eq!(block, [0.5; BLOCK]);
}
