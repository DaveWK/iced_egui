# iced_egui

Development preview. Not yet released on crates.io; clone this repository
to run the examples.

Render [Iced](https://iced.rs) widget trees inside [egui](https://egui.rs),
on egui's own wgpu device.

`iced_egui` runs Iced's wgpu renderer headless, presents an Iced
`UserInterface` into an offscreen texture, and paints that texture as an
ordinary egui image. An `IcedPane` is just another egui widget: it takes the
rect you give it, lives in a panel, a window or an
[`egui_tiles`](https://crates.io/crates/egui_tiles) pane, and drags around
like anything else.

The motivating use is charts. [`plotters-iced2`](https://crates.io/crates/plotters-iced2)
draws [plotters](https://crates.io/crates/plotters) charts into an Iced canvas,
so with this crate every plotters series (line, area, bar, histogram,
candlestick, heatmap, scatter, your own) is available inside an egui
dashboard, tessellated by Iced and drawn by wgpu.

```rust
use iced_egui::{IcedHost, IcedPane};
use iced_widget::{button, column, text};
use std::rc::Rc;

#[derive(Clone, Debug)]
enum Msg { Bump }

struct App { pane: IcedPane<Msg>, n: u32 }

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc.wgpu_render_state.as_ref().expect("run eframe with the wgpu backend");
        Self { pane: IcedPane::new(Rc::new(IcedHost::new(rs))), n: 0 }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let n = self.n;
            let out = self.pane.show(ui, || {
                column![text(format!("clicked {n} times")), button("bump").on_press(Msg::Bump)].into()
            });
            for m in out.messages { match m { Msg::Bump => self.n += 1 } }
        });
    }
}
```

## Examples

```sh
cargo run --example plotters --features fira-sans,plotters   # two live charts in an egui_tiles layout
cargo run --example widgets  --features fira-sans   # Iced button / slider / checkbox next to egui ones
```

## IcedPlotPane

Enable the optional `plotters` feature for the reusable chart widget.
Keep one `IcedPlotPane` per dashboard tile, implement `PlotSource` for
your data, and call `pane.show(ui, &source)`. The source supplies a revision,
full data bounds, Plotters series drawing, and an optional hover tooltip.

- Drag to pan; wheel or pinch to zoom around the cursor.
- Shift-drag to box-zoom; double-click to reset.
- Crosshair and tooltips are egui overlays: hover reuses the chart texture.
- Source revision, visible range, size, DPI or light/dark mode changes redraw.
- `bounds()` / `set_bounds()` allow application-managed persistence.
- Axes are linear f64; use epoch values and custom mesh formatters for time.
  Other coordinate systems and built-in series legend toggles are not implemented.
- Source drawing errors are returned in `PlotOutput` and displayed in the pane.

`PlotSource::draw` receives a typed Cartesian Plotters `ChartContext`,
so chart implementations do not import Iced. It supports arbitrary series
compatible with those axes, including custom elements. The trait uses a
generic drawing backend and is intentionally not object-safe.

The pane renders before processing interaction; range changes schedule the
next egui repaint. Range state lives in the pane, not global egui memory,
so keep the same instance when docking it elsewhere.

## Vulkan

The host shares egui's wgpu device and queue. If egui selects Vulkan, Iced
uses that same Vulkan device. For the native example:

```sh
WGPU_BACKEND=vulkan cargo run --example plotters --features fira-sans,plotters
```

There is no separate Iced backend selection. Browser builds use WebGPU or
WebGL through wgpu, rather than direct Vulkan access.

## CI

GitHub Actions uses a self-hosted Fedora/RHEL runner for native, Vulkan and
wasm checks. See [runner setup](docs/SELF-HOSTED-CI.md) for packages, labels
and registration. Public pull requests do not run automatically on this host.

## Tests

```sh
cargo test --features fira-sans,plotters
cargo test --features fira-sans,plotters --lib -- --ignored --nocapture --test-threads=1
```

The second command explicitly creates a Vulkan adapter (hardware or Mesa
lavapipe). It tests redraw caching, hover, zoom, pan/release, box-zoom,
resize, DPI, reset, theme and pane isolation, and reads back a rendered
pixel to verify the series actually reached the texture. Ordinary tests
cover range validation and coordinate transformations without needing a GPU.

## Requirements

- **egui on wgpu.** `eframe` with the `wgpu` feature and
  `Renderer::Wgpu`. The glow backend has no device to share.
- **No `tiny-skia` in iced.** `iced_widget::renderer::Renderer` is a type
  alias that resolves to `iced_wgpu::Renderer` only while `iced_renderer`'s
  `tiny-skia` feature is off. That alias is the type this crate builds
  interfaces for and the one `plotters-iced2` implements its renderer trait
  on. The `iced` facade crate turns `tiny-skia` on by default, so depend on
  `iced_widget` / `iced_runtime` directly, or on `iced` with
  `default-features = false`.
- **A font.** Enable the `fira-sans` feature, or call
  `IcedHost::load_font` with the same TTF you hand egui. Without one, Iced
  text is invisible wherever there are no system fonts (the browser).

## Version pinning

egui and Iced must be on the **same `wgpu` crate version**, since a device
from one cannot be used by the other. The experimental `feat/nastyfork-wgpu30`
branch pairs:

| crate | version | wgpu |
|---|---|---|
| egui, eframe, egui-wgpu | 0.36.2 | 30 |
| iced_* | 0.14 (forked renderer) | 30 |
| cryoglyph | 0.1 (fork) | 30 |
| egui_tiles | 0.17.1 | |
| plotters-iced2 | 0.14 | |

This branch requires the two `nastyfork` Git patches at your application workspace
root. See [docs/NASTYFORK.md](docs/NASTYFORK.md) for pinned dependency configuration,
port boundaries, validation, and limitations. Cargo does not inherit patches from
dependencies. `main` retains the original egui 0.33 / wgpu 27 stack. This branch
is not a crates.io release or a runtime switch between wgpu versions.

## Web

The library builds for `wasm32-unknown-unknown` with the `webgl` feature,
**provided nothing in your dependency graph enables wgpu's
`fragile-send-sync-non-atomic-wasm` feature.** That feature makes wgpu
demand `Send + Sync` window handles, which `iced_wgpu`'s compositor does
not provide on wasm32, and `egui-wgpu` defaults and eframe's `wgpu` feature enable it.
Use eframe's `wgpu_no_default_features` feature instead.
For a web build use:

```toml
eframe = { version = "0.36.2", default-features = false, features = ["wgpu_no_default_features"] }
iced_egui = { git = "https://github.com/DaveWK/iced_egui", branch = "feat/nastyfork-wgpu30", features = ["fira-sans", "webgl"] }
```

`web-demo/` is a minimal eframe web app wired this way (`trunk serve` in
that directory). See [HANDOFF.md](HANDOFF.md) for what has and has not been
verified.

## How a frame works

`IcedPane::show` does, in order:

1. Allocates a rect in the current `egui::Ui` and translates egui's pointer
   input on it (move, enter/leave, left/right press and release, wheel) into
   Iced mouse events in the pane's coordinate space.
2. Builds a `UserInterface` from your `view` closure, runs `update` with
   those events (collecting any messages), and rebuilds once if Iced reports
   the tree outdated.
3. Draws with the shared `iced_wgpu::Renderer` and presents into the pane's
   offscreen texture, recreating it when the rect or pixel density changed.
4. Paints the texture into the rect with `ui.painter().image(..)`, so egui
   clips, layers and composites it like any other image.

Presenting submits to the wgpu queue during egui's UI pass; egui's own frame
is submitted afterwards, and submissions execute in order.

## Redraw cost

By default the Iced interface is rebuilt and presented every egui frame,
which is always correct, and egui only repaints when something changed. For
heavy content, keep tessellation out of the per-frame path with Iced's own
`canvas::Cache` (`plotters-iced2` supports it through `Chart::draw_cache`),
or call `set_redraw_on_demand(true)` and `request_redraw()` to present only
when you say so. Input events always force a redraw.

## Clipboard and text editing

Click an Iced pane to give it keyboard focus. Copy and cut use egui's
platform clipboard output; paste consumes the text from egui's `Paste`
event. The same bridge works with native and web eframe backends without
another OS clipboard dependency. Run the `widgets` example to copy/paste
between its Iced and egui text inputs.

All egui logical keys, text input, IME events and modifiers are routed to
the focused pane. Tab and Shift+Tab traverse Iced widgets that implement the
focusable operation, then move to the neighboring egui widget or Iced pane.
Pointer cursors and Iced redraw deadlines are forwarded to egui. IME candidate
positioning uses the focused field's screen rectangle; preedit text appears
above the interface without entering the field until committed.

Clipboard events are consumed once. Each paste carries its own payload,
including Unicode and empty strings; the bridge never substitutes an earlier
paste. Standard clipboard reads remain event-scoped, not arbitrary synchronous
OS queries. Browser clipboard delivery uses eframe's platform integration.

### Linux primary selection

Enable the optional `primary-selection` feature and initialize the shared host:

```rust,ignore
let host = Rc::new(IcedHost::new(render_state));
host.enable_primary_selection()?;
```

This uses arboard's X11/Wayland backend. Wayland requires a supported compositor
data-control protocol. Text selection claims PRIMARY; middle-click positions
the caret and pastes PRIMARY. It never replaces the standard clipboard.
Secure Iced text inputs do not export their selection. Changed selections are
copied through Iced's copy handling into a capture-only clipboard; custom
widgets therefore need to support that handling to export selected text.

Applications can instead install their own `PrimarySelection` implementation
with `host.set_primary_selection(provider)`. Backend initialization returns an
error on failure; later read/write errors are available from
`host.take_primary_selection_error()`. Unavailable selections do not fall back
to the standard clipboard.

Try `cargo run --example widgets --features fira-sans,primary-selection` on Linux.
The example includes two Iced inputs, an egui editor, a pick list and a tooltip.

### Iced overlays

Pick-list menus, tooltips and nested overlays lay out against the egui viewport,
so they can extend beyond a panel or `egui_tiles` pane. The base widget tree
remains clipped to its pane. An extra transparent GPU texture is allocated only
while an overlay is visible and composited on egui's foreground layer. Chart
panes retain their existing cached rendering path.

Interactive overlays receive pointer input through an egui area covering their
bounds; passive tooltips do not intercept input. Interactivity is inferred from
the overlay's mouse interaction at its center. Custom overlays should return an
appropriate interaction there. Escape dismisses standard Iced menus using their
outside-click handling; outside clicks also dismiss them. Outside clicks remain
available to surrounding egui widgets. Popups stay inside the current viewport;
this is not a separate OS window or a global modal dialog.

Tests exercise real widgets on Vulkan, including IME preedit/commit, focus across
panes and egui, primary-selection isolation and failures, password privacy,
popup selection/dismissal, tooltip exit, and pixel readback beyond the pane.
Clipboard tests use an injected provider and inspect egui platform output; they
do not touch the developer's system clipboard.

Native compositor/IME delivery and browser visual checks remain outstanding.
The bridge cannot expose input metadata absent from egui's events, such as IME
selection ranges or Iced physical key codes. A visible overlay currently uses
a viewport-sized texture; benchmark this cost before making performance claims.

## License

MIT or Apache-2.0, at your option.
