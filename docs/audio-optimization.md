# Audio latency follow-up

English | [中文](audio-optimization_zh.md)

Qualified targets are p95 ≤150 ms for the raw/frequency-zero-gain calibration and ≤200 ms for the four new presets. The earlier 120 ms and 80 ms goals remain deferred. Current measurements and limitations are in [the user guide](user-guide.md#devices-and-buffering). For v0.4.0, the user explicitly accepted exceeding these latency targets in favor of stable audio; the current measurements are not a latency-target pass.

## 2026-11-06 resource contention

The final AI clear-voice chain completed **1,800.03 seconds** under fixed CPU/CUDA jobs with zero application underruns, overflows or lost trace records. The single startup driver notification did not grow. Input-to-CABLE p95 was **836.46 ms**, with 105/114 windows matched at correlation ≥0.5. This used RTX 2050, the TANCHJIM capture clock with injected speech, VB-CABLE, a 5 ms app buffer and enabled monitoring whose physical output was forced to zero. It is synthetic-contention evidence, not a new acoustic measurement or a game soak. Raw results are in `20261106-022300-monitor-headroom/20261106-031123589-six-on-replay-1800s-1/`.

The first final soak was stopped early after one monitor underrun, 386 ms after confirmed converted output. AI still had about 400 ms of queued output and DSP blocks took only tens of microseconds. The monitor stream had started after priming; its first callback consumed five DSP blocks at once, reducing its margin before a later 20 ms producer gap. Both renderers now start before capture, so their initial fills can receive silence before the live queue is ready. Replaying the recorded producer/monitor schedule causes one underrun with the old ordering and none with the new ordering. Queue sizes and declared latency are unchanged. The interrupted trial remains a failure in `20261106-004820-game-reserve/soak-first-failure.json`; the corrected build and qualification are in `20261106-022300-monitor-headroom/`.

GPU memory was unchanged in the thread-budget screening: PyTorch peak allocated memory was 472.86 MiB and reserved memory was 632 MiB for both budgets. These allocator counters exclude other processes and driver allocations.

The final soak used two 2048-square FP32 matrix products per 19 ms and eight CPU hash workers on 100 ms periods. Including audio and other running applications, CPU utilization measured median 57.3% / p95 78.6%, and GPU median 91% / p95 100%; these are higher than the initial medium-load target. Inference p95/p99/max was 147.21/159.40/196.06 ms, with bridge round-trip p95 149.22 ms. Minimum queued AI audio was 10,816 frames (225.3 ms). DSP p99/max was 0.296/2.607 ms, and both outputs had zero clipped samples. Device-wide GPU memory was median 917 MiB / maximum 926 MiB, including the competing workload. Final-binary parameter edits, reordering, bypass and AI removal also passed a separate silent check with monitoring off. All test workers exited.

After the startup repair, Female and AI each passed three 90-second starts. Ethereal passed two but its third run recorded six underruns at about 75 seconds, affecting both outputs. Three consecutive DSP blocks took 25.38, 27.29 and 22.82 ms while total CPU utilization peaked at 97.56%; that run's GPU median was 93%. Thread CPU cycles were not recorded, so scheduling delay and on-CPU DSP cost are not yet isolated. This saturated-load boundary remains failed. Only the separately qualified AI chain proceeds to the final soak; no all-preset stability pass is claimed.

The current worker uses two PyTorch/OpenMP/FAISS CPU threads and keeps the 200 ms input hop, Saisho model and index, retrieval strength, volume-envelope mix and model context. Three paired 30-second probes while Probably Stolen was running reduced worker process CPU time by **53.6%, 47.3% and 46.9%**. This is process CPU time, not whole-machine utilization or game FPS. GPU synthesis spans and competing-job completion times did not improve consistently. Higher CUDA priority and overlapping pitch extraction were rejected; neither is in the delivered worker. A final fixed-seed 30-second probe produced 1,440,000 samples bit-identical to the four-thread baseline.

The old 260 ms reply reserve did not survive game contention: two of four four-thread native trials and all four two-thread trials stopped. The first fault was an empty AI output queue; measured inference spikes reached 369 ms, with roughly 1–3 ms more in the bridge. After the user explicitly allowed 100–200 ms additional latency, the reply reserve became **420 ms**, making declared AI latency **620 ms** including input collection. A controlled-pipe regression survives a 370 ms reply at the new reserve and fails at the old one; exhausted-reserve silence and first-fault checks remain enabled. Sustained inference slower than 200 ms per block can still exhaust the queue.

Three alternating four-/two-thread native pairs at 620 ms all delivered 90 seconds. Under the same fixed background jobs, the old four-thread/460 ms control measured p95 681.16 ms, versus 830.04–841.25 ms for the two-thread/620 ms candidate: **149–160 ms additional latency**. Conditions were RTX 2050, TANCHJIM-OLA II DSP capture clock with injected synthetic speech, VB-CABLE, 5 ms application buffer and monitoring enabled with zero written to the headphones. The reference was the injected input, not an acoustic microphone recording. Every comparison passed the minimum 20 windows, correlation ≥0.5 and match rate ≥80% checks. These runs occurred after the game exited and do not establish game coexistence or FPS.

The initially calibrated background load grew heavier as the GPU clock fell from about 1515 to 735 MHz. Its GPU utilization rose to about 95%; a Female run then recorded **22 underruns** and is retained as a failed boundary test. Ethereal and the expanded-reserve AI short runs survived that profile. The fixed workload and actual sustained utilization must be reported together; a short calibration is not evidence of a constant 60% GPU load.

Evidence: `research/audio-acceptance/artifacts/20261105-214600-voice-quality/` contains game comparisons and listening controls; `20261106-001140-rvc-overlap/` contains rejected overlap experiments; `20261106-004820-game-reserve/` contains the authorized reserve trials, fixed workloads, raw windows and final qualification. `tests/resource_worker.py` measures local worker CPU time and sample output; `tests/resource_load.py` creates fixed CPU/CUDA jobs; `tests/resource_monitor.py` records total utilization separately. None plays audio.

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
