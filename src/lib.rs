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
//! # Clipboard
//!
//! A clicked pane owns editing focus. Copy/cut use egui platform output;
//! paste reads the payload supplied by egui's Paste event. Clipboard input
//! is consumed only by the focused pane. Standard text clipboard is
//! supported. All egui logical keys and IME composition events are routed to
//! the focused pane, with Tab traversal, candidate positioning and cursor feedback.
//! Install a [`PrimarySelection`] provider for middle-click paste, or enable the
//! optional Linux `primary-selection` feature. Standard clipboard reads remain
//! event-scoped. Iced overlays use a separate viewport-sized texture while visible
//! so menus and tooltips can extend beyond the pane.
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

mod clipboard;
mod input;
mod overlay;
mod primary;
pub use primary::PrimarySelection;

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use iced_core::mouse::{self, ScrollDelta};
use iced_core::{Color, Element, Event, Point, Size, Theme, renderer};
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
    primary: primary::SharedPrimary,
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
            primary: Default::default(),
        }
    }

    /// Install a primary-selection provider shared by all panes on this host.
    pub fn set_primary_selection(&self, provider: impl PrimarySelection + 'static) {
        self.primary.borrow_mut().provider = Some(Box::new(provider));
    }

    /// Take the last primary-selection read/write error, if any.
    pub fn take_primary_selection_error(&self) -> Option<String> {
        self.primary.borrow_mut().error.take()
    }

    /// Enable native X11 or Wayland primary selection.
    ///
    /// Requires the `primary-selection` feature on Linux. Wayland support
    /// depends on the compositor exposing a supported data-control protocol.
    #[cfg(all(feature = "primary-selection", target_os = "linux"))]
    pub fn enable_primary_selection(&self) -> Result<(), String> {
        let clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
        self.set_primary_selection(primary::Native(clipboard));
        Ok(())
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
    overlay_surface: Option<Surface>,
    overlay_bounds: Option<egui::Rect>,
    overlay_interactive: bool,
    input_method: iced_core::InputMethod,
    mouse_interaction: mouse::Interaction,
    theme: Theme,
    clear_color: Option<Color>,
    redraw_on_demand: bool,
    redraw_requested: bool,
    scheduled_redraw: Option<iced_core::time::Instant>,
    hovered_last_frame: bool,
    focused_last_frame: bool,
    window_focused: bool,
    pending_focus: Option<egui::FocusDirection>,
    tab_backwards: bool,
    last_selection: Option<String>,
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
            overlay_surface: None,
            overlay_bounds: None,
            overlay_interactive: false,
            input_method: iced_core::InputMethod::Disabled,
            mouse_interaction: mouse::Interaction::None,
            theme: Theme::Light,
            clear_color: Some(Color::TRANSPARENT),
            redraw_on_demand: false,
            redraw_requested: true,
            scheduled_redraw: None,
            hovered_last_frame: false,
            focused_last_frame: false,
            window_focused: true,
            pending_focus: None,
            tab_backwards: false,
            last_selection: None,
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
        if let Some(backwards) = ui.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key: egui::Key::Tab,
                    pressed: true,
                    modifiers,
                    ..
                } => Some(modifiers.shift),
                _ => None,
            })
        }) {
            self.tab_backwards = backwards;
        }
        if let Some(direction) = self.pending_focus.take() {
            ui.memory_mut(|memory| memory.move_focus(direction));
        }
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let screen = ui.ctx().content_rect();
        let origin = if forward_input { screen.min } else { rect.min };
        let overlay_response = self.overlay_bounds.map(|bounds| {
            egui::Area::new(response.id.with("iced_overlay"))
                .order(egui::Order::Foreground)
                .fixed_pos(bounds.min)
                .movable(false)
                .interactable(self.overlay_interactive)
                .show(ui.ctx(), |ui| {
                    ui.allocate_exact_size(bounds.size(), egui::Sense::click_and_drag())
                        .1
                })
                .inner
        });
        let ppp = ui.ctx().pixels_per_point();
        let px = [
            ((rect.width() * ppp).round() as u32).max(1),
            ((rect.height() * ppp).round() as u32).max(1),
        ];

        // ── Input → Iced events ─────────────────────────────────────────
        let (events, cursor) = if forward_input {
            self.collect_events(
                ui,
                overlay_response
                    .as_ref()
                    .filter(|r| r.contains_pointer() || r.dragged())
                    .unwrap_or(&response),
                egui::Rect::from_min_size(origin, rect.size()),
            )
        } else {
            (Vec::new(), mouse::Cursor::Unavailable)
        };

        if forward_input
            && (response.contains_pointer()
                || overlay_response
                    .as_ref()
                    .is_some_and(|r| r.contains_pointer()))
            && ui.input(|i| i.pointer.any_pressed())
        {
            response.request_focus();
        }
        let focused = forward_input && ui.memory(|memory| memory.has_focus(response.id));
        let window_focused = ui.input(|i| i.focused);
        let gained_focus = focused && !self.focused_last_frame;
        let lost_focus = self.focused_last_frame && !focused;
        if lost_focus {
            self.last_selection = None;
        }
        if focused {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
        }
        self.focused_last_frame = focused;
        let mut events: Vec<clipboard::RoutedEvent> = events.into_iter().map(Into::into).collect();
        // An open popup also needs outside clicks to dismiss itself.
        if forward_input
            && self.overlay_bounds.is_some()
            && !response.contains_pointer()
            && !overlay_response
                .as_ref()
                .is_some_and(|r| r.contains_pointer())
            && ui.input(|i| i.pointer.any_pressed())
        {
            events.push(Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)).into());
        }
        let keyboard_events = clipboard::take_events(ui.ctx(), focused && window_focused);
        let select = events.iter().any(|e| match e.event {
            Event::Mouse(
                mouse::Event::ButtonPressed(mouse::Button::Left)
                | mouse::Event::ButtonReleased(mouse::Button::Left),
            ) => true,
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                ui.input(|i| i.pointer.primary_down())
            }
            _ => false,
        }) || !keyboard_events.is_empty();
        events.extend(keyboard_events);
        let mut clipboard = clipboard::EguiClipboard::new(ui.ctx());
        clipboard.primary = self.host.primary.clone();

        let must_redraw = self.redraw_requested
            || self
                .scheduled_redraw
                .is_some_and(|deadline| deadline <= iced_core::time::Instant::now())
            || lost_focus
            || gained_focus
            || (forward_input && (focused || self.overlay_bounds.is_some()))
            || !self.redraw_on_demand
            || !events.is_empty()
            || self.last_geometry != Some((px, ppp, rect.size()));

        let mut messages = Vec::new();
        let mut redrawn = false;

        if must_redraw {
            self.scheduled_redraw = None;
            let host = self.host.clone();
            let mut renderer = host.renderer.borrow_mut();
            let bounds = if forward_input {
                Size::new(screen.width(), screen.height())
            } else {
                Size::new(rect.width(), rect.height())
            };
            let pass = Rc::new(overlay::Pass::default());
            let pane_bounds = iced_core::Rectangle::new(
                Point::new(rect.min.x - origin.x, rect.min.y - origin.y),
                Size::new(rect.width(), rect.height()),
            );
            let mut build = || {
                if forward_input {
                    overlay::root(view(), pane_bounds, pass.clone())
                } else {
                    view()
                }
            };

            self.input_method = iced_core::InputMethod::Disabled;
            let cache = self.cache.take().unwrap_or_default();
            let mut interface = UserInterface::build(build(), bounds, cache, &mut *renderer);

            if lost_focus {
                interface.operate(
                    &*renderer,
                    &mut iced_core::widget::operation::focusable::unfocus::<()>(),
                );
            }
            if gained_focus && !ui.input(|i| i.pointer.any_pressed()) {
                let mut count = input::Focus::default();
                interface.operate(&*renderer, &mut count);
                let index = if self.tab_backwards {
                    count.count.checked_sub(1)
                } else {
                    Some(0)
                };
                interface.operate(
                    &*renderer,
                    &mut input::Focus {
                        set: Some(index),
                        ..Default::default()
                    },
                );
            }
            if forward_input {
                if (gained_focus || !self.window_focused) && window_focused {
                    events.push(Event::Window(iced_core::window::Event::Focused).into());
                }
                if lost_focus || (self.window_focused && !window_focused) {
                    events.push(Event::Window(iced_core::window::Event::Unfocused).into());
                }
                events.push(
                    Event::Window(iced_core::window::Event::RedrawRequested(
                        iced_core::time::Instant::now(),
                    ))
                    .into(),
                );
            }
            self.window_focused = window_focused;
            let mut entering_with_tab = gained_focus && !ui.input(|i| i.pointer.any_pressed());
            let mut events: std::collections::VecDeque<_> = events.into();
            while let Some(event) = events.pop_front() {
                if let Event::Keyboard(iced_core::keyboard::Event::KeyPressed {
                    key: iced_core::keyboard::Key::Named(iced_core::keyboard::key::Named::Tab),
                    modifiers,
                    ..
                }) = &event.event
                {
                    if entering_with_tab {
                        entering_with_tab = false;
                        continue;
                    }
                    let backwards = modifiers.shift();
                    let mut count = input::Focus::default();
                    interface.operate(&*renderer, &mut count);
                    let target = match (count.focused, backwards) {
                        (Some(index), true) => index.checked_sub(1),
                        (Some(index), false) if index + 1 < count.count => Some(index + 1),
                        (None, true) => count.count.checked_sub(1),
                        (None, false) if count.count > 0 => Some(0),
                        _ => None,
                    };
                    interface.operate(
                        &*renderer,
                        &mut input::Focus {
                            set: Some(target),
                            ..Default::default()
                        },
                    );
                    if target.is_none() {
                        self.pending_focus = Some(if backwards {
                            egui::FocusDirection::Previous
                        } else {
                            egui::FocusDirection::Next
                        });
                        ui.ctx().request_repaint();
                    }
                    continue;
                }

                if self.overlay_bounds.is_some()
                    && matches!(
                        &event.event,
                        Event::Keyboard(iced_core::keyboard::Event::KeyPressed {
                            key: iced_core::keyboard::Key::Named(
                                iced_core::keyboard::key::Named::Escape
                            ),
                            ..
                        })
                    )
                {
                    interface.update(
                        &[Event::Mouse(mouse::Event::ButtonPressed(
                            mouse::Button::Left,
                        ))],
                        mouse::Cursor::Unavailable,
                        &mut *renderer,
                        &mut clipboard,
                        &mut messages,
                    );
                }
                clipboard.begin_event(event.paste);
                let (state, statuses) = interface.update(
                    std::slice::from_ref(&event.event),
                    cursor,
                    &mut *renderer,
                    &mut clipboard,
                    &mut messages,
                );
                if matches!(
                    event.event,
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle))
                ) && focused
                    && statuses
                        .iter()
                        .all(|status| *status == iced_core::event::Status::Ignored)
                    && matches!(
                        &state,
                        user_interface::State::Updated {
                            mouse_interaction: mouse::Interaction::Text,
                            ..
                        }
                    )
                    && let Some(text) = host.primary.borrow_mut().read()
                {
                    // Only text widgets get the fallback. A middle-click on a
                    // button must never activate it as a synthetic left-click.
                    let mut paste = vec![
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)).into(),
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)).into(),
                    ];
                    paste.extend(clipboard::translate(
                        vec![egui::Event::Paste(text)],
                        ui.input(|i| i.modifiers),
                    ));
                    for event in paste.into_iter().rev() {
                        events.push_front(event);
                    }
                }
                self.feedback(ui, &state);
                if let user_interface::State::Outdated = state {
                    let cache = interface.into_cache();
                    interface = UserInterface::build(build(), bounds, cache, &mut *renderer);
                }
            }

            if select && focused && host.primary.borrow().provider.is_some() {
                // Ask the focused widget for its selection. Secure inputs decline
                // copying. This clipboard never emits standard clipboard output.
                clipboard.selection_only = true;
                for event in
                    clipboard::translate(vec![egui::Event::Copy], ui.input(|i| i.modifiers))
                {
                    interface.update(
                        &[event.event],
                        cursor,
                        &mut *renderer,
                        &mut clipboard,
                        &mut messages,
                    );
                }
                clipboard.selection_only = false;
                if clipboard.captured_selection != self.last_selection {
                    if let Some(text) = &clipboard.captured_selection {
                        host.primary.borrow_mut().write(text.clone());
                    }
                    self.last_selection = clipboard.captured_selection.take();
                }
            }
            // update([]) computes overlay layout even on an otherwise idle frame.
            let (state, _) =
                interface.update(&[], cursor, &mut *renderer, &mut clipboard, &mut messages);
            self.feedback(ui, &state);
            let style = renderer::Style {
                text_color: self.theme.palette().text,
            };
            interface.draw(&mut *renderer, &self.theme, &style, cursor);
            let clear_color = self.clear_color;
            let format = host.format;
            let surface = Self::surface(&host, &mut self.surface, px);
            let viewport = Viewport::with_physical_size(Size::new(px[0], px[1]), ppp);
            renderer.present(clear_color, format, &surface.view, &viewport);
            if forward_input && pass.active.get() {
                pass.overlay.set(true);
                interface.draw(&mut *renderer, &self.theme, &style, cursor);
                let size = [
                    (screen.width() * ppp).ceil().max(1.0) as u32,
                    (screen.height() * ppp).ceil().max(1.0) as u32,
                ];
                let surface = Self::surface(&host, &mut self.overlay_surface, size);
                renderer.present(
                    Some(Color::TRANSPARENT),
                    format,
                    &surface.view,
                    &Viewport::with_physical_size(Size::new(size[0], size[1]), ppp),
                );
                self.overlay_interactive = pass.interactive.get();
                self.overlay_bounds = pass.bounds.get().map(|b| {
                    egui::Rect::from_min_size(
                        origin + egui::vec2(b.x, b.y),
                        egui::vec2(b.width, b.height),
                    )
                });
            } else {
                self.overlay_bounds = None;
                if let Some(surface) = self.overlay_surface.take() {
                    Self::free_surface(&host, surface);
                }
            }
            self.cache = Some(interface.into_cache());

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

        if let Some(surface) = &self.overlay_surface {
            ui.ctx()
                .layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    response.id.with("iced_overlay"),
                ))
                .image(
                    surface.egui_id,
                    screen,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
        }
        if forward_input
            && (response.hovered() || overlay_response.as_ref().is_some_and(|r| r.hovered()))
        {
            ui.ctx()
                .set_cursor_icon(input::cursor(self.mouse_interaction));
        }
        if focused
            && window_focused
            && let iced_core::InputMethod::Enabled {
                cursor, preedit, ..
            } = &self.input_method
        {
            let caret = egui::Rect::from_min_size(
                origin + egui::vec2(cursor.x, cursor.y),
                egui::vec2(cursor.width, cursor.height),
            );
            ui.ctx().output_mut(|output| {
                output.ime = Some(egui::output::IMEOutput {
                    rect,
                    cursor_rect: caret,
                });
                output.mutable_text_under_cursor = true;
            });
            if let Some(preedit) = preedit
                && !preedit.content.is_empty()
            {
                egui::Area::new(response.id.with("iced_preedit"))
                    .order(egui::Order::Tooltip)
                    .fixed_pos(caret.left_bottom())
                    .interactable(false)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label(egui::RichText::new(&preedit.content).underline());
                        });
                    });
            }
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
        let hovered = response.hovered() || response.dragged() || response.drag_stopped();
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

            for (egui_button, iced_button) in [
                (egui::PointerButton::Primary, mouse::Button::Left),
                (egui::PointerButton::Secondary, mouse::Button::Right),
                (egui::PointerButton::Middle, mouse::Button::Middle),
                (egui::PointerButton::Extra1, mouse::Button::Other(4)),
                (egui::PointerButton::Extra2, mouse::Button::Other(5)),
            ] {
                if ui.input(|i| i.pointer.button_pressed(egui_button)) {
                    events.push(Event::Mouse(mouse::Event::ButtonPressed(iced_button)));
                }
                if ui.input(|i| i.pointer.button_released(egui_button)) {
                    events.push(Event::Mouse(mouse::Event::ButtonReleased(iced_button)));
                }
            }
            let scroll = ui.input(|i| i.smooth_scroll_delta);
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
    fn surface<'s>(host: &IcedHost, slot: &'s mut Option<Surface>, px: [u32; 2]) -> &'s Surface {
        let stale = slot.as_ref().is_none_or(|s| s.size != px);
        if stale {
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
            let egui_id = match slot.take() {
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

            *slot = Some(Surface {
                texture,
                view,
                size: px,
                egui_id,
            });
        }
        slot.as_ref().expect("surface just ensured")
    }
    fn free_surface(host: &IcedHost, surface: Surface) {
        host.egui_renderer.write().free_texture(&surface.egui_id);
        surface.texture.destroy();
    }
    fn feedback(&mut self, ui: &egui::Ui, state: &user_interface::State) {
        if let user_interface::State::Updated {
            mouse_interaction,
            redraw_request,
            input_method,
            ..
        } = state
        {
            self.mouse_interaction = *mouse_interaction;
            // Empty updates do not request IME; retain the redraw event's state.
            if input_method.is_enabled() {
                self.input_method = input_method.clone();
            }
            let now = iced_core::time::Instant::now();
            let deadline = match redraw_request {
                iced_core::window::RedrawRequest::NextFrame => Some(now),
                iced_core::window::RedrawRequest::At(time) => Some(*time),
                iced_core::window::RedrawRequest::Wait => None,
            };
            if let Some(deadline) = deadline {
                self.scheduled_redraw = Some(
                    self.scheduled_redraw
                        .map_or(deadline, |old| old.min(deadline)),
                );
                ui.ctx()
                    .request_repaint_after(deadline.saturating_duration_since(now));
            }
        }
    }
}

impl<Message> Drop for IcedPane<Message> {
    fn drop(&mut self) {
        for surface in [self.surface.take(), self.overlay_surface.take()]
            .into_iter()
            .flatten()
        {
            Self::free_surface(&self.host, surface);
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod integration_tests;
