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
