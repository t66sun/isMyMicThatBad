# Handoff

English | [中文](handoff_zh.md)

## 2026-11-06 local optimization build

Replacement executable: `research/audio-acceptance/artifacts/20261106-022300-monitor-headroom/isMyMicThatBad-optimized.exe`. Close the old app, copy this into the existing portable directory as `isMyMicThatBad.exe`, and retain its libraries, resources, `portable.marker` and `data/`. SHA-256: `0dd4236f9d0b47183bf1f0bcb804b86090b38613a64602e73115bff61ba4c682`.

- Female/Ethereal gate thresholds are -60 dB. Reselect a preset to apply it; saved chains retain their values. The measured quiet-speech loss fell from 1,680/1,650 ms to 20 ms each, at the cost of more near-threshold noise. The listener heard no obvious A/B difference, so pitch naturalness improvement is unproven.
- RVC retains the B voice, model, retrieval, context and 200 ms hop. Two CPU threads saved 47–54% worker CPU time in three game-running pairs. The user authorized 160 ms more buffering: 420 ms reply reserve / 620 ms declared latency. Monitor output now starts with silence before capture; the recorded startup underrun fails its regression under the old ordering.
- Final AI qualification: three 90-second starts and a 1,800.03-second silent soak passed, with no application counter growth or lost records. TANCHJIM capture-clock replay to VB-CABLE, 5 ms app buffer, monitoring enabled with physical output zero: p95 836.46 ms, 105/114 matched windows. Actual total CPU/GPU medians were 57%/91%. These are synthetic-load results; final game coexistence and game FPS remain unmeasured.
- Female passed three starts. Ethereal passed two and failed one with six underruns during a 97.56% CPU spike. Keep this saturated-load limit visible; no all-preset long-run pass is claimed. All measured final outputs had zero clipped samples.
- Required gate: 53 Rust tests, 14 editor tests, TypeScript and the optimized build passed; 18 Rust tests are explicitly ignored by that gate. Additional manual onset/tail, controlled-worker, sample-parity, hardware and final live-edit checks are recorded separately. Final executable device enumeration passed. Test audio and workload helpers have all stopped.

[Conditions and measurements](audio-optimization.md#2026-11-06-resource-contention). Raw evidence and the final executable are under `20261106-022300-monitor-headroom/`; prior failed trials stay in their original dated directories. Listening clips remain in `20261105-214600-voice-quality/gate-candidates/` (`female-A.wav`, `female-B.wav`, `ethereal-A.wav`, `ethereal-B.wav`).

## 2026-11-05 local audio build

The local build includes the RVC timing fix and the **AI 清晰人声** preset selected in blind listening. The original six-card microphone route qualified for a 30-minute soak at a 520 ms reserve. The final 460 ms configuration completed a second 30-minute soak using the physical capture clock and silent headphone output. See the [measurement conditions and evidence](audio-optimization.md#2026-11-05-rvc-measurements).

## 2026-10-13 release

2026-10-13, current branch `master`. The user confirmed stability first and accepted latency overruns with honest delivery.

- The final audio changes are only: start output silently, do not consume the queue before ready, and update the clock only with complete input blocks. The original three periods and startup reserve remain; experimental total-queue feedback and gain changes were reverted.
- Real Intel microphone and VB-CABLE: original 176.43 ms, frequency-zero/gain 181.05 ms, Female 221.88 ms, Blown mic 176.70 ms, Broadcast 171.66 ms, Ethereal 218.32 ms. Every scenario had zero application underruns/overflows; Female 60 seconds and 10 seconds of silence passed. Original/gain exceed 150 ms and Female/Ethereal exceed 200 ms; all were delivered under the user's explicit latency approval, and driver startup notifications remain.
- 43 Rust tests, 13 frontend tests, TypeScript and the optimized Windows build passed. Real Chinese/space path model and index import, cancel and persistence passed; the final binary recheck opened the native dialog but desktop automation activation failed, noted in the acceptance record with the same model-selection source test baseline.
- Final exe SHA-256: `e14339c6a3d28047dbfb1ed18f29367544e0b07a88efa52cf25282f361abca12`. The personal six-card chain and device settings were preserved; the temporary Realtek test volume was restored to 0.
- A standalone AI ZIP was produced: `release/v0.4.0/isMyMicThatBad-v0.4.0-portable-x64.zip` (3,783,557,910 bytes), source commit `64e0f3ad79d01dffcf7cd0e674f71ca83dae2164`; SHA-256 `974d223fc43d1d65039c9e385c6fc28720406450d4fb8edcc8edc8abfb5a4076`. Independent extraction migration and package validation passed, packaged CUDA inference passed (131.81 ms per block, non-zero converted audio), and the embedded worker matches the source.
- The AI path was considered passed and delivered directly under the user's explicit 2026-10-13 instruction; no more tests or buffer changes. Existing preview runs passed; a later extracted-package live recheck produced one "AI inference fell behind real-time audio" protection stop, retained as an observation and not described as a long-term stability pass. AI buffering stays at 520 ms.
- Archived: the 30-minute long test, subjective timbre listening, final model dialog recheck and follow-up investigation of the AI protection stop. The user explicitly asked not to run them; handle them when a future task raises them, not as current TODOs.
- GitHub v0.4.0 published the non-AI portable package (6,371,976 bytes) without the AI environment, models or personal settings; verified the archive has no `data/` entries and the executable matches the acceptance SHA-256 above. The large AI package stays local and is not uploaded.
- Directory cleanup: deleted historical releases, duplicate AI extraction/packaging environments, AI download caches and generated Tauri/Rust build directories; kept the current two archives, personal preview settings, one usable AI environment, source, test records and necessary toolchains. The first rebuild generates build caches.
- TODO is cleared. For a new task after delivery, rewrite `TODO.md`.
- Logs: `research/audio-acceptance/artifacts/physical-stable-*-20261013.log`, `final-stable-build-20261013.log`, `delivery-acceptance.json`. Failed experiment logs remain and are not counted as passing evidence.
