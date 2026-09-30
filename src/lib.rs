//! Render [Iced](https://iced.rs) widget trees inside [egui](https://egui.rs).
//!
//! `iced_egui` runs Iced's wgpu renderer headless on the **same `wgpu::Device`
//! egui is drawing with**, presents an Iced [`UserInterface`] into an
//! offscreen texture, and paints that texture as an ordinary egui image. From
//! egui's point of view an [`IcedPane`] is just another widget: it takes the
//! rect you give it, it can live in a panel, a window, or an `egui_tiles`
//! pane, and it can be dragged around like anything else.
//!
//! The motivating use is charts: [`plotters-iced2`](https://crates.io/crates/plotters-iced2)
//! draws [plotters](https://crates.io/crates/plotters) charts into an Iced
//! canvas, so with this crate every plotters series type is available inside
//! an egui dashboard, tessellated by Iced and drawn by wgpu.
//!
//! # Requirements
//!
//! - egui must be running on its wgpu backend (`eframe` with the `wgpu`
//!   feature, `Renderer::Wgpu`). The glow backend has no device to share.
//! - `iced_renderer`'s `tiny-skia` feature must be **off** in your dependency
//!   graph (it is off in this crate). With it off,
//!   `iced_widget::renderer::Renderer` *is* `iced_wgpu::Renderer`, which is the
//!   type this crate builds interfaces for and the one `plotters-iced2`
//!   implements its renderer trait on. The `iced` facade crate enables
//!   `tiny-skia` by default, so depend on `iced_widget` / `iced_runtime`
//!   directly or on `iced` with `default-features = false`.
//! - On the web, enable the `fira-sans` feature (or load a font with
//!   [`IcedHost::load_font`]) or Iced text will be invisible: there are no
//!   system fonts in a browser.
//!
//! # Example
//!
//! ```no_run
//! use iced_egui::{IcedHost, IcedPane};
//! use iced_widget::{button, column, text};
//! use std::rc::Rc;
//!
//! #[derive(Clone, Debug)]
//! enum Msg { Bump }
//!
//! struct App { host: Rc<IcedHost>, pane: IcedPane<Msg>, n: u32 }
//!
//! impl App {
//!     fn new(cc: &eframe::CreationContext<'_>) -> Self {
//!         let rs = cc.wgpu_render_state.as_ref().expect("run eframe with the wgpu backend");
//!         let host = Rc::new(IcedHost::new(rs));
//!         Self { pane: IcedPane::new(host.clone()), host, n: 0 }
//!     }
//! }
//!
//! impl eframe::App for App {
//!     fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
//!         egui::CentralPanel::default().show(ctx, |ui| {
//!             let n = self.n;
//!             let out = self.pane.show(ui, || {
//!                 column![text(format!("clicked {n} times")), button("bump").on_press(Msg::Bump)].into()
//!             });
//!             for m in out.messages { match m { Msg::Bump => self.n += 1 } }
//!         });
//!     }
//! }
//! ```
//!
//! # How a frame works
//!
//! [`IcedPane::show`] does, in order:
//!
//! 1. Allocates a rect in the current [`egui::Ui`] and reads egui's pointer
//!    input for it, translating it to Iced mouse events in the pane's
//!    coordinate space.
//! 2. Builds a [`UserInterface`] from your `view` closure, runs
//!    [`UserInterface::update`] with those events (collecting any messages
//!    your widgets emit), and rebuilds once if Iced reports the tree outdated.
//! 3. Draws the interface with the shared [`iced_wgpu::Renderer`] and presents
//!    it to the pane's offscreen texture, recreating the texture when the rect
//!    or the pixel density changed.
//! 4. Paints the texture into the rect with `ui.painter().image(..)`, so egui
//!    clips, layers and composites it like any other image.
//!
//! Presenting submits to the wgpu queue during egui's UI pass. egui's own
//! frame is submitted afterwards, and queue submissions execute in order, so
//! the texture is complete before egui samples it.
//!
//! # Redraw cost
//!
//! By default the Iced interface is rebuilt and presented on every egui
//! frame, which is always correct. egui only repaints when something changed,
//! so this is usually fine. For heavy content, keep tessellation out of the
//! per-frame path with Iced's own [`iced_widget::canvas::Cache`] (which
//! `plotters-iced2` supports through `Chart::draw_cache`), or call
//! [`IcedPane::set_redraw_on_demand`] and [`IcedPane::request_redraw`] to
//! present only when you say so. Input events always force a redraw.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use iced_core::mouse::{self, ScrollDelta};
use iced_core::{Color, Element, Event, Point, Size, Theme, clipboard, renderer};
use iced_graphics::{Antialiasing, Shell, Viewport};
use iced_runtime::user_interface::{self, UserInterface};

#[cfg(feature = "plotters")]
mod plot;
#[cfg(feature = "plotters")]
pub use plot::{IcedPlotPane, PlotBounds, PlotOutput, PlotSource};
#[cfg(feature = "plotters")]
pub use plotters;

pub use iced_core;
pub use iced_runtime;
pub use iced_wgpu;
pub use iced_widget;

/// The Iced renderer type this crate builds interfaces for.
///
/// This is `iced_widget::renderer::Renderer`, which resolves to
/// [`iced_wgpu::Renderer`] when `iced_renderer`'s `tiny-skia` feature is off.
/// It is the type `plotters-iced2` implements its `Renderer` trait on.
pub type Renderer = iced_widget::renderer::Renderer;

/// An Iced element for this crate's [`Renderer`] and Iced's default [`Theme`].
pub type IcedElement<'a, Message> = Element<'a, Message, Theme, Renderer>;

/// Shared Iced rendering state on egui's wgpu device.
///
/// Create one from eframe's [`egui_wgpu::RenderState`] and hand an
/// `Rc<IcedHost>` to every [`IcedPane`]. All panes share one
/// [`iced_wgpu::Renderer`] (and so one set of pipelines and one text atlas).
pub struct IcedHost {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    egui_renderer: Arc<egui::mutex::RwLock<egui_wgpu::Renderer>>,
    renderer: RefCell<Renderer>,
}

impl IcedHost {
    /// Build a host on egui's device with 4x MSAA and Iced's default font.
    pub fn new(render_state: &egui_wgpu::RenderState) -> Self {
        Self::with_antialiasing(render_state, Some(Antialiasing::MSAAx4))
    }

    /// Build a host with a specific antialiasing setting (`None` for none).
    pub fn with_antialiasing(
        render_state: &egui_wgpu::RenderState,
        antialiasing: Option<Antialiasing>,
    ) -> Self {
        let device = render_state.device.clone();
        let queue = render_state.queue.clone();
        let format = render_state.target_format;
        let engine = iced_wgpu::Engine::new(
            &render_state.adapter,
            device.clone(),
            queue.clone(),
            format,
            antialiasing,
            Shell::headless(),
        );
        let renderer =
            iced_wgpu::Renderer::new(engine, iced_core::Font::default(), iced_core::Pixels(16.0));
        Self {
            device,
            queue,
            format,
            egui_renderer: render_state.renderer.clone(),
            renderer: RefCell::new(renderer),
        }
    }

    /// Register a font with Iced's text system, e.g. the same TTF you gave egui.
    pub fn load_font(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        iced_graphics::text::font_system()
            .write()
            .expect("iced font system lock")
            .load_font(bytes.into());
    }

    /// The texture format panes render in (egui's target format).
    pub fn texture_format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// The wgpu device shared with egui.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The wgpu queue shared with egui.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}

/// The offscreen texture a pane presents into, registered with egui.
struct Surface {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: [u32; 2],
    egui_id: egui::TextureId,
}

/// What [`IcedPane::show`] hands back.
pub struct IcedOutput<Message> {
    /// Messages your Iced widgets produced this frame, in order.
    pub messages: Vec<Message>,
    /// egui's response for the pane's rect (hover, drag, click, context menu...).
    pub response: egui::Response,
    /// Whether the Iced interface was re-presented this frame.
    pub redrawn: bool,
}

/// An Iced interface living in an egui rect.
///
/// One `IcedPane` owns one offscreen texture and one Iced widget-state
/// cache. Keep it in your app state and call [`IcedPane::show`] each frame.
pub struct IcedPane<Message> {
    host: Rc<IcedHost>,
    cache: Option<user_interface::Cache>,
    surface: Option<Surface>,
    theme: Theme,
    clear_color: Option<Color>,
    redraw_on_demand: bool,
    redraw_requested: bool,
    hovered_last_frame: bool,
    last_cursor: Option<Point>,
    last_geometry: Option<([u32; 2], f32, egui::Vec2)>,
    _marker: std::marker::PhantomData<Message>,
}

impl<Message> IcedPane<Message> {
    /// A pane on `host` with Iced's light theme and a transparent background.
    pub fn new(host: Rc<IcedHost>) -> Self {
        Self {
            host,
            cache: Some(user_interface::Cache::new()),
            surface: None,
            theme: Theme::Light,
            clear_color: Some(Color::TRANSPARENT),
            redraw_on_demand: false,
            redraw_requested: true,
            hovered_last_frame: false,
            last_cursor: None,
            last_geometry: None,
            _marker: std::marker::PhantomData,
        }
    }

    /// Use an Iced theme for this pane's widgets. Default: [`Theme::Light`].
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.request_redraw();
    }

    /// The pane's Iced theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Colour the texture is cleared to before Iced draws. `None` keeps the
    /// previous contents. Default: transparent, so egui's background shows
    /// through wherever Iced draws nothing.
    pub fn set_clear_color(&mut self, color: Option<Color>) {
        self.clear_color = color;
        self.request_redraw();
    }

    /// When `true`, the interface is only re-presented after
    /// [`request_redraw`](Self::request_redraw), on input, or when the rect
    /// changes. When `false` (the default) it is re-presented every frame.
    pub fn set_redraw_on_demand(&mut self, on_demand: bool) {
        self.redraw_on_demand = on_demand;
    }

    /// Present again on the next [`show`](Self::show), regardless of policy.
    pub fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    /// Lay out, update, draw and paint the interface into the available rect.
    ///
    /// `view` is called at least once per frame to build the widget tree; it
    /// may be called twice when Iced reports the tree outdated after an
    /// update. Messages emitted by the widgets are returned, not dispatched.
    pub fn show<'a>(
        &mut self,
        ui: &mut egui::Ui,
        view: impl FnMut() -> IcedElement<'a, Message>,
    ) -> IcedOutput<Message> {
        let size = ui.available_size();
        self.show_sized(ui, size, view)
    }

    /// [`show`](Self::show) with an explicit size in egui points.
    pub fn show_sized<'a>(
        &mut self,
        ui: &mut egui::Ui,
        size: egui::Vec2,
        view: impl FnMut() -> IcedElement<'a, Message>,
    ) -> IcedOutput<Message> {
        self.show_inner(ui, size, true, view)
    }

    fn show_inner<'a>(
        &mut self,
        ui: &mut egui::Ui,
        size: egui::Vec2,
        forward_input: bool,
        mut view: impl FnMut() -> IcedElement<'a, Message>,
    ) -> IcedOutput<Message> {
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let px = [
            ((rect.width() * ppp).round() as u32).max(1),
            ((rect.height() * ppp).round() as u32).max(1),
        ];

        // ── Input → Iced events ─────────────────────────────────────────
        let (events, cursor) = if forward_input {
            self.collect_events(ui, &response, rect)
        } else {
            (Vec::new(), mouse::Cursor::Unavailable)
        };

        let must_redraw = self.redraw_requested
            || !self.redraw_on_demand
            || !events.is_empty()
            || self.last_geometry != Some((px, ppp, rect.size()));

        let mut messages = Vec::new();
        let mut redrawn = false;

        if must_redraw {
            let host = self.host.clone();
            let mut renderer = host.renderer.borrow_mut();
            let bounds = Size::new(rect.width(), rect.height());

            let cache = self.cache.take().unwrap_or_default();
            let mut interface = UserInterface::build(view(), bounds, cache, &mut *renderer);

            let (state, _statuses) = interface.update(
                &events,
                cursor,
                &mut *renderer,
                &mut clipboard::Null,
                &mut messages,
            );
            if let user_interface::State::Outdated = state {
                let cache = interface.into_cache();
                interface = UserInterface::build(view(), bounds, cache, &mut *renderer);
            }

            let style = renderer::Style {
                text_color: self.theme.palette().text,
            };
            interface.draw(&mut *renderer, &self.theme, &style, cursor);
            self.cache = Some(interface.into_cache());

            let clear_color = self.clear_color;
            let format = host.format;
            let surface = self.ensure_surface(px);
            let viewport = Viewport::with_physical_size(Size::new(px[0], px[1]), ppp);
            renderer.present(clear_color, format, &surface.view, &viewport);

            self.last_geometry = Some((px, ppp, rect.size()));
            self.redraw_requested = false;
            redrawn = true;
        }

        if let Some(surface) = &self.surface {
            ui.painter().image(
                surface.egui_id,
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        IcedOutput {
            messages,
            response,
            redrawn,
        }
    }

    /// Translate this frame's egui pointer input on `rect` into Iced events.
    fn collect_events(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
    ) -> (Vec<Event>, mouse::Cursor) {
        let mut events = Vec::new();
        let hovered = response.hovered() || response.dragged();
        let pointer_pos = ui.input(|i| i.pointer.latest_pos());

        let cursor_point = pointer_pos.map(|p| Point::new(p.x - rect.min.x, p.y - rect.min.y));

        if hovered && !self.hovered_last_frame {
            events.push(Event::Mouse(mouse::Event::CursorEntered));
        }
        if !hovered && self.hovered_last_frame {
            events.push(Event::Mouse(mouse::Event::CursorLeft));
        }
        self.hovered_last_frame = hovered;

        if hovered {
            if let Some(p) = cursor_point {
                if self.last_cursor.is_none_or(|last| last != p) {
                    events.push(Event::Mouse(mouse::Event::CursorMoved { position: p }));
                }
                self.last_cursor = Some(p);
            }

            let (pressed, released, secondary_pressed, secondary_released, scroll) =
                ui.input(|i| {
                    (
                        i.pointer.button_pressed(egui::PointerButton::Primary),
                        i.pointer.button_released(egui::PointerButton::Primary),
                        i.pointer.button_pressed(egui::PointerButton::Secondary),
                        i.pointer.button_released(egui::PointerButton::Secondary),
                        i.smooth_scroll_delta,
                    )
                });
            if pressed {
                events.push(Event::Mouse(mouse::Event::ButtonPressed(
                    mouse::Button::Left,
                )));
            }
            if released {
                events.push(Event::Mouse(mouse::Event::ButtonReleased(
                    mouse::Button::Left,
                )));
            }
            if secondary_pressed {
                events.push(Event::Mouse(mouse::Event::ButtonPressed(
                    mouse::Button::Right,
                )));
            }
            if secondary_released {
                events.push(Event::Mouse(mouse::Event::ButtonReleased(
                    mouse::Button::Right,
                )));
            }
            if scroll != egui::Vec2::ZERO {
                events.push(Event::Mouse(mouse::Event::WheelScrolled {
                    delta: ScrollDelta::Pixels {
                        x: scroll.x,
                        y: scroll.y,
                    },
                }));
            }
        } else {
            self.last_cursor = None;
        }

        let cursor = match (hovered, cursor_point) {
            (true, Some(p)) => mouse::Cursor::Available(p),
            _ => mouse::Cursor::Unavailable,
        };
        (events, cursor)
    }

    /// (Re)create the offscreen texture for `px` pixels and keep egui's
    /// registration of it current.
    fn ensure_surface(&mut self, px: [u32; 2]) -> &Surface {
        let stale = self.surface.as_ref().is_none_or(|s| s.size != px);
        if stale {
            let host = &self.host;
            let texture = host.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("iced_egui pane"),
                size: wgpu::Extent3d {
                    width: px[0],
                    height: px[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: host.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            let mut egui_renderer = host.egui_renderer.write();
            let egui_id = match self.surface.take() {
                Some(old) => {
                    egui_renderer.update_egui_texture_from_wgpu_texture(
                        &host.device,
                        &view,
                        wgpu::FilterMode::Linear,
                        old.egui_id,
                    );
                    old.texture.destroy();
                    old.egui_id
                }
                None => egui_renderer.register_native_texture(
                    &host.device,
                    &view,
                    wgpu::FilterMode::Linear,
                ),
            };
            drop(egui_renderer);

            self.surface = Some(Surface {
                texture,
                view,
                size: px,
                egui_id,
            });
        }
        self.surface.as_ref().expect("surface just ensured")
    }
}

impl<Message> Drop for IcedPane<Message> {
    fn drop(&mut self) {
        if let Some(surface) = self.surface.take() {
            self.host
                .egui_renderer
                .write()
                .free_texture(&surface.egui_id);
            surface.texture.destroy();
        }
    }
}
