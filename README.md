# isMyMicThatBad

A Windows voice changer with a frosted-glass filter-card editor, configurable audio chains, presets and optional close-to-tray behavior.

Download the portable Windows x64 ZIP from [GitHub Releases](https://github.com/t66sun/isMyMicThatBad/releases). Usage and known audio limitations are in [the user guide](docs/user-guide.md).

## Source and build

`src/app/src` contains the React editor; `src/native` contains our Rust audio engine, window/tray behavior and native build configuration. `scripts/Initialize-Native.ps1` copies this source into the ignored `src/app/src-tauri` workspace. Edit `src/native`, not the generated copy. Research checkouts, dependency sources, vendor driver/runtime files and build output are excluded from Git.

On Windows, install Node.js, the Rust MSVC toolchain, Visual Studio C++ Build Tools, Windows SDK and libclang (set `LIBCLANG_PATH` if needed). Run:

```powershell
./scripts/Build.ps1 -ChecksOnly
./scripts/Build.ps1
```

The local workspace must also contain the original VB-CABLE redistribution files under `src/app/src-tauri/resources/vb-cable` and Microsoft redistributable DLLs under `resources/msvc-runtime` for the portable package. Follow the vendors' redistribution terms; these third-party files are not source assets in this repository. The optimized build creates the executable without an application installer. `scripts/Package.ps1` requires a clean committed source tree and a matching local acceptance record before creating a portable ZIP.

The native tray regression in `tests/tray.mjs` uses an isolated portable copy, a local Edge WebDriver and PowerShell 7 for Windows UI Automation. It verifies processing state, persistence, restore/quit and failed-start feedback without saving microphone audio.
