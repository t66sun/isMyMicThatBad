# Voice quality and resource use

English | [中文](docs/TODO_zh.md)

- [ ] Publish v0.4.1 with the pending optimization commits and the complete portable AI runtime after the build, exact-binary acceptance and extracted-package gates pass.
- [x] Locate the first attenuation of soft speech and word endings in the Female and Ethereal presets using aligned, same-input controls: the -50 dB gate suppresses quiet speech.
- [x] Repair the confirmed cause while preserving pitch/formant targets. The -60 dB candidate passes the exported-preset onset/tail regression; the user heard no obvious A/B difference, so naturalness improvement remains unproven.
- [x] Compare resource candidates while Probably Stolen keeps running. Two CPU threads save 47–54% process CPU time in three pairs, but the 260 ms reply reserve still fails native short runs. Higher-priority CUDA execution and pitch overlap were rejected.
- [x] Qualify the authorized 160 ms additional reserve (420 ms reply budget): three paired short comparisons and the final AI-only 1,800.03-second silent soak passed. Final p95 was 836.46 ms; no application counter growth or trace loss. Actual fixed-job utilization is recorded separately from its initial calibration.
- [x] Recheck Female after sustained warmup: the final three 90-second starts passed. Preserve the earlier 22-underrun heavier-profile failure; this is not a Female 30-minute pass.
- [x] Repair monitor startup ordering. The first soak failed with one monitor underrun; the captured schedule fails with the old ordering and passes when both renderers start before capture. Three AI starts and the final AI soak passed after the repair.
- [x] Pass required checks and final live-edit regression, build and smoke-test the replacement executable, and record measurements, listening clips and remaining limitations in the handoff.
- [ ] Investigate the ordinary-preset saturated-load boundary if supporting near-100% system CPU is required: the final Ethereal campaign passed two starts and failed one with six underruns during a 97.56% CPU spike. Scheduling delay versus DSP CPU cost remains unisolated.
- [ ] Recheck the final candidate with Probably Stolen running again. The game exited before the authorized reserve increase; game FPS and final game-coexistence stability have not been measured.
