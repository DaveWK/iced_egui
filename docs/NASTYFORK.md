# Iced 0.14 + egui 0.36 on wgpu 30

The `feat/nastyfork-wgpu30` branch uses egui/eframe/egui-wgpu 0.36.2,
egui_tiles 0.17.1, Iced 0.14 and plotters-iced2 0.14 with a shared wgpu 30.0.1
device and queue. `IcedPlotPane` still occupies an ordinary egui_tiles pane.
The renderer and text renderer are ported; chart code needs no source changes.

This is an experimental compile-time dependency migration, not a runtime
`NastyShim` toggle. Cargo patches apply to the entire application dependency
graph and cannot be conditional on a crate feature. `main` remains on egui 0.33
and wgpu 27. This branch sets `publish = false`: a consumer must explicitly
select the forked dependencies, which crates.io would not carry transitively.

## Application setup

Use Rust 1.95 or newer. Add these patches to the **application workspace root**,
even when depending on this crate through Git. Cargo ignores dependency-local
patch sections. The separate `web-demo` root has the same pins.

```toml
[patch.crates-io]
iced_wgpu = { git = "https://github.com/DaveWK/iced-wgpu-nastyfork", rev = "1f0b97b4e8c47ed64a6e66007672c9dbfd1baf61" }
cryoglyph = { git = "https://github.com/DaveWK/cryoglyph-nastyfork", rev = "078fab8b8425c5da41465d4aca7fb78c5af3b7fe" }
```

For eframe 0.36, use `default-features = false` and
`features = ["wgpu_no_default_features", "default_fonts"]`, with `x11` and/or
`wayland` for native examples. The plain `wgpu` feature enables egui-wgpu's
`fragile-send-sync-non-atomic-wasm`, incompatible with Iced's wasm window bounds.
Keep `tiny-skia` disabled in `iced_renderer` so the renderer alias remains wgpu.

The forks retain their upstream Cargo package names (`iced_wgpu`, `cryoglyph`)
and package versions. Each is a standalone release-source extraction with an
upstream baseline commit, provenance document, original manifests, and licenses.
GitHub repository names use `nastyfork`; no upstream crate name is republished.

## Port details

- Renderer and glyph staging uploads, pipeline descriptors, samplers, readback,
  adapter selection, native display ownership, surface acquisition and queue
  presentation use wgpu 30 APIs.
- egui's new `eframe::App::ui` entry point and panel APIs are used by examples.
- Modifier events and the new physical modifier keys route to Iced. IME preedit
  selection offsets convert character indices to UTF-8 byte offsets; commits
  and empty preedit clear composition. Input purpose reaches egui's IME output.
- Headless tests explicitly discard egui font-atlas updates because they read
  Iced render targets directly rather than running egui's compositor.

The shared-device path stays on the GPU. Pixel readback exists only in tests
and upstream screenshot APIs. No cross-version GPU handle casts are added.
No rendering performance improvement is claimed without benchmarks.

## Validation (2026-09-30)

- All 18 bridge unit/integration tests pass, including 8 Vulkan tests, on
  AMD RX 7900 XT / RADV and Mesa llvmpipe with `--test-threads=1`.
- Regression coverage includes clipboard isolation, primary selection,
  keyboard/IME focus, popup/tooltip overlays, redraw timing, chart interaction,
  DPI/resize, pane isolation, and rendered pixels.
- Renderer fork: Vulkan pixels for quads, text, geometry, odd-width raster
  uploads, SVGs, MSAA, DPI changes and resize. Glyph fork: pixel output and
  vertex-buffer growth/reuse with 1,000 text areas.
- The doctest, minimal-feature build, and rustdoc with warnings denied pass.
- Native examples and strict bridge clippy checks pass. Renderer all-feature
  clippy and glyph library clippy pass; glyph examples/benchmarks compile.
- wasm library (including plotters/WebGL) and the separate eframe web demo
  compile. Browser execution is not yet validated.

```sh
cargo fmt --all --check
cargo clippy --all-targets --features fira-sans,plotters,primary-selection -- -D warnings
cargo test --features fira-sans,plotters,primary-selection
cargo test --features fira-sans,plotters,primary-selection --lib -- --ignored --test-threads=1
cargo check --no-default-features
cargo check --target wasm32-unknown-unknown --features fira-sans,plotters,webgl
cargo check --manifest-path web-demo/Cargo.toml --target wasm32-unknown-unknown
```

## Open gates

On-screen native window presentation, actual browser rendering, Metal and DX12
remain unverified. The native compositor's new surface-status paths are compiled
but not exercised by offscreen Vulkan tests.

`egui::ImeEvent::DeleteSurrounding` has no corresponding Iced 0.14 operation;
it remains unconsumed rather than guessing with backspace counts. Real OS IME
and X11/Wayland primary clipboard integration still need interactive testing.

A concurrent software-Vulkan test run crashed during validation. Serialized
runs pass; the CI Vulkan command uses `--test-threads=1`. The cause of that
parallel-process crash has not been isolated.

CI remains restricted to self-hosted Fedora/RHEL-family runners. Registration
is still deferred; no Ubuntu runner or automatic public-PR execution is added.
