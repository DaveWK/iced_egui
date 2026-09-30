# Contributor handoff

## Experimental wgpu 30 branch

`feat/nastyfork-wgpu30` upgrades egui/eframe to 0.36.2 and egui_tiles to 0.17.1
using pinned `iced-wgpu-nastyfork` and `cryoglyph-nastyfork` dependencies.
See [docs/NASTYFORK.md](docs/NASTYFORK.md) for the current dependency patches,
validation record, and open gates. Earlier verification notes below describe
the original egui 0.33 branch unless repeated in that document.

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
transfer to an egui editor. Standard synchronous OS clipboard reads remain
unsupported. Browser/native OS clipboard delivery has not been
manually validated; integration tests assert the platform output boundary.

## Keyboard, primary selection and overlays

`input.rs` implements focusable-widget traversal and cursor conversion.
`clipboard.rs` routes every egui logical key and IME lifecycle event, preserving
semantic clipboard events. `IcedPane` handles Tab/Shift+Tab boundaries before
egui allocates the pane on the next frame, preserving egui's focus ordering.
Iced redraw deadlines and IME candidate rectangles feed egui platform output;
preedit text is a separate egui overlay until committed.

`PrimarySelection` is an injectable provider shared by the host. The optional
Linux `primary-selection` feature adds arboard with Wayland data-control support.
Initialization and later access errors are surfaced. Selection capture uses
Iced's copy handling without emitting standard clipboard output; middle-click
reads PRIMARY, positions the caret, and pastes it. Password selection is tested
not to replace PRIMARY. Tests inject a provider, not the desktop clipboard.

`overlay.rs` constrains the root widget to its pane but gives overlays viewport
bounds. It gates base and overlay draw passes into separate textures. Foreground
egui areas route popup input; nested overlays use the same pass. Tooltip areas
are passive, inferred from the overlay's center-point mouse interaction. Outside
clicks dismiss menus and remain available to surrounding egui. Escape uses the
standard menu's outside-click dismissal path. Custom overlays should report their
mouse interaction accurately. A visible overlay allocates a viewport-sized
texture; `IcedPlotPane` keeps its cached rendering path.

The Vulkan tests cover actual text-input composition, candidate coordinates,
cursors, Tab/Shift+Tab within/across panes and egui, middle-click PRIMARY,
password privacy, popup pixels outside a pane, selection and dismissal, and
passive tooltip cleanup. Unit tests cover all egui logical keys and clipboard
errors. Native OS IME/selection delivery and browser interaction still need
manual validation.

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
cargo clippy --all-targets --features fira-sans,plotters,primary-selection -- -D warnings
cargo check --no-default-features
cargo test --features fira-sans,plotters,primary-selection
cargo test --features fira-sans,plotters,primary-selection --lib -- --ignored --nocapture
cargo check --target wasm32-unknown-unknown --features fira-sans,plotters,webgl
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features fira-sans,plotters,primary-selection
```

The ignored test requires a Vulkan adapter; Mesa lavapipe works in CI.

## Next contributions

1. Validate both native examples on screen and the web demo in browsers.
   Check colors, transparency, fonts and pointer coordinates at different
   scale factors.
2. Validate native IME candidate windows and primary-selection protocol support
   on X11 and the intended Wayland compositors.
3. Measure overlay texture cost and test additional custom/nested overlays.
4. Extend chart functionality: built-in legends, nonlinear axes and more
   examples such as candlesticks and heatmaps.
5. Benchmark rendering and cached frames before making performance claims.

GitHub publication and crates.io release are separate. Before a crate
release, run `cargo publish --dry-run`, review the packaged files, and
complete the outstanding visual checks.
