# Implementation checkpoint

Current milestone: v0.3.0 portable delivery, 2026-10-10. The user explicitly accepted the female preset's one additional underrun (“记录已知问题并交付”). Functional and latency delivery criteria passed with that accepted limitation. The release manifest under release/v0.3.0 records actual packaging completion and the commit; no push, tag, application installer or GitHub Release is requested. Earlier v0.1/v0.2 deliveries remain intact.

## v0.3 implementation

- Catalog: 11 filters. Adds noise gate, formant shift, three-band EQ, compressor, pitch shift and mono Schroeder reverb. Distortion adds clipping, default 0 preserves the legacy effect. Seven built-in presets include 女声, 炸麦, 广播声 and 空灵声. All parameters remain in the inspector; summaries show at most two values. The single-row library has accessible navigation buttons.
- Local native effects are in src/app/src-tauri/src/effects.rs. Signalsmith Stretch wrapper is pinned to 0.1.3; 2048-frame window / 64-frame hop. Adjacent pitch/formant cards share analysis and one 2048-frame delay, retaining independent smoothed controls/bypass. Zero shifts use aligned dry audio. Compensation covers 64 ×2048 frames. A short initial 1024/256 window gave a 530.5 Hz peak for 440 Hz +3 semitones; final accuracy is checked within 10 cents on that case.
- CPAL realtime scheduling is enabled; the DSP worker uses audio_thread_priority and releases its scheduling handle on exit. No timer, registry, driver or buffer reserve was changed. DSP-only headroom measured four seconds of female/blown-mic/broadcast/ethereal in 0.3229/0.0149/0.0089/0.4315 seconds.
- Legacy distortion sessions and saved presets add only missing clip=0. Native loading validates before copying the original settings into a timestamped pre-v0.3 backup; failed backup prevents migration/autosave. Invalid values remain rejected.
- libclang 18.1.1 is located in the ignored local toolchain by Use-Toolchain.ps1. The wrapper's incorrectly forwarded formant-base setter is not called. Native sources, manifests/lock/config and all external code remain excluded from Git under the owner's explicit policy.

## Validation and accepted limitations

- Passed: 5 Rust library tests, 17 DSP/settings integrations, 3 delay calibration tests; 5 frontend model tests; TypeScript; production frontend and optimized Windows build. The opt-in DSP budget test also passed. These counts exclude ignored hardware tests from normal test totals.
- Production Edge/Playwright UI checks passed for all 11 cards/new parameters, all 7 presets/reload, navigation, drag/drop/cancel, keyboard/menu alternatives, copies, insertion, reduced motion/focus and layouts 1100×760, 900×640, 390×844. Browser plugin unavailable; production preview uses no HMR or WebSocket.
- Same-binary isolated portable WebView2 checks passed over loopback HTTP WebDriver: persistence/presets, validation, actual routing, start/stop/restart/monitoring, live drag/bypass and window controls. Drag-frame p95 11.3 ms over 190 samples. Short UI observations record startup gaps rather than claiming zero-underrun stability.
- Delivery application SHA-256: c9df91bd20d28a570a9cc11c5ec75fa80109c9ed497087df177cd053cdc0c348. The verified isolated copy under research/build-environment/artifacts/native-ui-v030 was restored to target/release before packaging. A standalone Cargo hardware-test build produces a different main executable; do not substitute it without native UI acceptance.
- Hardware: Intel microphone, VB-CABLE 3072 samples / 48000 Hz, 5 ms app buffer, monitoring off. Raw/frequency-zero-gain p95 138.66/141.91 ms (25/26 and 26/26 matches). Female/blown-mic/broadcast/ethereal p95 180.96/149.31/136.57/188.21 ms (27/27, 28/28, 24/24, 27/28 matches). All latency targets passed; no fatal errors or overflows.
- Female underruns increased 3→4 during 14 seconds. Other new presets did not grow (4/3/2 respectively). The original capture process failed its then-active no-growth assertion. The user explicitly accepted the one event; captured latency/coverage/error/overflow data were revalidated under that delivery scope. The original failed exit and logs remain preserved. The test now records underruns as diagnostics; zero-growth qualification is deferred to the stability milestone.
- Raw waveform timing cannot compare transformed voices reliably. New preset measurements predict the reference through the same DSP and shift predicted timestamps back by declared algorithmic delay, retaining hardware plus DSP time in the result. 2 ms RMS-envelope matching searches 0..300 ms, excludes reference RMS below -50 dB, requires >=20 matches and >=80% coverage. Calibration preserves 47/200/250 ms hardware plus 42.67 ms DSP, and independently checks the amplitude response within 8 ms. Audio stays in memory; no recording is saved. Earlier poor-signal/raw-envelope attempts are diagnostic failures, not passing evidence.
- Full 30-minute stability testing remains explicitly waived. Manual voice-naturalness assessment is not complete. These results exclude chat-app buffering, other devices and arbitrary long chains.

## Delivery

Acceptance is recorded in research/audio-acceptance/artifacts/delivery-acceptance.json as passed_with_accepted_limitations, matching version 0.3.0 and the verified binary. Package.ps1 requires a clean first-party source commit, matching version/hash and required ZIP contents. It records tracked source fingerprints plus local native source/manifests separately. Unmodified Microsoft x64 msvcp140/vcruntime140/vcruntime140_1 DLLs are placed beside the executable; vendor licensing and nested Signalsmith MIT notices are included. No user settings are packaged. Only explicit first-party paths are staged; research, all Tauri directories, dependencies and releases remain excluded.

The frontend test runtime's sandbox user-info API failed before running tests; the same frontend tests passed when rerun outside that sandbox. WASAPI initialization likewise requires the authorized hardware execution context. No source error was attributed to those sandbox failures.

Owned measurement/native UI windows were closed; the production preview and WebDriver are stopped. No additional microphone capture is required for this accepted delivery. The GitHub connector previously returned 403 for issue creation; ticket publication remains blocked on issue-write access. First-party source is locally validated; public checkout still cannot build Windows without the retained local backend.
## v0.2 implementation (historical)

- Unified custom titlebar, native window controls, preset menu and settings sheet.
- Persistent top filter library, horizontal left-to-right chain, fixed shared parameter inspector.
- Pointer dragging starts at 6 px, uses a gentle lift and 180 ms drop, and previews neighboring displacement. A valid release commits once; Escape/outside release cancels without changing session/DSP.
- Card bypass remains visible; more-menu holds duplicate/delete/previous/next. Clicking insertion positions and Alt+left/right (plus old up/down aliases) provide alternatives. Space/Enter remain ordinary button activation.
- Local wheel-to-horizontal and edge auto-scroll, adjacent selection after deletion, empty inspector, reduced-motion support.
- Existing five filters, compiled custom template, native audio commands, session schema, endpoint IDs and settings validation preserved. Selection/drag state is not persisted.
- Version metadata is 0.2.0. Builds use no-bundle; delivery remains portable ZIP only.

## v0.2 verification (historical)

- Rust: five library regressions, six DSP/settings integrations and known-delay calibration passed; two hardware tests are opt-in.
- Frontend model tests, TypeScript and production build passed.
- Browser checks use installed Edge with Playwright (Browser plugin unavailable), production preview at 127.0.0.1:1420, and screenshots outside the repo. Drag preview/commit/cancellation, middle insertion, buttons/menus/keyboard, presets/reload, wheel/edge scroll and reduced motion are checked. Layouts: 1100×760, 900×640 and 390×844.
- Real portable WebView2 checks use matching Edge WebDriver over local HTTP. Native persistence, validation, actual microphone/CABLE routing, audio start/stop/restart, headphone monitoring, drag preview/drop during processing, bypass and maximize/restore/minimize were exercised. The test restores the isolated session, stops audio and closes its window.
- Hardware regression at the qualified 5 ms buffer passed: raw p95 147.13 ms; frequency-zero/-6 dB gain p95 146.58 ms; 26/26 correlated windows each, zero app underruns/overflows and one startup driver Xrun. The first near-silent run was inconclusive and was repeated with user-provided microphone sound.
- Full 30-minute stability testing remains explicitly waived. No claim of long-term stability for v0.2; see research/audio-acceptance/README.md for historical v0.1 evidence and current scope.
- The public repository has no existing issue ticket for this milestone. Creating the tracking issue through the GitHub connector returned HTTP 403 (Resource not accessible by integration). The implementation behavior is documented in docs/design/ui-v0.2.md; publishing the ticket remains pending issue-write access.

## Delivery and resume

Current AGENTS.md requires a local commit after applicable validation. On 2026-10-10, the owner authorized reorganizing unpublished commits and pushing first-party source. `research/` and the entire Tauri backend are now excluded from tracking while their local files are preserved. Published history remains unchanged; the complete previous local v0.2 commits are retained on `codex/backup-before-source-only-20261010`. A fresh GitHub checkout cannot build the Windows application without the locally retained backend. The previously delivered portable ZIP remains intact and still records its original tested commit. No new tag or GitHub Release is requested. Package.ps1 requires a clean committed source tree and acceptance matching the tested executable SHA-256.

Dot-source scripts/Use-Toolchain.ps1 for the existing project-local Rust toolchain. Build.ps1 runs normal checks and creates the optimized no-bundle executable. Stop owned test windows before rebuilding, since Windows locks running executable files. Test native UI from an isolated portable copy; its settings must not affect the user's application profile. Keep the working VB-CABLE driver/control panel intact (3072 samples, 48000 Hz).

The current no-WebSocket instruction is authoritative. Native UI automation uses HTTP WebDriver. The historical native-soak.mjs CDP harness requires explicit authorization before reuse. Screenshot/log directories and release binaries remain ignored; reference artwork is documentation only.
