# Implementation checkpoint

Current milestone: v0.2.0 UI redesign, continued on 2026-10-09. The initial v0.1 commit is 654ac75 on master and its portable delivery remains intact. The current task implements the accepted monochrome frosted-glass concept directly in the Windows application.

## Current implementation

- Unified custom titlebar, native window controls, preset menu and settings sheet.
- Persistent top filter library, horizontal left-to-right chain, fixed shared parameter inspector.
- Pointer dragging starts at 6 px, uses a gentle lift and 180 ms drop, and previews neighboring displacement. A valid release commits once; Escape/outside release cancels without changing session/DSP.
- Card bypass remains visible; more-menu holds duplicate/delete/previous/next. Clicking insertion positions and Alt+left/right (plus old up/down aliases) provide alternatives. Space/Enter remain ordinary button activation.
- Local wheel-to-horizontal and edge auto-scroll, adjacent selection after deletion, empty inspector, reduced-motion support.
- Existing five filters, compiled custom template, native audio commands, session schema, endpoint IDs and settings validation preserved. Selection/drag state is not persisted.
- Version metadata is 0.2.0. Builds use no-bundle; delivery remains portable ZIP only.

## Verification

- Rust: five library regressions, six DSP/settings integrations and known-delay calibration passed; two hardware tests are opt-in.
- Frontend model tests, TypeScript and production build passed.
- Browser checks use installed Edge with Playwright (Browser plugin unavailable), production preview at 127.0.0.1:1420, and screenshots outside the repo. Drag preview/commit/cancellation, middle insertion, buttons/menus/keyboard, presets/reload, wheel/edge scroll and reduced motion are checked. Layouts: 1100×760, 900×640 and 390×844.
- Real portable WebView2 checks use matching Edge WebDriver over local HTTP. Native persistence, validation, actual microphone/CABLE routing, audio start/stop/restart, headphone monitoring, drag preview/drop during processing, bypass and maximize/restore/minimize were exercised. The test restores the isolated session, stops audio and closes its window.
- Hardware regression at the qualified 5 ms buffer passed: raw p95 147.13 ms; frequency-zero/-6 dB gain p95 146.58 ms; 26/26 correlated windows each, zero app underruns/overflows and one startup driver Xrun. The first near-silent run was inconclusive and was repeated with user-provided microphone sound.
- Full 30-minute stability testing remains explicitly waived. No claim of long-term stability for v0.2; see research/audio-acceptance/README.md for historical v0.1 evidence and current scope.
- The public repository has no existing issue ticket for this milestone. Creating the tracking issue through the GitHub connector returned HTTP 403 (Resource not accessible by integration). The implementation behavior is documented in docs/design/ui-v0.2.md; publishing the ticket remains pending issue-write access.

## Delivery and resume

Current AGENTS.md requires a local commit after applicable validation. On 2026-10-10, the owner authorized reorganizing unpublished commits and pushing first-party source. `research/` and the entire Tauri backend are now excluded from tracking while their local files are preserved. Published history remains unchanged; the complete previous local v0.2 commits are retained on `codex/backup-before-source-only-20261010`. A fresh GitHub checkout cannot build the Windows application without the locally retained backend. The previously delivered portable ZIP remains intact and still records its original tested commit. No new tag or GitHub Release is requested. Package.ps1 requires a clean committed source tree and acceptance matching the tested executable SHA-256.

Dot-source scripts/Use-Toolchain.ps1 for the existing project-local Rust toolchain. Build.ps1 runs normal checks and creates the optimized no-bundle executable. Stop owned test windows before rebuilding, since Windows locks running executable files. Test native UI from an isolated portable copy; its settings must not affect the user's application profile. Keep the working VB-CABLE driver/control panel intact (3072 samples, 48000 Hz).

The current no-WebSocket instruction is authoritative. Native UI automation uses HTTP WebDriver. The historical native-soak.mjs CDP harness requires explicit authorization before reuse. Screenshot/log directories and release binaries remain ignored; reference artwork is documentation only.
