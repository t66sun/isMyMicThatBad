# Adding a compiled filter

The engine processes mono f32 blocks at 48 kHz. Start with `src/app/src-tauri/src/template.rs`: implement `Filter::process(input, output)` and `set_params`. Declare any algorithm delay with `latency()`.

1. Allocate history and scratch buffers in the constructor. Keep processing free of allocation, blocking locks, disk access and logging.
2. Add the filter type, name, description, color and parameter ranges/defaults to `src/app/src/filters.json`. The Rust validator and frontend share this file.
3. Add the constructor arm to `Chain::new` in `dsp.rs`. The UI generates numeric controls and uses a fallback icon; no new card component is required.
4. Validate changes through the `Chain` interface in `tests/audio_pipeline.rs`. Test a known signal, state across blocks, and parameter transitions. Smooth parameter updates and keep instance state independent.

The template includes a working `amount_db` control with the same 10 ms smoothing as built-in filters. Replace its multiplication with your own algorithm. It is a compiled extension point, not a runtime DLL/script loader, and is not added as a redundant sixth built-in effect.

To register a copy of the template, append this definition to `filters.json`:

```json
{"type":"user-trim","name":"自定义滤镜","description":"可修改的滤镜模板。","color":"#75bfc7","parameters":[{"key":"amount_db","label":"强度","unit":"dB","min":-24,"max":12,"step":0.1,"default":0}]}
```

Then add `"user-trim" => Box::new(crate::template::TemplateFilter::new(config.params["amount_db"]))` to the `Chain::new` constructor match. Rebuild the app; the library card and numeric controls appear from the shared metadata. Keep parameter validation in this metadata and test the processor through `Chain` after registration.

Development: `npm.cmd --prefix src/app run desktop` with Rust/MSVC on PATH. This machine also has a project-local toolchain; dot-source `scripts/Use-Toolchain.ps1`.
Validation: `powershell -File scripts/Build.ps1 -ChecksOnly`.
Portable Windows build: `powershell -File scripts/Build.ps1`. It uses `tauri build --no-bundle` and generates no application installer.
After validating and committing the source, run `powershell -File scripts/Package.ps1`. It requires `research/audio-acceptance/artifacts/delivery-acceptance.json` to identify the tested executable, and writes a portable ZIP, changelog, source checksums and `release.json` under `release/v<version>/`. Runtime-generated files stay outside Git.

Use shared-mode WASAPI via CPAL, rtrb SPSC queues and rubato asynchronous sinc conversion. Main output controls production demand; capture and monitor clocks are adapted independently. Callbacks do not allocate. The DSP worker retires graphs to the control thread and accepts the latest pending edit after a transition completes.

Native checks: `ismymicthatbad.exe --devices`; `ismymicthatbad.exe --smoke 5` requires a physical microphone and installed CABLE Input. It sends actual microphone audio to the cable and performs a live filter edit. `--smoke 1800` runs the planned 30-minute soak. Stop other audio senders before running these checks. UI verification uses tests/native-ui.mjs against a deliberately launched local WebView2 debugging port, not a browser preview; the test restores the saved session and stops audio afterward. Browser plugin is not available in this environment, so the UI checks use the installed Edge through playwright-core.

For the qualified portable build, launch an isolated portable copy with WebView2 debugging at `127.0.0.1:9222`. Run `node tests/native-ui.mjs <screenshot-path> <portable-directory>/data/settings.json`. With `MIC_TEST_INPUT_ID` set, `node tests/native-soak.mjs 1800` then checks the same binary at a 5 ms buffer with frequency +100 Hz/gain -6 dB, monitoring off, and stops audio afterward. It rejects fatal errors, dropped samples or counter growth after the first 10 seconds. No microphone recording is written.

Hardware latency: `cargo test --release --features custom-protocol --manifest-path src/app/src-tauri/Cargo.toml --test hardware_latency microphone_to_cable_p95_is_under_150ms -- --ignored --nocapture`. The user revised the V1 target to p95 ≤150 ms on 2026-10-09. This opt-in test uses WASAPI capture timestamps and signal correlation between the physical microphone and CABLE Output. Both endpoints must use native f32 at 48 kHz. It measures both an empty chain and frequency-zero/gain (including FIR delay), with monitoring off and a 5 ms buffer target; audio stays in memory. Correlation searches up to 300 ms so exceeding the target cannot be hidden by clipping. A silent or uncorrelated run fails as inconclusive rather than passing latency. It does not include a chat application's additional buffering or establish latency for arbitrary long chains.

The test selects the default physical microphone. For repeatable device comparisons, set `MIC_TEST_INPUT_ID` to an exact ID from `ismymicthatbad.exe --devices`; it changes only test selection, not Windows defaults or saved app settings. Run the virtual-only `measure_cable_without_microphone_or_dsp` test separately, never concurrently with the microphone test.

The user authorized one initial commit and a push to origin/master for this delivery. Package generation creates no Git tag or GitHub Release. The application needs installed VB-CABLE and WebView2 even in portable mode.
