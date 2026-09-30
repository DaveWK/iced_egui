//! Plotters charts with egui-owned interaction.
use crate::{IcedHost, IcedPane};
use plotters::{coord::types::RangedCoordf64, prelude::*};
use plotters_iced2::{Chart, ChartWidget};
use std::{cell::Cell, rc::Rc};

/// Finite, increasing Cartesian data ranges. Fields are private to preserve validity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlotBounds {
    x: [f64; 2],
    y: [f64; 2],
}

impl PlotBounds {
    /// Returns None for non-finite, reversed, zero-width or overflowing ranges.
    pub fn new(x: [f64; 2], y: [f64; 2]) -> Option<Self> {
        let valid = |r: [f64; 2]| {
            r[0].is_finite() && r[1].is_finite() && r[0] < r[1] && (r[1] - r[0]).is_finite()
        };
        (valid(x) && valid(y)).then_some(Self { x, y })
    }
    /// X-axis minimum and maximum.
    pub fn x(self) -> [f64; 2] {
        self.x
    }
    /// Y-axis minimum and maximum.
    pub fn y(self) -> [f64; 2] {
        self.y
    }
    fn point(self, rect: egui::Rect, p: egui::Pos2) -> [f64; 2] {
        [
            self.x[0] + (p.x - rect.left()) as f64 / rect.width() as f64 * (self.x[1] - self.x[0]),
            self.y[1] - (p.y - rect.top()) as f64 / rect.height() as f64 * (self.y[1] - self.y[0]),
        ]
    }
    fn pan(self, dx: f64, dy: f64) -> Self {
        Self::new(self.x.map(|v| v + dx), self.y.map(|v| v + dy)).unwrap_or(self)
    }
    fn zoom(self, anchor: [f64; 2], factor: f64) -> Self {
        Self::new(
            self.x.map(|v| anchor[0] + (v - anchor[0]) * factor),
            self.y.map(|v| anchor[1] + (v - anchor[1]) * factor),
        )
        .unwrap_or(self)
    }
}

/// An application's chart data. Implementations need no Iced types.
///
/// The pane creates linear f64 axes and owns margins so input coordinates
/// match the actual Plotters plotting area. Draw any Cartesian Plotters series
/// and optionally configure the mesh here. This generic trait is not dyn-compatible.
pub trait PlotSource {
    /// Change whenever data, labels, styles or other drawing settings change.
    fn revision(&self) -> u64;
    /// Initial bounds and the destination of double-click reset.
    fn bounds(&self) -> PlotBounds;
    /// Draw mesh and series. Drawing failures are displayed by the pane.
    fn draw<DB: DrawingBackend>(
        &self,
        chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    ) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>>;
    /// Tooltip at a data coordinate; override for nearest-point lookup.
    fn hover(&self, point: [f64; 2]) -> Option<String> {
        Some(format!("x: {:.4}\ny: {:.4}", point[0], point[1]))
    }
}

struct SourceChart<'a, S> {
    source: &'a S,
    bounds: PlotBounds,
    dark: bool,
    area: &'a Cell<Option<egui::Rect>>,
    error: &'a std::cell::RefCell<Option<String>>,
}
impl<S: PlotSource> Chart<()> for SourceChart<'_, S> {
    type State = ();
    fn build_chart<DB: DrawingBackend>(&self, _: &(), mut builder: ChartBuilder<DB>) {
        let result = (|| {
            let mut chart = builder
                .margin(8)
                .x_label_area_size(28)
                .y_label_area_size(48)
                .build_cartesian_2d(
                    self.bounds.x[0]..self.bounds.x[1],
                    self.bounds.y[0]..self.bounds.y[1],
                )?;
            let (x, y) = chart.plotting_area().get_pixel_range();
            self.area.set(Some(egui::Rect::from_min_max(
                egui::pos2(x.start as f32, y.start as f32),
                egui::pos2(x.end as f32, y.end as f32),
            )));
            let ink = if self.dark { WHITE } else { BLACK };
            chart
                .configure_mesh()
                .axis_style(ink)
                .label_style(("sans-serif", 12).into_font().color(&ink))
                .light_line_style(ink.mix(0.1))
                .bold_line_style(ink.mix(0.2))
                .draw()?;
            self.source.draw(&mut chart)
        })();
        if let Err(e) = result {
            *self.error.borrow_mut() = Some(format!("{e:?}"));
        }
    }
}

/// A reusable plotters-iced2 chart that fills an egui or egui_tiles pane.
///
/// Retain one instance per chart: range and texture survive docking moves.
/// Drag to pan, wheel/pinch to zoom, Shift-drag to select a zoom rectangle,
/// and double-click to reset. Hover overlays are drawn by egui without an
/// Iced redraw. Supports linear f64 axes; timestamp axes can use epoch values
/// with a custom mesh formatter in PlotSource::draw.
pub struct IcedPlotPane {
    pane: IcedPane<()>,
    bounds: Option<PlotBounds>,
    rendered: Option<(u64, PlotBounds, bool)>,
    area: Cell<Option<egui::Rect>>,
    drag: Option<(egui::Pos2, PlotBounds, bool)>,
    error: std::cell::RefCell<Option<String>>,
}

/// Chart response and rendering information.
pub struct PlotOutput {
    /// egui response, usable for context menus and hover handling.
    pub response: egui::Response,
    /// Whether the chart texture was rendered this frame.
    pub redrawn: bool,
    /// Current visible data range.
    pub bounds: PlotBounds,
    /// Plotters error, if drawing failed.
    pub error: Option<String>,
}

impl IcedPlotPane {
    /// Create a chart pane sharing the host's GPU renderer.
    pub fn new(host: Rc<IcedHost>) -> Self {
        let mut pane = IcedPane::new(host);
        pane.set_redraw_on_demand(true);
        Self {
            pane,
            bounds: None,
            rendered: None,
            area: Cell::new(None),
            drag: None,
            error: Default::default(),
        }
    }
    /// Current range, or None before first display.
    pub fn bounds(&self) -> Option<PlotBounds> {
        self.bounds
    }
    /// Restore a saved range or set one programmatically.
    pub fn set_bounds(&mut self, bounds: PlotBounds) {
        self.bounds = Some(bounds);
    }
    /// Use the source's full bounds on the next display.
    pub fn reset(&mut self) {
        self.bounds = None;
        self.drag = None;
    }
    /// Invalidate cached drawing even if the source revision is unchanged.
    pub fn request_redraw(&mut self) {
        self.pane.request_redraw();
    }
    /// Fill the available UI space. Source borrows last only for this call.
    pub fn show<S: PlotSource>(&mut self, ui: &mut egui::Ui, source: &S) -> PlotOutput {
        self.show_sized(ui, ui.available_size(), source)
    }
    /// Draw into an explicit size in logical egui points.
    pub fn show_sized<S: PlotSource>(
        &mut self,
        ui: &mut egui::Ui,
        size: egui::Vec2,
        source: &S,
    ) -> PlotOutput {
        let bounds = *self.bounds.get_or_insert_with(|| source.bounds());
        let dark = ui.visuals().dark_mode;
        let key = (source.revision(), bounds, dark);
        if self.rendered != Some(key) {
            self.pane.request_redraw();
        }
        // Tiny/hidden tiles must not create invalid Plotters axes.
        if size.x < 80.0 || size.y < 60.0 || !size.is_finite() {
            let (_, response) = ui.allocate_exact_size(egui::vec2(0.0, 0.0), egui::Sense::hover());
            return PlotOutput {
                response,
                redrawn: false,
                bounds,
                error: None,
            };
        }
        let out = self.pane.show_inner(ui, size, false, || {
            self.area.set(None);
            *self.error.borrow_mut() = None;
            ChartWidget::new(SourceChart {
                source,
                bounds,
                dark,
                area: &self.area,
                error: &self.error,
            })
            .into()
        });
        self.rendered = Some(key);
        let response = out.response;
        if let Some(local) = self.area.get() {
            let area = local.translate(response.rect.min.to_vec2());
            let pointer = ui.input(|i| i.pointer.interact_pos());
            if response.double_clicked() {
                self.reset();
            } else {
                if response.drag_started_by(egui::PointerButton::Primary)
                    && let Some(start) = ui
                        .input(|i| i.pointer.press_origin())
                        .filter(|p| area.contains(*p))
                {
                    self.drag = Some((start, bounds, ui.input(|i| i.modifiers.shift)));
                }
                if let (Some((start, original, boxed)), Some(p)) = (self.drag, pointer) {
                    if boxed {
                        let selection = egui::Rect::from_two_pos(start, area.clamp(p));
                        ui.painter().with_clip_rect(area).rect_stroke(
                            selection,
                            0.0,
                            egui::Stroke::new(1.0_f32, ui.visuals().selection.stroke.color),
                            egui::StrokeKind::Inside,
                        );
                        if response.drag_stopped()
                            && selection.width() > 3.0
                            && selection.height() > 3.0
                        {
                            let a = original.point(area, selection.left_bottom());
                            let b = original.point(area, selection.right_top());
                            self.bounds = PlotBounds::new([a[0], b[0]], [a[1], b[1]]);
                        }
                    } else {
                        let a = original.point(area, start);
                        let b = original.point(area, p);
                        self.bounds = Some(original.pan(a[0] - b[0], a[1] - b[1]));
                    }
                }
                if response.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
                    self.drag = None;
                }
                if response.hovered()
                    && self.drag.is_none()
                    && let Some(p) = pointer.filter(|p| area.contains(*p))
                {
                    let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
                    let factor = ((-scroll as f64 * 0.002).exp() / pinch as f64).clamp(0.1, 10.0);
                    if factor != 1.0 {
                        self.bounds = Some(bounds.zoom(bounds.point(area, p), factor));
                        ui.input_mut(|i| {
                            i.smooth_scroll_delta = egui::Vec2::ZERO;
                        });
                    }
                }
            }
            if response.hovered()
                && let Some(p) = pointer.filter(|p| area.contains(*p))
            {
                let painter = ui.painter().with_clip_rect(area);
                let stroke = egui::Stroke::new(1.0_f32, ui.visuals().weak_text_color());
                painter.line_segment(
                    [egui::pos2(p.x, area.top()), egui::pos2(p.x, area.bottom())],
                    stroke,
                );
                painter.line_segment(
                    [egui::pos2(area.left(), p.y), egui::pos2(area.right(), p.y)],
                    stroke,
                );
                if let Some(text) = source.hover(bounds.point(area, p)) {
                    response.clone().on_hover_ui_at_pointer(|ui| {
                        ui.label(text);
                    });
                }
            }
        }
        // Interaction changes the next texture; do not invalidate on mere hover.
        if self.bounds != Some(bounds) {
            ui.ctx().request_repaint();
        }
        let error = self.error.borrow().clone();
        if let Some(message) = &error {
            ui.painter().text(
                response.rect.center(),
                egui::Align2::CENTER_CENTER,
                message,
                egui::FontId::proportional(12.0),
                ui.visuals().error_fg_color,
            );
        }
        PlotOutput {
            response,
            redrawn: out.redrawn,
            bounds: self.bounds.unwrap_or(bounds),
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_ranges_are_rejected() {
        assert!(PlotBounds::new([0.0, 0.0], [0.0, 1.0]).is_none());
        assert!(PlotBounds::new([f64::NAN, 1.0], [0.0, 1.0]).is_none());
        assert!(PlotBounds::new([-f64::MAX, f64::MAX], [0.0, 1.0]).is_none());
    }
    #[test]
    fn coordinates_account_for_margins_and_inverted_y() {
        let b = PlotBounds::new([10.0, 30.0], [-2.0, 2.0]).unwrap();
        let r = egui::Rect::from_min_size(egui::pos2(56.0, 8.0), egui::vec2(200.0, 100.0));
        assert_eq!(b.point(r, r.left_bottom()), [10.0, -2.0]);
        assert_eq!(b.point(r, r.center()), [20.0, 0.0]);
        assert_eq!(b.point(r, r.right_top()), [30.0, 2.0]);
    }
    #[test]
    fn zoom_preserves_anchor_and_pan_preserves_span() {
        let b = PlotBounds::new([0.0, 10.0], [-5.0, 5.0]).unwrap();
        let z = b.zoom([2.0, 1.0], 0.5);
        assert_eq!(z.x(), [1.0, 6.0]);
        assert_eq!(z.y(), [-2.0, 3.0]);
        assert_eq!(b.pan(2.0, -1.0).x(), [2.0, 12.0]);
        assert_eq!(b.zoom([0.0, 0.0], f64::INFINITY), b);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod gpu_tests {
    use super::*;
    use std::sync::Arc;

    struct Solid {
        revision: u64,
    }
    impl PlotSource for Solid {
        fn revision(&self) -> u64 {
            self.revision
        }
        fn bounds(&self) -> PlotBounds {
            PlotBounds::new([0.0, 10.0], [0.0, 10.0]).unwrap()
        }
        fn draw<DB: DrawingBackend>(
            &self,
            chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
        ) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
            chart.draw_series(std::iter::once(Rectangle::new(
                [(2.0, 2.0), (8.0, 8.0)],
                RED.filled(),
            )))?;
            Ok(())
        }
    }

    // Explicitly opt in: this creates a real Vulkan device, including lavapipe.
    #[test]
    #[ignore = "requires a Vulkan adapter; run with --ignored"]
    fn vulkan_render_cache_hover_zoom_and_resize() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("Vulkan adapter");
        eprintln!("Vulkan adapter: {:?}", adapter.get_info());
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).expect("Vulkan device");
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let renderer = egui_wgpu::Renderer::new(&device, format, Default::default());
        let state = egui_wgpu::RenderState {
            adapter,
            available_adapters: vec![],
            device: device.clone(),
            queue,
            target_format: format,
            renderer: Arc::new(egui::mutex::RwLock::new(renderer)),
        };
        let host = Rc::new(IcedHost::new(&state));
        let mut pane = IcedPlotPane::new(host);
        let ctx = egui::Context::default();
        let mut source = Solid { revision: 1 };
        let frame =
            |pane: &mut IcedPlotPane, source: &Solid, events: Vec<egui::Event>, size, scale| {
                ctx.set_pixels_per_point(scale);
                let mut output = None;
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(640.0, 480.0),
                        )),
                        modifiers: events
                            .iter()
                            .find_map(|e| match e {
                                egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                                _ => None,
                            })
                            .unwrap_or_else(|| ctx.input(|i| i.modifiers)),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            output = Some(pane.show_sized(ui, size, source));
                        });
                    },
                );
                let output = output.unwrap();
                assert!(output.error.is_none(), "{:?}", output.error);
                output
            };
        let size = egui::vec2(400.0, 300.0);
        assert!(frame(&mut pane, &source, vec![], size, 1.0).redrawn);
        assert!(!frame(&mut pane, &source, vec![], size, 1.0).redrawn);
        assert!(
            !frame(
                &mut pane,
                &source,
                vec![egui::Event::PointerMoved(egui::pos2(180.0, 120.0))],
                size,
                1.0
            )
            .redrawn
        );
        let before = pane.bounds().unwrap();
        frame(&mut pane, &source, vec![egui::Event::Zoom(1.5)], size, 1.0);
        assert_ne!(pane.bounds().unwrap(), before);
        assert!(frame(&mut pane, &source, vec![], size, 1.0).redrawn);
        source.revision += 1;
        assert!(frame(&mut pane, &source, vec![], size, 1.0).redrawn);
        assert!(frame(&mut pane, &source, vec![], size, 2.0).redrawn);
        assert!(frame(&mut pane, &source, vec![], egui::vec2(350.0, 250.0), 2.0).redrawn);
        pane.reset();
        frame(&mut pane, &source, vec![], size, 1.0);
        assert_eq!(pane.bounds(), Some(source.bounds()));

        // Pan through egui events, then release outside the plot.
        let press = egui::pos2(160.0, 110.0);
        let release = egui::pos2(440.0, 330.0);
        let button = |pos, pressed, shift| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers {
                shift,
                ..Default::default()
            },
        };
        frame(
            &mut pane,
            &source,
            vec![egui::Event::PointerMoved(press)],
            size,
            1.0,
        );
        frame(
            &mut pane,
            &source,
            vec![button(press, true, false)],
            size,
            1.0,
        );
        frame(
            &mut pane,
            &source,
            vec![egui::Event::PointerMoved(release)],
            size,
            1.0,
        );
        frame(
            &mut pane,
            &source,
            vec![button(release, false, false)],
            size,
            1.0,
        );
        assert!(pane.drag.is_none());
        assert_ne!(pane.bounds(), Some(source.bounds()));
        pane.reset();
        frame(&mut pane, &source, vec![], size, 1.0);

        // Shift-drag box zoom, using the actual plotting area rather than pane edges.
        let end = egui::pos2(280.0, 220.0);
        frame(
            &mut pane,
            &source,
            vec![egui::Event::PointerMoved(press)],
            size,
            1.0,
        );
        frame(
            &mut pane,
            &source,
            vec![button(press, true, true)],
            size,
            1.0,
        );
        frame(
            &mut pane,
            &source,
            vec![egui::Event::PointerMoved(end)],
            size,
            1.0,
        );
        frame(
            &mut pane,
            &source,
            vec![button(end, false, true)],
            size,
            1.0,
        );
        let selected = pane.bounds().unwrap();
        assert!(selected.x()[1] - selected.x()[0] < 10.0);
        assert!(selected.y()[1] - selected.y()[0] < 10.0);

        // Independent panes retain their own ranges even with a shared host.
        let mut other = IcedPlotPane::new(pane.pane.host.clone());
        frame(&mut other, &source, vec![], size, 1.0);
        assert_eq!(other.bounds(), Some(source.bounds()));
        assert_eq!(pane.bounds(), Some(selected));
        ctx.set_visuals(egui::Visuals::light());
        assert!(frame(&mut other, &source, vec![], size, 1.0).redrawn);

        // Read the texture back: verify an actual red series pixel, not just a submission.
        let surface = other.pane.surface.as_ref().unwrap();
        let center = other.area.get().unwrap().center();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("plot pixel readback"),
            size: 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &surface.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: center.x as u32,
                    y: center.y as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        state.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let pixel = buffer.slice(..).get_mapped_range();
        assert!(
            pixel[0] > 200 && pixel[1] < 30 && pixel[2] < 30 && pixel[3] > 200,
            "expected opaque red series pixel, got {:?}",
            &pixel[..4]
        );
        drop(pixel);
        buffer.unmap();
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    }
}
