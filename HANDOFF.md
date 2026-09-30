# Contributor handoff

## Architecture

`IcedHost` runs a headless Iced wgpu renderer on egui's own device and
queue. Each `IcedPane` owns an offscreen texture and Iced widget-state
cache. It translates pointer events, updates the widget tree, presents
to the texture, and paints that texture with `ui.painter().image()`.

The optional `plotters` feature exports `IcedPlotPane`, `PlotSource`,
`PlotBounds` and `PlotOutput`. The chart pane owns pan, zoom, box selection,
reset, crosshair, tooltip and redraw caching. Applications implement
Plotters series drawing without importing Iced types. Axes are linear f64;
range persistence is explicit through `bounds()` and `set_bounds()`.

## Verified locally

On 2026-09-30:

- Range validation, coordinate mapping, pan and zoom math tests pass.
- The Vulkan integration test passes with Mesa llvmpipe and an AMD Radeon
  RX 7900 XT (RADV). It covers rendering and pixel readback, hover caching,
  pinch zoom, pan/release, box selection, resize, DPI, reset, theme and pane
  isolation.
- Native examples compile; clippy and rustdoc pass with warnings denied.
- The doctest, feature-disabled build and wasm32 check with the plotters
  feature pass.
- The basic eframe web demo has been type-checked.

On-screen and browser visual validation remain outstanding. Headless test
success does not establish hardware performance or cross-browser behavior.
See GitHub Actions for CI results on the published commit.

## Clipboard follow-up

`src/clipboard.rs` bridges the Iced Clipboard trait to egui CopyText output
and event-scoped Paste payloads. The focused pane consumes semantic
Copy/Cut/Paste events, synthesizes Iced command shortcuts with key releases,
and suppresses duplicate raw shortcuts. Event ordering is preserved for
multiple pastes per frame. Focus loss clears Iced widget focus.

The widgets example includes both Iced and egui text editors. Tests exercise
Unicode, empty/multiple pastes, copy/cut, shortcut deduplication, and focus
transfer to an egui editor. Primary selection and synchronous OS clipboard
reads are unsupported. Browser/native OS clipboard delivery has not been
manually validated; integration tests assert the platform output boundary.

## Compatibility constraints

- egui/eframe/egui-wgpu 0.33 and Iced 0.14 share wgpu 27.
- Keep `iced_renderer/tiny-skia` disabled: plotters-iced2 implements its
  renderer trait on the Iced renderer alias used by this crate.
- On wasm, keep `wgpu/fragile-send-sync-non-atomic-wasm` disabled.
  Use eframe without default features; see the README.
- Load fonts explicitly or enable `fira-sans`.
- The shared renderer uses a RefCell. Do not call another pane's
  `show()` recursively from an Iced view closure.
- Generic Iced panes redraw each frame unless demand rendering is enabled.
  Chart panes cache by data revision, range, dimensions, DPI and dark mode.

## Development checks

```sh
cargo fmt --all --check
cargo clippy --all-targets --features fira-sans,plotters -- -D warnings
cargo check --no-default-features
cargo test --features fira-sans,plotters
cargo test --features fira-sans,plotters --lib -- --ignored --nocapture
cargo check --target wasm32-unknown-unknown --features fira-sans,plotters,webgl
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features fira-sans,plotters
```

The ignored test requires a Vulkan adapter; Mesa lavapipe works in CI.

## Next contributions

1. Validate both native examples on screen and the web demo in browsers.
   Check colors, transparency, fonts and pointer coordinates at different
   scale factors.
2. Extend keyboard coverage and IME composition support; standard text
   clipboard and focused editing input are implemented.
3. Decide how generic Iced overlays should behave beyond pane boundaries;
   currently they are clipped to the pane texture.
4. Extend chart functionality: built-in legends, nonlinear axes and more
   examples such as candlesticks and heatmaps.
5. Benchmark rendering and cached frames before making performance claims.

GitHub publication and crates.io release are separate. Before a crate
release, run `cargo publish --dry-run`, review the packaged files, and
complete the outstanding visual checks.
