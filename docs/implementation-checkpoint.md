# Implementation checkpoint

Resumed on 2026-10-09. The user authorized VB-CABLE installation and continuing the approved V1 plan. Latest delivery instructions supersede the previous no-first-commit restriction: produce only a portable ZIP, make one initial commit and push it to origin/master. Do not generate an application installer, Git tag or GitHub Release.

## Implemented

- Tauri 2 / React / TypeScript card editor: drag insertion/reorder, keyboard movement, selected-position plus-button insertion, parameters, bypass, duplicate/delete, built-in and user presets, autosave.
- Native CPAL/WASAPI microphone capture, VB-CABLE output, optional independent headphone monitoring, Tauri commands and persistent endpoint IDs.
- Frequency shift in Hz, high-pass, low-pass, gain and distortion. Parameter/bypass ramps, chain warmup/crossfade and smooth delay-compensation changes.
- Shared numeric metadata and a working compiled template with smoothed amount_db; registration instructions in docs/filter-development.md.
- Preallocated sinc resampling and independent-clock drift control. Callbacks wake the DSP worker instead of periodic polling. A later probe measured Rust 1 ms sleep at 1.617 ms; the earlier 16 ms probe used .NET/PowerShell, so it is not a measurement of the Rust worker.
- Validated settings, atomic replacement and backups of corrupted originals. No microphone capture starts automatically.
- Official VB-CABLE installed and Windows restarted; physical and virtual endpoints enumerate. Rust, MSVC, Windows SDK, WebView2 and rustfmt are available.

## Verification and current work

- Five Rust library regressions passed, including recoverable CPAL errors, monitor failure isolation, output starvation recovery, waveform continuity during delayed-filter insertion, and template parameter changes.
- Final combined checks passed: five Rust library tests, six DSP/persistence integration tests, the known-delay calibration self-check, two frontend model tests, TypeScript, frontend production build and debug/custom-protocol build. Hardware latency remains a separate failing acceptance check.
- Browser UI tests passed including selected-position insertion, drag/reorder, keyboard movement, parameters, copy/delete, preset save/reload and desktop/narrow layout. Actual native WebView2 tests passed hardware start/stop/restart, monitoring and live filter changes. The latest executable also passed native UI start/stop/restart, monitoring, live edits and no uncaught JavaScript errors.
- Thirty-minute diagnostic --smoke 1800 completed at 2026-10-09 01:22:57 Asia/Shanghai after 30.07 minutes. Evidence: research/audio-acceptance/artifacts/20261009-005253-soak.jsonl and soak-baseline.json (exact tested executable SHA-256). All 181 progress samples and the final record show running=true, no fatal error, underruns=2, overflows=0 and deviceXruns=2. Counts remained at startup values. The waiting wrapper could not retrieve ExitCode from a Get-Process object; completion was independently verified from all expected samples, the unique final record and empty stderr.
- Source repairs after soak start added smooth delay transitions, useful template parameters, error isolation and an extra sinc priming block. The recorded soak binary precedes these repairs. Latest native UI and short --smoke 10 passed; two startup underruns remain, with no subsequent growth during that short run.
- The user revised V1 acceptance to p95 ≤150 ms. Optimized Intel calibration at 5 ms buffer passed both scenarios: raw p95 137.81 ms and frequency-zero/gain p95 140.26 ms, 26/26 windows each, no app underruns/overflows. A prior 10 ms comparison failed with frequency-zero/gain at 156.46 ms. Cable-only p95 is 56.29 ms; its different time boundary prevents direct subtraction. Optimization follow-up is recorded in docs/audio-optimization.md.
- The control panel initially reported not installed despite working audio. PnP had healthy ROOT\MEDIA\0000 and a duplicate ROOT\MEDIA\0001 with Code 10 / STATUS_DEVICE_BUSY, both signed driver 3.3.1.7. The installed and bundled panel hashes matched. Removed only the failed duplicate as administrator (exit 0), then restarted ROOT\MEDIA\0000 (exit 0). The user confirmed the panel now opens. Driver package and healthy instance were preserved.
- The user confirmed current Latency=3072 samples. Read-only registry inspection confirms 3072 and Internal SR=48000 Hz. Full-path and cable-only tests were rerun. A full 30-minute soak has not been repeated with this setting.
- A connected TANCHJIM-OLA II DSP became the default input but yielded an inconclusive near-silent correlation run. Test selection now uses the default physical input or an explicit MIC_TEST_INPUT_ID. Controlled measurements keep the previous Intel microphone without modifying system defaults.
- Fixed 10 ms host output buffering did not reduce latency; a one-period app reserve introduced 19 debug and 16 optimized underruns. Both experiments were reverted. Keep the two-period reserve. New sessions default to the qualified 5 ms buffer; existing preferences are preserved.
- Calibration now searches 300 ms and self-checks both 47 and 200 ms delays. Earlier search ranges clipped at 150 ms cannot support acceptance at that same threshold.
- Final optimized portable binary SHA-256 is 2d18cf3df4b6ffb8c912c76f11ec4fe9f102f1e78ffbfdc2c8f888448714aaef. Isolated portable WebView2 UI verification passed at the qualified 5 ms buffer, with zero application underruns/overflows. The user explicitly requested skipping stability testing and delivering immediately. Stopped the test pipeline; progress through 580 seconds had zero underruns/overflows and one unchanged startup driver Xrun. The resulting stopped-stream assertion is an intentional cancellation. Do not claim a completed 30-minute soak for this build.
- Computer Use node_repl initialization failed twice with windows sandbox setup-refresh errors. No direct PowerShell UI Automation fallback was used. The user is operating the vendor panel. Keep it open for them.
## Delivery boundary

The prior packaging rejection was based on the former no-first-commit exception. The user has now explicitly authorized the initial commit, portable ZIP and push, and waived the full stability check. Build, regular checks, hardware latency and same-binary UI verification passed. VERSION is 0.1.0 for the initial delivery. Preserve the user's simplified AGENTS.md. The package manifest records the actual commit and the stability waiver; distribution scripts create no installer/tag/Release.

## Resume commands

Dot-source scripts/Use-Toolchain.ps1 for the project-local Rust environment (research/build-environment/artifacts/cargo and rustup).

```powershell
./scripts/Build.ps1 -ChecksOnly
npm.cmd --prefix src/app run build
cargo build --manifest-path src/app/src-tauri/Cargo.toml --features custom-protocol
./src/app/src-tauri/target/debug/ismymicthatbad.exe --devices
./src/app/src-tauri/target/debug/ismymicthatbad.exe --smoke 10
cargo test --release --manifest-path src/app/src-tauri/Cargo.toml --test hardware_latency microphone_to_cable_p95_is_under_150ms -- --ignored --nocapture
```

Stop test app processes before rebuilding on Windows; running executables are locked. Vite ignores src-tauri/** to avoid watching locked Rust outputs. Native UI tests require a deliberately launched WebView2 debugging port 9222 and portable.marker in target/debug; they restore the saved session and stop audio afterward. Browser plugin is absent, so existing Playwright/Edge tests are used.
