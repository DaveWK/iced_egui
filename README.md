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

## Tests

```sh
cargo test --features fira-sans,plotters
cargo test --features fira-sans,plotters --lib -- --ignored --nocapture
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
from one cannot be used by the other. This release pairs:

| crate | version | wgpu |
|---|---|---|
| egui, eframe, egui-wgpu | 0.33 | 27 |
| iced_* | 0.14 | 27 |
| plotters-iced2 | 0.14 | |

egui 0.34+ moved to wgpu 29/30; this crate will follow when Iced does.

## Web

The library builds for `wasm32-unknown-unknown` with the `webgl` feature,
**provided nothing in your dependency graph enables wgpu's
`fragile-send-sync-non-atomic-wasm` feature.** That feature makes wgpu
demand `Send + Sync` window handles, which `iced_wgpu`'s compositor does
not provide on wasm32, and `eframe` / `egui-wgpu` enable it by default.
For a web build use:

```toml
eframe = { version = "0.33", default-features = false, features = ["wgpu"] }
iced_egui = { version = "0.1", features = ["fira-sans", "webgl"] }
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

## Not yet

- Keyboard and IME events are not forwarded (mouse only).
- Iced's clipboard is `clipboard::Null`.
- Iced overlays (pick-list menus, tooltips) draw inside the pane's texture
  and are clipped to it; they cannot float over surrounding egui.

## License

MIT or Apache-2.0, at your option.
