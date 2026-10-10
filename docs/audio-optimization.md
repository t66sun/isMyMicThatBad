# Audio latency follow-up

V1 acceptance is p95 ≤150 ms for the documented microphone-to-CABLE calibration, revised by the user on 2026-10-09. The earlier 120 ms and 80 ms goals are deferred. At a 5 ms app buffer, optimized raw/frequency-zero-and-gain p95 measured 137.81/140.26 ms. Measurements and acceptance status live in [the hardware record](../research/audio-acceptance/README.md).

## Measured constraints

- VB-CABLE current latency is 3072 samples, internal rate 48 kHz. The registry confirms the user's setting. Cable-only p95 is 56.29 ms, compared with 64.53 ms before adjustment. This render-to-capture boundary differs from full-path calibration; subtraction cannot isolate app latency.
- Debug full-path measurements with the Intel microphone remain approximately 128–140 ms. Queue snapshots show capture buffers generally below 13 ms and output buffers below 26 ms. These are snapshots, not percentile measurements.
- Requesting a fixed 10 ms WASAPI output buffer did not reduce the observed render lead or full-path p95; the experiment was reverted. Windows may enlarge a request to its required minimum, per [IAudioClient::Initialize](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize).
- Reducing the steady output reserve from two device periods to one caused 19 underruns in a 12-second debug run and failed latency acceptance. The experiment was reverted. Keep the working reserve until scheduling and startup demand are understood.
- The same smaller-reserve experiment caused 16 underruns in an optimized run. Optimization alone did not make it viable. A stable 10 ms app-buffer run measured 156.46 ms with the delayed-filter scenario; the 5 ms configuration is the qualified milestone, not a latency guarantee for all settings.

## Deferred work

| Priority | Investigation | Evidence required before keeping a change |
| --- | --- | --- |
| 1 | Profile optimized builds, callback cadence and DSP worker scheduling; distinguish startup gaps from ongoing starvation. | Matched hardware/configuration before-and-after p95 and counters; 30-minute stability with no counter growth after startup. |
| 2 | Tune VB-CABLE latency against its actual input/output buffer statistics. Keep internal rate matched at 48 kHz. | Driver statistics satisfy the vendor's sizing guidance; full-path correlation passes and no new driver glitches. Preserve a known working setting for rollback. |
| 3 | Reduce application buffering only after proving why a smaller reserve currently starves. | Stable FIFO fill and callback deadlines under monitoring, live edits and CPU load; no sample dropping used to meet the target. |
| 4 | Evaluate Windows low-latency shared-mode periods if the installed CPAL backend remains the limiting factor. | Demonstrated host-buffer contribution and compatibility with real microphone, cable and monitor devices; avoid a custom backend without measured need. |
| 5 | Profile the two sinc conversions and filter chains before changing resampling quality or filter length. | CPU savings, frequency-response checks, independent-clock stability and audible comparisons; retain clock correction and smooth graph transitions. |

The inactive TANCHJIM microphone produced insufficient correlated signal in the first run. It needs a separate audible-input test before making claims about that device. Calibration excludes chat-app buffering and does not cover every possible 64-card chain.

## v0.3 scheduling investigation

The first new-preset capture failed the 200 ms target: female voice p95 211.79 ms, with 8 application underruns and 4 driver Xruns. Most qualified windows were 185–189 ms; the last two rose to about 212 ms. Keep this failed run as diagnostic evidence, not acceptance.

Offline DSP budget passed on this machine: four seconds of female/blown-mic/broadcast/ethereal audio rendered in 0.3229/0.0149/0.0089/0.4315 seconds. This measures DSP headroom, not callback timing or end-to-end latency. CPAL's optional realtime feature was disabled. It is now enabled and the DSP worker uses the same audio_thread_priority library, releasing its scheduling handle on exit. Windows scheduling uses MMCSS; see [Microsoft's scheduler documentation](https://learn.microsoft.com/en-us/windows/win32/procthread/multimedia-class-scheduler-service). Scheduling-denied fallback retains normal operation; hardware acceptance must still be measured. No system registry, timer, driver setting or queue reserve was changed.
The initial raw-RMS envelope reference rejected adequate transformed speech because formant/EQ changes alter its energy pattern. The final measurement predicts the reference with the same preset and attributes samples to their source capture timestamps by subtracting declared DSP latency before envelope matching. Hardware and algorithmic delay both remain in the result. Spectral-chain calibration checks total delays of 89.67/242.67/292.67 ms (47/200/250 ms hardware plus 42.67 ms DSP); the direct amplitude response independently agrees with declared latency within 8 ms. Neither low-correlation attempts nor quiet runs count as passing evidence.
Final v0.3 latency capture meets both milestone limits: raw/frequency-zero-gain p95 138.66/141.91 ms; female/blown-mic/broadcast/ethereal 180.96/149.31/136.57/188.21 ms. Female voice had one additional underrun (3 to 4); the user explicitly accepted this known issue for delivery. Investigate capture/output cadence and the scheduling deadline before changing buffer reserve; retain the event and its measured latency rather than claiming zero-glitch stability. Other new presets had no counter growth. Full 30-minute stability and manual voice-quality assessment remain deferred.