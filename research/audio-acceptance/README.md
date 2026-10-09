# Windows audio acceptance, 2026-10-09

The user revised V1 full-path acceptance to **p95 ≤150 ms** on 2026-10-09 and requested a portable ZIP, one initial commit and a push to origin/master. Optimized calibration with a 5 ms application buffer passed both scenarios. Deferred optimization work is recorded in [docs/audio-optimization.md](../../docs/audio-optimization.md).

## Measurements

| Check | Actual result |
| --- | --- |
| Microphone to CABLE, frequency +100 Hz and gain -6 dB, monitoring off, 10 ms target, `--smoke 1800` | Completed 30.07 minutes. All 181 progress records and the final record show no fatal error, overflows=0, underruns=2 and deviceXruns=2. Both nonzero counters stayed at their startup values. |
| Final same-binary native UI | Start/stop/restart, live parameter/bypass/insertion changes, persistence, native validation and headphone monitoring passed. The sampled running status had zero application underruns/overflows and one startup driver Xrun. This is a functional check, not proof of zero glitches on every device. |
| Final portable build stability | **Skipped at the user's explicit request.** Before stopping, 580 seconds of progress showed running=true, underruns=0, overflows=0 and deviceXruns=1 with no growth after startup. This is not a completed 30-minute soak. The test's stopped-stream assertion followed the intentional stop, not an unexpected application failure. |
| Full-path acceptance, optimized build, 5 ms app buffer, driver 3072 samples | **Passes 150 ms:** raw p95 137.81 ms; frequency-zero/gain p95 140.26 ms. Both have 26/26 correlated windows, no application underruns or overflows, and one startup driver Xrun. |
| Earlier optimized comparison, 10 ms app buffer | Raw p95 142.04 ms; frequency-zero/gain p95 156.46 ms failed the revised target. Device/startup variation remains; the qualified default is 5 ms. |
| Cable-only diagnostic, driver 3072 samples | p95 56.29 ms with 25 correlated windows, compared with the previous 64.53 ms. The boundary is CPAL's predicted WASAPI render timestamp to cable capture. It differs from the full-path test, so subtraction does not establish application latency. |
| Rejected buffer experiments | Fixed 10 ms WASAPI render request did not reduce full-path p95 (140.56 ms). One-period application reserve caused 19 underruns in debug and 16 in an optimized run. Both changes were reverted; the stable two-period reserve is retained. |
| Known-delay calibration self-check | Passed: synthetic 47 ms and 200 ms delays identified within 0.2 ms. Correlation searches up to 300 ms, beyond the acceptance limit. Earlier tests limited to 150 ms are not used to claim 150 ms acceptance. |
| Timer probes | PowerShell/.NET 1 ms sleep averaged 16.05 ms; a separate Rust probe averaged 1.617 ms. These are different implementations. The .NET result does not establish the Rust worker's delay. |

Calibration hardware is the Intel Smart Sound microphone array and VB-CABLE 3.3.1.7. The prior soak used Realtek headphones; the final UI check selects the current default non-cable monitor. Audio used in correlation remains in memory; no recordings are stored. The test requires both capture endpoints at native f32/48 kHz and excludes chat-app buffering. Arbitrary 64-filter chains and other microphones are not covered.

Generated logs, executable hashes and the device snapshot are under `artifacts/` and ignored by Git. `soak-baseline.json` identifies the earlier development binary. Final portable binary SHA-256 is `2d18cf3df4b6ffb8c912c76f11ec4fe9f102f1e78ffbfdc2c8f888448714aaef`. It passed real WebView2 persistence, hardware start/stop/restart, live edits, monitoring and native validation. Its full stability acceptance was waived by the user; the earlier soak does not establish 30-minute stability for this binary/settings.

## Driver/control-panel repair

The official control panel incorrectly reported not installed while audio streamed successfully. PnP showed healthy `ROOT\MEDIA\0000` and a same-version duplicate `ROOT\MEDIA\0001` failing Code 10 / STATUS_DEVICE_BUSY. Installed and bundled control-panel files had identical hashes.

Only the failed duplicate was removed through an administrator `pnputil /remove-device` command. The working instance was then restarted with `pnputil /restart-device`; both commands returned 0. The user confirmed the control panel opened afterward. The driver package and working instance remain installed. These commands are separate device-instance operations documented by [Microsoft PnPUtil](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/pnputil-command-syntax).

The user changed current Latency from 7168 to 3072 samples, keeping Internal SR at 48000 Hz. Read-only registry inspection confirmed both values. Correlation was rerun after the change, with results above. The [VB-CABLE reference manual](https://vb-audio.com/Cable/VBCABLE_ReferenceManual.pdf) describes live latency adjustment and sizing based on actual driver buffer statistics.

A newly connected TANCHJIM-OLA II DSP became the default microphone. Its first run had insufficient correlated signal (4 of 26 windows) and was correctly rejected as inconclusive. Controlled comparisons explicitly select the prior Intel microphone through `MIC_TEST_INPUT_ID`; Windows defaults and application settings are unchanged.

## Application repairs

- CPAL Xrun was incorrectly fatal. Recoverable notifications are now counted, and device failures retain their real message. [CPAL error categories](https://docs.rs/cpal/0.18.2/cpal/enum.ErrorKind.html) distinguish these cases.
- Audio callbacks wake the DSP worker; its progress no longer relies on a periodic polling sleep. Output prefill includes device-period demand and sinc history.
- Delay compensation itself crossfades when inserting a delayed filter. A continuous-waveform regression catches the previous jump.
- Monitor errors disable monitoring and retain main output. Settings-load errors preserve existing data.
- Card plus buttons select the insertion position, and device selectors have stable accessible names.

Portable delivery is authorized. The optimized no-bundle build, regular checks, 150 ms calibration and same-binary native UI checks passed. The user explicitly requested skipping the remaining stability test and delivering now. No application installer, Git tag or GitHub Release is requested. `release/v0.1.0/release.json` records the commit, tested binary, stability waiver and package checksums after delivery verification.
