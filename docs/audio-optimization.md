# Audio latency follow-up

English | [中文](audio-optimization_zh.md)

Qualified targets are p95 ≤150 ms for the raw/frequency-zero-gain calibration and ≤200 ms for the four new presets. The earlier 120 ms and 80 ms goals remain deferred. Current measurements and limitations are in [the user guide](user-guide.md#devices-and-buffering). For v0.4.0, the user explicitly accepted exceeding these latency targets in favor of stable audio; the current measurements are not a latency-target pass.

## 2026-11-05 RVC measurements

On the RTX 2050 with the Saisho model and index, four of five monitored starts of the saved six-card chain previously stopped when the AI output queue ran dry. With prewarmed CUDA graphs, all five physical starts delivered converted speech. Four delivered at least 90 seconds from confirmed output, and one delivered 89.14 seconds. A separate 1,800.05-second run on the TANCHJIM-OLA II DSP microphone and headphones, VB-CABLE and a 5 ms app buffer had no steady counter growth. The p95 from the acoustic microphone reference to CABLE Output was 733.31 ms with the 520 ms reserve.

The final configuration collects 200 ms of input and retains 260 ms for the worker reply. With the same devices, monitoring and app buffer, the clear voice preset ran for 1,800.04 seconds using the TANCHJIM capture clock, a known input waveform and silent headphone output. Input-to-CABLE p95 was 656.44 ms across 106 matched windows out of 113. Application underruns and overflows stayed at zero. One startup driver notification did not increase. The 400 ms candidate stopped on a late first reply and was rejected.

Matched 91-second starts with the clear voice preset measured p95 of **721.13 ms** at 520 ms and **661.10 ms** at 460 ms. The matches were 207/219 and 204/219 windows. Five final starts each delivered at least 90 seconds of confirmed output. Another start reached 89.38 seconds after an early CABLE threshold and is recorded separately.

The listener chose the clear voice at matched loudness. On the same RVC output, the RMS difference from the clean high-pass reference fell from 63.61% to 14.74%. Cold formant changes now prepare their synthesis lengths at model load, adding about six seconds in the isolated probe. Reopening the same voice during a card reorder took 6.5 ms, down from 248.6 ms. Live editing, reordering, global bypass and removing the AI card all delivered audio to CABLE Output.

Raw settings, 2 ms match windows, per-block timing, counter histories and rejected runs are in `research/audio-acceptance/artifacts/20261105-000235-voice-fix/`. The original incidents are in `research/audio-acceptance/artifacts/20261104-224546-six-card/`.

## 2026-11-05 quiet-speech controls

Same-input offline controls isolated the Female and Ethereal presets' -50 dB gate as the first source of quiet-speech loss. Against the same preset without its gate, attenuation exceeding 6 dB for at least 20 ms totaled 1,680 ms for Female and 1,650 ms for Ethereal. Lowering only the threshold to -60 dB reduced both to 20 ms and removed the one-second cold soft-onset silence. Labels were fixed from the input before processing; outputs were aligned by declared DSP delay. Pitch/formant targets and gate timing stay unchanged. The exported-preset onset/tail regression fails with the old presets and passes with the new ones.

The noise cost is explicit: stationary -65 dB noise still closes the gate, while -60 to -55 dB noise can pass. At -55 dB input noise, settled output measured -57.65 dB for Female and -62.57 dB for Ethereal, where the old presets were silent. The two 15.75-second listening pairs used one fixed gain per clip at -23 LUFS. The user heard no obvious difference in either pair; improved naturalness has not been demonstrated. Evidence is in `research/audio-acceptance/artifacts/20261105-214600-voice-quality/` (`controls`, `gate-candidates`, `noise-controls`, and the recorded listening response).

## Constraints to preserve

- Qualified configuration: Intel microphone, VB-CABLE 3072 samples / 48 kHz, 5 ms app buffer, monitoring off. Cable-only p95 measured 56.29 ms; this boundary differs from the full path, so subtraction cannot isolate app latency.
- A fixed 10 ms WASAPI output-buffer request did not reduce full-path latency and was reverted. Reducing output reserve from two device periods to one caused 19 debug and 16 optimized-run underruns and was reverted. Current source reserves three nominal output periods, bounded by queue capacity, and one additional period during startup; actual WASAPI callbacks can exceed the nominal period. Requalify latency before reducing this reserve.
- CPAL realtime scheduling and DSP-worker priority are enabled. Callbacks wake the worker. Offline DSP headroom does not establish callback deadlines or end-to-end latency.
- Transformed voices require the same-preset processed reference with source timestamps minus declared DSP latency. Direct raw-waveform comparison is unreliable; quiet or low-correlation runs are not passing evidence.

## Current stability regression

Output queues now hold whole DSP blocks, and monitoring warmup leaves room for the sinc input history. Inaudible samples below 1e-20 are replaced with zero before output resampling: an optimized three-second tiny-tail probe fell from approximately 0.372 s to 0.007 s without changing audible samples or sample timing. Shared-preset DSP headroom tests also cover quiet tails.

With the qualified microphone/cable configuration above, three consecutive optimized 10-second female-preset starts passed with continuous delivery to a CABLE Output reader and zero application underruns/overflows. The subsequent run was stopped at the user's request after the last 1230-second status record, also with zero application queue counters; it is not a completed 30-minute pass. Recoverable driver notifications remained observable. Full-path p95 was not remeasured, and the extra reserve may increase latency.

## Deferred work

| Priority | Investigation | Evidence required before keeping a change |
| --- | --- | --- |
| 1 | Requalify latency and full-duration stability after the female-preset reserve and quiet-tail fixes; investigate driver notifications separately from application starvation. | Matched hardware/configuration before-and-after p95 and counters; 30-minute stability without counter growth after startup. |
| 2 | Tune VB-CABLE against its actual buffer statistics, preserving 48 kHz and a rollback setting. | Full-path correlation passes without additional driver glitches. |
| 3 | Reduce application buffering only after explaining why a smaller reserve starves. | Stable FIFO fill and callback deadlines under monitoring, live edits and CPU load; no sample dropping to meet the target. |
| 4 | Evaluate Windows low-latency shared-mode periods only if the backend is the measured limiting factor. | Demonstrated host-buffer contribution and compatibility with microphone, cable and monitor devices. |
| 5 | Profile sinc conversions and long chains before changing resampling quality or filter length. | CPU savings, frequency response, independent-clock stability and audible comparisons. |

Full stability and manual voice-naturalness assessment remain deferred. Hardware results exclude chat-app buffering, other devices and arbitrary 64-card chains. Preserve local raw measurements and completed release packages as evidence.

The v0.4.0 release starts render with silence before capture and retains the three-period FIFO and additional startup reserve. Readers wait for priming, and input-clock correction samples only complete input blocks. Final physical calibration, 60-second female-preset playback and 10-second silence checks passed with zero application underruns/overflows. Full 30-minute and subjective checks remain deferred.

## Fixed-chain RVC incident capture

Use an optimized build and the saved six-card session. In the project toolchain shell, build the observer with `cargo test --release --features custom-protocol --manifest-path src/app/src-tauri/Cargo.toml --test hardware_latency --no-run`. Pass its printed test executable to the runner. Create a new dated evidence directory for each campaign.

```powershell
./tests/Run-ChainEvidence.ps1 -TestExe $testExe -SessionPath $session -AiRoot $preparedAi -EvidenceDirectory $evidence -StimulusWav $speech -SpeakerId $physicalSpeaker -CableReaderId $cableOutput -Scenario six-on -Repeats 5 -Seconds 90
```

Use `six-off`, `rvc` and `dsp` for the matched controls and `digital` for speaker loopback. The stimulus is audible. The runner stages a private runtime in the generated build target directory. Each trial owns its settings, first-fault status, GPU log when available, per-block timing, pre-clamp peaks and endpoint energy measurements. Microphone audio stays in memory. Compare timing from the same route, levels, buffer and monitoring state. `tests/summarize_chain_evidence.py` condenses the raw runs. In the app, failure status retains the last queue counters until a new start or explicit stop. The preceding worker session log is available in `ai/worker.previous.log`.

For a quiet run, build the same observer with `--features custom-protocol,acceptance-replay`. Add `-SilentReplay` to the command above and omit `-SpeakerId`. This mode replaces microphone samples with the WAV at the real 48 kHz capture callback. It records the capture clock and CABLE Output independently. The headphone renderer processes monitoring normally and writes zero to the physical endpoint. Its internal signal level is available in the trace. Use the audible mode when measuring room acoustics or the headphone endpoint itself.

For offline quality comparisons, use `tests/chain_quality.py prepare` with a local 48 kHz mono PCM16 speech WAV. Run `tests/ai_inference.py` with the new stimulus, its printed duration, `--output-directory` and `--float-output`. Copy the six-card session and the four exported presets to the quality directory as `session.json` and `presets.json`. Point `MIC_TEST_QUALITY_DIR` there, run the ignored release audio-pipeline test `quality_probe::render_matching_inputs_at_each_card`, then run `tests/chain_quality.py summarize`. The summary identifies the peak-changing stage and measured loudness of the shuffled listening clips. Keep listener ratings separate from the objective measurements.
