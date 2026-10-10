# Audio latency follow-up

Qualified targets are p95 ≤150 ms for the raw/frequency-zero-gain calibration and ≤200 ms for the four new presets. The earlier 120 ms and 80 ms goals remain deferred. Current measurements and limitations are in [the user guide](user-guide.md#设备与缓冲); v0.3.2 inherits v0.3.0 hardware qualification without remeasurement.

## Constraints to preserve

- Qualified configuration: Intel microphone, VB-CABLE 3072 samples / 48 kHz, 5 ms app buffer, monitoring off. Cable-only p95 measured 56.29 ms; this boundary differs from the full path, so subtraction cannot isolate app latency.
- A fixed 10 ms WASAPI output-buffer request did not reduce full-path latency and was reverted. Reducing output reserve from two device periods to one caused 19 debug and 16 optimized-run underruns and was reverted. Keep the working reserve until scheduling/startup demand is understood.
- CPAL realtime scheduling and DSP-worker priority are enabled. Callbacks wake the worker. Offline DSP headroom does not establish callback deadlines or end-to-end latency.
- Transformed voices require the same-preset processed reference with source timestamps minus declared DSP latency. Direct raw-waveform comparison is unreliable; quiet or low-correlation runs are not passing evidence.

## Deferred work

| Priority | Investigation | Evidence required before keeping a change |
| --- | --- | --- |
| 1 | Profile capture/output cadence, worker scheduling and the female preset's accepted underrun. Distinguish startup gaps from ongoing starvation. | Matched hardware/configuration before-and-after p95 and counters; 30-minute stability without counter growth after startup. |
| 2 | Tune VB-CABLE against its actual buffer statistics, preserving 48 kHz and a rollback setting. | Full-path correlation passes without additional driver glitches. |
| 3 | Reduce application buffering only after explaining why a smaller reserve starves. | Stable FIFO fill and callback deadlines under monitoring, live edits and CPU load; no sample dropping to meet the target. |
| 4 | Evaluate Windows low-latency shared-mode periods only if the backend is the measured limiting factor. | Demonstrated host-buffer contribution and compatibility with microphone, cable and monitor devices. |
| 5 | Profile sinc conversions and long chains before changing resampling quality or filter length. | CPU savings, frequency response, independent-clock stability and audible comparisons. |

Full stability and manual voice-naturalness assessment remain deferred. Hardware results exclude chat-app buffering, other devices and arbitrary 64-card chains. Preserve local raw measurements and completed release packages as evidence.
