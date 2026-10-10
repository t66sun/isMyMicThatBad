# isMyMicThatBad

A Windows voice changer with a frosted-glass filter-card editor, configurable audio chains, presets and optional close-to-tray behavior.

Download the portable Windows x64 ZIP from [GitHub Releases](https://github.com/t66sun/isMyMicThatBad/releases). Usage and known audio limitations are in [the user guide](docs/user-guide.md).

## Architecture

The microphone feeds bounded WASAPI queues, an ordered mono DSP chain and CABLE Input; other applications capture CABLE Output. Optional headphone monitoring adapts its clock independently. Processing uses 48 kHz, 256-frame blocks.

- React owns the session and filter-card editor. Drag previews and selection are temporary; a valid drop commits the chain through validated native commands.
- `src/app/src/filters.json` is the shared catalog and parameter contract. Rust validates it before constructing processors in `dsp.rs` and `effects.rs`.
- `audio.rs` owns streams, preallocated queues and the worker; `clock.rs` handles resampling. `graph.rs` preserves processor state and crossfades live changes. Audio callbacks use preallocated processing buffers.
- `settings.rs` persists configuration and preserves corrupted originals. `main.rs` owns window/tray behavior and the shared audio controller. Loading settings never starts capture.

See [filter development](docs/filter-development.md) for the compiled extension point and [audio optimization](docs/audio-optimization.md) for deferred latency work. Track implementation tasks and release history in GitHub Issues and Releases.

## Source and build

`src/app/src` contains the React editor; `src/native` contains our Rust audio engine, window/tray behavior and native build configuration. `scripts/Initialize-Native.ps1` copies this source into the ignored `src/app/src-tauri` workspace. Edit `src/native`, not the generated copy. Research checkouts, dependency sources, vendor driver/runtime files and build output are excluded from Git.

On Windows, install Node.js, the Rust MSVC toolchain, Visual Studio C++ Build Tools, Windows SDK and libclang (set `LIBCLANG_PATH` if needed). Run:

```powershell
./scripts/Build.ps1 -ChecksOnly
./scripts/Build.ps1
```

The local workspace must also contain the original VB-CABLE redistribution files under `src/app/src-tauri/resources/vb-cable` and Microsoft redistributable DLLs under `resources/msvc-runtime` for the portable package. Follow the vendors' redistribution terms; these third-party files are not source assets in this repository. The optimized build creates the executable without an application installer. `scripts/Package.ps1` requires a clean committed source tree and a matching local acceptance record before creating a portable ZIP.

The native tray regression in `tests/tray.mjs` uses an isolated portable copy, a local Edge WebDriver and PowerShell 7 for Windows UI Automation. It verifies processing state, persistence, restore/quit and failed-start feedback without saving microphone audio.
