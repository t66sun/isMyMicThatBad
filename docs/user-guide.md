# isMyMicThatBad user guide

English | [中文](user-guide_zh.md)

## Getting started

1. Extract the portable ZIP into a writable directory and run `isMyMicThatBad.exe`. This release is portable only.
2. The ZIP includes the Microsoft C++ runtime libraries the app needs; keep them next to the executable. The interface requires system WebView2. If it is missing, install the Evergreen Runtime from https://developer.microsoft.com/microsoft-edge/webview2/.
3. Click the gear in the title bar to open settings. If VB-CABLE is not installed, click "Install the bundled VB-CABLE", complete the official installer and allow the administrator prompt, then restart Windows.
4. In settings, select the real microphone as input and CABLE Input as output; select CABLE Output as the microphone in the chat application.
5. Click "Start processing". Headphone monitoring is off by default; before enabling it, select a headphone device different from the main output.

Portable only means the app needs no installation; VB-CABLE is still a system driver. The portable package keeps the official VB-CABLE files. VB-CABLE is Donationware from VB-Audio; donations are welcome. Source: https://vb-audio.com/Cable/. Distribution terms: https://vb-audio.com/Services/licensing.htm.

## Window and tray

"Close window to tray" in settings is off by default. When it is on, clicking close or pressing Alt+F4 hides the window and audio keeps processing. Click the grey-white waveform tray icon to restore the window, or right-click and choose "Show window". The "Start processing" item is checked while processing; click it again to stop. Window and tray share processing state, and a failed start restores the window with the reason. Right-click "Quit" stops audio and exits completely. The option is saved with the configuration; older configurations keep the close-to-quit behavior.

The app, title bar and tray use the same grey-white waveform icon.

## Card pipeline

Drag a filter from the library above, or click to add one. You can also click the plus at a position in the chain and then click a filter above to insert it there. Cards process left to right; drag a card's title or empty area to reorder. Dragging previews the position first and commits on release; press Esc or drop outside the chain to cancel. With many cards, scroll horizontally over the chain; dragging to the left or right edge scrolls automatically.

The filter library stays on one row; use the left and right buttons, a touchpad horizontal gesture or Tab to browse all filters. Cards show only their main parameters; scroll the parameter area below for the full controls.

Click a card and adjust sliders or enter values in the parameter area below. The power button bypasses the filter; the "⋯" menu moves it forward or backward, copies it or deletes it. With a card selected, Alt+Left/Right reorders it; Alt+Up/Down still works. Space or Enter selects a card. The same filter can be used repeatedly; copies have independent parameters. Deleting the selected card selects a neighbour, and an empty chain outputs the original microphone signal.

Undo and redo buttons sit beside the chain title; Ctrl+Z undoes and Ctrl+Y or Ctrl+Shift+Z redoes. Adding, removing, reordering, bypassing, parameter changes and preset switches are all undoable; one slider drag counts as one change, while text inputs keep their own text undo. A new change clears the redo history; the last 100 changes live only in the current window and clear on restart or reload. Undone chains still autosave and update audio in real time.

Frequency shift adds or subtracts a fixed number of Hz from every frequency, producing metallic or unnatural timbres; pitch shift moves in semitones and preserves formants, while formant shift adjusts timbre independently. The title-bar preset menu offers Original, Metallic, Telephone, Female, Blown mic, Broadcast, Ethereal, AI 清晰人声 and "Save current chain"; custom presets can be named, loaded and deleted in settings. Restarting restores settings but never starts capture automatically.

The noise gate controls the noise floor between phrases; it does not continuously remove noise while you speak. Distortion's "clip" control moves between soft saturation and hard clipping; 0% keeps the previous behavior. Adjacent pitch/formant cards share analysis to reduce latency.

Female and Ethereal now use a -60 dB gate threshold to preserve quiet speech and fading words. Reselect the preset to apply this setting; saved chains keep their own values. More background noise near this level can pass through. Pitch, formant, gate attack, hold and release are unchanged.

Tremolo makes volume rise and fall periodically: rate is adjustable from 0.1–20 Hz and depth from 0–100%. 0% depth keeps the original volume; 100% varies between the original volume and silence. It changes neither pitch nor algorithmic latency.

Bitcrusher adjusts bit depth from 2–16 bit, sample hold from 1–48 samples and mix from 0–100%. Lower bit depth sounds coarser; longer hold updates samples more slowly (at 48 kHz, holding 4 samples updates 12,000 times per second). The processed signal is quantized within ±1; 0% mix keeps the original. It adds no algorithmic latency and does not change the device sample rate.

For a cleaner RVC voice, choose **AI 清晰人声** from the preset menu. It keeps the current voice model and RVC controls, with saturation set to 10% wet and bitcrushing to 25% wet. Newly added bitcrushers also start at 25% mix. Existing saved cards retain their settings.

AI voice conversion (RVC) allows at most one card per chain in the first release and works with traditional filters, reordering, bypass and original-audio comparison. Add the AI card first and click "Prepare AI environment" in the parameter area; the first run downloads several GB of isolated Python, CUDA PyTorch, base models and a test voice, after which it runs offline; audio is never uploaded. A personal portable package with the full AI environment works immediately after extraction. The environment lives in the `ai` subdirectory of the configuration directory, with preparation logs in `ai/setup.log` and inference error logs in `ai/worker.log`. The first start loads the model and warms up, which can take tens of seconds; wait for "Processing".

The default test voice was created by [Saisho Utane](https://huggingface.co/Saisho-Utane/Saisho-Model-RVC) under CC BY 4.0; keep the author attribution when demonstrating or publishing results. RVC v1/v2 `.pth` models and optional `.index` indexes can be imported; model and index paths persist with cards, presets and undo history, and processing must stop before changing either. Without an index, retrieval is skipped and index strength has no effect. AI parameters include pitch, formant, index strength and volume-envelope mix; bypassing the card keeps inference state. Model load failure, out of memory, interrupted inference or inference falling behind real-time audio stops processing with an error; there is no automatic fallback to the original voice.

AI collects 200 ms of input per request and allows 420 ms for the reply. The chain reports 620 ms of algorithmic latency. The extra 160 ms reserve is intended to absorb the measured game-contention spikes; the audio endpoints and VB-CABLE add to that delay. Model startup prepares the synthesis lengths used by the formant slider before capture begins. If inference exhausts the reserve, processing stops and the app retains the first error. Increasing this reserve cannot compensate for sustained inference slower than the 200 ms input cadence.

After processing starts, click "Compare original" beside the sound-chain title to switch between original and processed audio for both the virtual microphone output and headphone monitoring. The pressed button and the title-bar "Original comparison" indicate original output; click again to restore effects. Switching is smooth and preserves chain parameters, bypass state and undo history; the original is compensated to the same algorithmic latency and filters keep running to preserve tails and internal state. Comparison state is not saved; stopping, a device failure or a restart restores effect output.

A portable package with `portable.marker` keeps settings in `data/settings.json` beside the executable; without the marker it uses `%APPDATA%/com.t66sun.ismymicthatbad/settings.json`. Corrupt settings are preserved as `settings.corrupt-*.json`; an unwritable directory shows a save error instead of silently losing the old configuration. Selecting a card and drag previews never write configuration.

Older distortion cards and user presets are automatically given 0% clip, and the pre-upgrade configuration is saved as `settings.pre-v0.3-*.json`. If backup fails, the upgrade and autosave stop so the original file survives.

## Devices and buffering

Stop processing before changing devices at runtime. If the monitoring device fails, monitoring turns off and the main output keeps running; if the main input or output fails, processing stops and the chain configuration is preserved. If audio drops, raise "Buffer target", restart and check the Windows audio format and the VB-CABLE control panel.

Internal processing is mono at 48 kHz, resampled from and to the device format. The frequency-shift filter has algorithmic latency; deleting it while running keeps the existing delay compensation, while stop/start recalculates it. The first-release acceptance target is p95 ≤150 ms on the specified test path; 64 cards is the configuration limit, and arbitrary long chains are not promised to reach it.

New configurations default to a 5 ms buffer target; existing configurations keep their value. Buffer target is not end-to-end latency. Underrun counts record output-queue starvation, overflow records dropped samples, and device interruptions record brief driver-reported dropouts; these notifications are not mistaken for a disconnected device.

v0.3 acceptance: Original p95 138.66 ms, frequency-zero/gain 141.91 ms; Female 180.96 ms, Blown mic 149.31 ms, Broadcast 136.57 ms, Ethereal 188.21 ms. This met the older calibration path's ≤150 ms and the new presets' ≤200 ms stage targets, using the Intel built-in microphone, VB-CABLE 3072 samples/48000 Hz, a 5 ms buffer and monitoring off; extra buffering in other devices and chat applications is outside these results.

The v0.3 female preset's 14-second short test once underran. Current source adds output and startup reserves and zeroes inaudible tiny tails before output resampling so they cannot slow the callback. Under the same device configuration, three consecutive 10-second start tests showed no application underruns or overflows; the follow-up long run was stopped at the user's request after 20 minutes 30 seconds, with both counters at 0 for the recorded interval, and does not count as a completed 30-minute acceptance. Brief driver-interruption notifications are still recorded, end-to-end latency was not remeasured, and the added reserve may slightly increase latency; the historical results above do not represent a new acceptance of current source. The female timbre has not been auditioned; adjust pitch and formant to your own voice.

This measurement used the Intel array microphone; if the default input is a USB device with no actual signal, select a microphone with input level manually in settings.

The 2026-10-12 automated WAV retest used WASAPI digital loopback input from the Realtek playback side, VB-CABLE output, a 5 ms buffer and monitoring off. Original p95 173.27 ms, frequency-zero/gain 176.11 ms, Female 203.28 ms; Tremolo 176.23 ms, Bitcrusher 172.36 ms. The four original presets and both new filters passed the digital-loopback stage checks with no application underruns or overflows; a few startup phases recorded driver-interruption notifications. The female preset ran continuously for 60 seconds with no application underruns, overflows or driver-interruption notifications. Digital loopback includes extra Windows playback and capture buffering, so it uses a separate budget of original ≤200 ms and presets ≤250 ms; these results do not replace the physical-microphone acceptance of original ≤150 ms and presets ≤200 ms.

The final v0.4 stability acceptance on 2026-10-13 used speaker playback of the test speech, Intel array microphone capture and VB-CABLE output, a 5 ms buffer and monitoring off. Original p95 176.43 ms, frequency-zero/gain 181.05 ms; Female 221.88 ms, Blown mic 176.70 ms, Broadcast 171.66 ms, Ethereal 218.32 ms. Every scenario had at least 26 valid matched windows and zero application underruns/overflows; the female preset also passed 60 continuous seconds and 10 seconds of silence. Driver startup interrupt notifications remain and do not grow during preset runs. Original and frequency-zero/gain exceed 150 ms, and Female and Ethereal exceed 200 ms; the user explicitly chose stability over the latency target and accepted honest delivery, so this release does not claim the original target. The output driver starts silently and does not consume audio before prefill completes; clock control does not update without a complete input block.

Run `./tests/Run-AudioAcceptance.ps1` from the source directory to generate and loop a speech WAV automatically; logs go to `research/audio-acceptance/artifacts`. `-StimulusWavPath` can point to a 48 kHz, mono, PCM16 WAV; playback uses the Realtek speakers and is audible. By default the test uses digital loopback to exercise the software and virtual-microphone path; `-PhysicalMicrophone` captures through the Intel array microphone, and an acoustic loop that is too weak is rejected as silent. `-AiRootPath '<prepared ai directory>'` adds AI live-edit, VB-CABLE output and end-to-end latency calibration without human speech; the AI latency budget is separately p95 ≤1000 ms. The script restores the test environment variables when it ends.
