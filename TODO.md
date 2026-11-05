# Voice quality and resource use

English | [中文](docs/TODO_zh.md)

- [x] Locate the first attenuation of soft speech and word endings in the Female and Ethereal presets using aligned, same-input controls: the -50 dB gate suppresses quiet speech.
- [x] Repair the confirmed cause while preserving pitch/formant targets. The -60 dB candidate passes the exported-preset onset/tail regression; the user heard no obvious A/B difference, so naturalness improvement remains unproven.
- [ ] Compare resource candidates while Probably Stolen keeps running, as the user requested. Record actual game/CPU/GPU activity; synthetic calibration and game frame-rate claims are outside this revised run.
- [ ] Qualify the selected resource change in three paired short runs and a 30-minute silent run with the current game, with unchanged AI quality and at most 20 ms extra p95 latency.
- [ ] Pass required checks, build the executable and deliver the measurements, listening clips and remaining limitations.
