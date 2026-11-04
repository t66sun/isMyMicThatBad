use super::*;

pub(super) fn run(replay: Option<Arc<[f32]>>) {
    let root = std::path::PathBuf::from(
        std::env::var_os("MIC_TEST_AI_ROOT")
            .expect("set MIC_TEST_AI_ROOT to the prepared AI runtime"),
    );
    let input_id = std::env::var("MIC_TEST_INPUT_ID").expect("select the capture input explicitly");
    assert!(
        replay.is_some() || input_id.starts_with("loopback:"),
        "select the speaker loopback endpoint"
    );
    let host = cpal::default_host();
    let cable = host
        .input_devices()
        .unwrap()
        .find(|device| {
            device
                .description()
                .unwrap()
                .name()
                .contains("CABLE Output")
        })
        .expect("VB-CABLE capture endpoint");
    let output = host
        .output_devices()
        .unwrap()
        .find(|device| device.description().unwrap().name().contains("CABLE Input"))
        .expect("VB-CABLE render endpoint");
    let errors = Arc::new(Mutex::new(Vec::new()));
    let stimulus = replay
        .is_none()
        .then(|| play_stimulus(Arc::clone(&errors)).expect("set MIC_TEST_STIMULUS_WAV"));

    let mut chain = vec![
        default_filter("highpass", "rvc-highpass"),
        default_filter("rvc", "rvc-default"),
        default_filter("gain", "rvc-gain"),
    ];
    let mut controller = Controller::with_ai(root);
    #[cfg(feature = "acceptance-replay")]
    if let Some(samples) = replay {
        controller
            .replay_capture(samples, mic_engine::trace::Trace::default())
            .unwrap();
    }
    controller
        .start(&Session {
            input_id,
            output_id: output.id().unwrap().to_string(),
            buffer_ms: 5,
            chain: chain.clone(),
            ..Session::default()
        })
        .unwrap();
    let (cable_stream, mut received) = record(&cable, Arc::clone(&errors));
    cable_stream.play().unwrap();

    run_ai_capture_stage("initial", false, &mut controller, &mut received, &errors);

    let params = &mut chain
        .iter_mut()
        .find(|filter| filter.kind == "rvc")
        .unwrap()
        .params;
    params.insert("pitch".into(), 2.0);
    params.insert("formant".into(), 2.5);
    controller.set_chain(&chain).unwrap();
    run_ai_capture_stage("pitch_formant_edit", false, &mut controller, &mut received, &errors);

    chain.swap(0, 1);
    controller.set_chain(&chain).unwrap();
    run_ai_capture_stage(
        "reordered_graph",
        false,
        &mut controller,
        &mut received,
        &errors,
    );

    controller.set_bypassed(true).unwrap();
    run_ai_capture_stage(
        "global_bypass_on",
        true,
        &mut controller,
        &mut received,
        &errors,
    );

    controller.set_bypassed(false).unwrap();
    run_ai_capture_stage(
        "global_bypass_off",
        false,
        &mut controller,
        &mut received,
        &errors,
    );

    chain.retain(|filter| filter.kind != "rvc");
    controller.set_chain(&chain).unwrap();
    run_ai_capture_stage(
        "rvc_removed",
        false,
        &mut controller,
        &mut received,
        &errors,
    );

    controller.stop();
    let status = controller.status();
    assert!(!status.running, "controller remained active after stop");
    assert!(status.error.is_none(), "controller failed while stopping");
    println!("{}", serde_json::json!({"stage":"stopped","status":status}));
    drop(cable_stream);
    drop(stimulus);
}
