//! Headless integration tests exercise real Iced widgets on a Vulkan device.
use super::*;
use std::cell::RefCell;

struct Selection(Rc<RefCell<String>>);
impl PrimarySelection for Selection {
    fn read(&mut self) -> Result<Option<String>, String> {
        Ok(Some(self.0.borrow().clone()))
    }
    fn write(&mut self, text: String) -> Result<(), String> {
        *self.0.borrow_mut() = text;
        Ok(())
    }
}
#[derive(Clone, Debug)]
enum Msg {
    First(String),
    Second(String),
    Pick(&'static str),
}
struct App {
    host: Rc<IcedHost>,
    pane: IcedPane<Msg>,
    next_pane: IcedPane<String>,
    next_value: String,
    two_panes: bool,
    tooltip: bool,
    tooltip_delay: std::time::Duration,
    ctx: egui::Context,
    first: String,
    second: String,
    other: String,
    selected: Option<&'static str>,
    rect: egui::Rect,
    other_rect: egui::Rect,
    selection: Rc<RefCell<String>>,
    popup: bool,
    secure: bool,
}
impl App {
    fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let renderer = egui_wgpu::Renderer::new(&device, format, Default::default());
        let host = Rc::new(IcedHost::new(&egui_wgpu::RenderState {
            instance,
            surface_config: egui_wgpu::SurfaceConfig::LOW_LATENCY,
            adapter,
            available_adapters: vec![],
            device,
            queue,
            target_format: format,
            renderer: Arc::new(egui::mutex::RwLock::new(renderer)),
        }));
        let selection = Rc::new(RefCell::new(String::new()));
        host.set_primary_selection(Selection(selection.clone()));
        Self {
            pane: IcedPane::new(host.clone()),
            next_pane: IcedPane::new(host.clone()),
            next_value: "next".into(),
            two_panes: false,
            tooltip: false,
            tooltip_delay: std::time::Duration::ZERO,
            host,
            ctx: egui::Context::default(),
            first: "alpha".into(),
            second: "beta".into(),
            other: String::new(),
            selected: None,
            rect: egui::Rect::NOTHING,
            other_rect: egui::Rect::NOTHING,
            selection,
            popup: false,
            secure: false,
        }
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let first = self.first.clone();
        let second = self.second.clone();
        let selected = self.selected;
        crate::run_test_ui(
            &self.ctx,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 480.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_space(40.0); // Exercise nonzero pane and IME origins.
                    let out = self.pane.show_sized(
                        ui,
                        egui::vec2(260.0, if self.popup { 32.0 } else { 80.0 }),
                        || {
                            if self.tooltip {
                                iced_widget::tooltip(
                                    iced_widget::button("Hover me").on_press(Msg::Pick("clicked")),
                                    iced_widget::container(iced_widget::text(
                                        "An Iced tooltip outside the pane",
                                    ))
                                    .padding(8)
                                    .style(iced_widget::container::rounded_box),
                                    iced_widget::tooltip::Position::Right,
                                )
                                .gap(180)
                                .delay(self.tooltip_delay)
                                .into()
                            } else if self.popup {
                                iced_widget::pick_list(
                                    ["one", "two", "three", "four"],
                                    selected,
                                    Msg::Pick,
                                )
                                .placeholder("Choose")
                                .into()
                            } else {
                                iced_widget::column![
                                    iced_widget::text_input("first", &first)
                                        .secure(self.secure)
                                        .on_input(Msg::First),
                                    iced_widget::text_input("second", &second)
                                        .on_input(Msg::Second),
                                ]
                                .spacing(6)
                                .into()
                            }
                        },
                    );
                    self.rect = out.response.rect;
                    for msg in out.messages {
                        match msg {
                            Msg::First(s) => self.first = s,
                            Msg::Second(s) => self.second = s,
                            Msg::Pick(s) => self.selected = Some(s),
                        }
                    }
                    if self.two_panes {
                        let value = self.next_value.clone();
                        let out = self.next_pane.show_sized(ui, egui::vec2(260.0, 40.0), || {
                            iced_widget::text_input("next pane", &value)
                                .on_input(|s| s)
                                .into()
                        });
                        if let Some(value) = out.messages.last() {
                            self.next_value = value.clone();
                        }
                    }
                    self.other_rect = ui.text_edit_singleline(&mut self.other).rect;
                });
            },
        )
    }
    fn click(&mut self, p: egui::Pos2, button: egui::PointerButton) {
        self.frame(vec![egui::Event::PointerMoved(p), pointer(p, button, true)]);
        self.frame(vec![pointer(p, button, false)]);
    }
    fn pixel(&self, surface: &Surface, point: egui::Pos2) -> [u8; 4] {
        let buffer = self.host.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.host.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &surface.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: point.x as u32,
                    y: point.y as u32,
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
        self.host.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        self.host
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        rx.recv().unwrap().unwrap();
        let data = buffer
            .slice(..)
            .get_mapped_range()
            .expect("readback mapped");
        let pixel = data[..4].try_into().unwrap();
        drop(data);
        buffer.unmap();
        pixel
    }
}
fn pointer(pos: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Default::default(),
    }
}
fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}
fn tab(back: bool) -> egui::Event {
    key(
        egui::Key::Tab,
        egui::Modifiers {
            shift: back,
            ..Default::default()
        },
    )
}
#[test]
#[ignore = "requires a Vulkan adapter"]
fn ime_tab_cursor_and_primary_selection() {
    let mut app = App::new();
    app.frame(vec![]);
    let p = app.rect.min + egui::vec2(15.0, 15.0);
    app.click(p, egui::PointerButton::Primary);
    let output = app.frame(vec![egui::Event::PointerMoved(p)]);
    assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Text);
    let ime = output
        .platform_output
        .ime
        .expect("focused Iced text input requests IME");
    assert!(app.rect.contains(ime.cursor_rect.center()));
    app.frame(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    assert_eq!(&*app.selection.borrow(), "alpha");
    let output = app.frame(vec![egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "にほん".into(),
        active_range_chars: None,
    })]);
    assert_eq!(app.first, "alpha");
    assert!(output.platform_output.ime.is_some());
    app.frame(vec![
        egui::Event::Ime(egui::ImeEvent::Commit("日本🦀".into())),
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        }),
    ]);
    assert_eq!(app.first, "日本🦀");
    app.frame(vec![tab(false)]);
    app.frame(vec![
        key(egui::Key::A, egui::Modifiers::COMMAND),
        egui::Event::Paste("second".into()),
    ]);
    assert_eq!(app.second, "second");
    assert_eq!(app.first, "日本🦀");
    app.frame(vec![tab(true)]);
    app.frame(vec![
        key(egui::Key::A, egui::Modifiers::COMMAND),
        egui::Event::Paste("first".into()),
    ]);
    assert_eq!(app.first, "first");
    *app.selection.borrow_mut() = "PRIMARY".into();
    app.click(
        app.rect.min + egui::vec2(220.0, 15.0),
        egui::PointerButton::Middle,
    );
    assert_eq!(app.first, "firstPRIMARY");
    let output = app.frame(vec![]);
    assert!(output.platform_output.commands.is_empty());
    app.frame(vec![tab(false)]);
    app.frame(vec![tab(false)]);
    app.frame(vec![]);
    app.frame(vec![egui::Event::Paste("egui".into())]);
    assert_eq!(app.other, "egui");
    assert_eq!(app.second, "second");
    // Shift+Tab enters the last Iced field from the following egui editor.
    app.frame(vec![tab(true)]);
    app.frame(vec![]);
    app.frame(vec![
        key(egui::Key::A, egui::Modifiers::COMMAND),
        egui::Event::Paste("back".into()),
    ]);
    assert_eq!(app.second, "back");
}
#[test]
#[ignore = "requires a Vulkan adapter"]
fn pick_list_extends_beyond_pane_and_selects() {
    let mut app = App::new();
    app.popup = true;
    app.frame(vec![]);
    let p = app.rect.min + egui::vec2(20.0, 15.0);
    app.click(p, egui::PointerButton::Primary);
    app.frame(vec![]); // Register foreground hit area after popup opens.
    let bounds = app.pane.overlay_bounds.expect("pick list opens");
    assert!(
        bounds.bottom() > app.rect.bottom() + 30.0,
        "{bounds:?} vs {:?}",
        app.rect
    );
    let point = egui::pos2(bounds.left() + 10.0, bounds.bottom() - 12.0);
    let pixel = app.pixel(app.pane.overlay_surface.as_ref().unwrap(), point);
    assert!(
        pixel[3] > 200,
        "popup has visible pixels outside pane: {pixel:?}"
    );
    app.click(point, egui::PointerButton::Primary);
    assert_eq!(app.selected, Some("four"));
    assert!(app.pane.overlay_bounds.is_none());
    app.click(p, egui::PointerButton::Primary);
    assert!(app.pane.overlay_bounds.is_some());
    app.frame(vec![key(egui::Key::Escape, Default::default())]);
    assert!(app.pane.overlay_bounds.is_none(), "Escape dismisses popup");
    app.click(p, egui::PointerButton::Primary);
    app.click(egui::pos2(550.0, 350.0), egui::PointerButton::Primary);
    assert!(
        app.pane.overlay_bounds.is_none(),
        "outside click dismisses popup"
    );
}
#[test]
#[ignore = "requires a Vulkan adapter"]
fn password_selection_does_not_replace_primary() {
    let mut app = App::new();
    app.secure = true;
    app.frame(vec![]);
    *app.selection.borrow_mut() = "keep".into();
    app.click(
        app.rect.min + egui::vec2(10.0, 15.0),
        egui::PointerButton::Primary,
    );
    app.frame(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    assert_eq!(&*app.selection.borrow(), "keep");
}

#[test]
#[ignore = "requires a Vulkan adapter"]
fn tab_between_panes_and_tooltip_hover() {
    let mut app = App::new();
    app.two_panes = true;
    app.frame(vec![]);
    app.click(
        app.rect.min + egui::vec2(10.0, 15.0),
        egui::PointerButton::Primary,
    );
    app.frame(vec![tab(false), tab(false)]);
    app.frame(vec![]);
    app.frame(vec![
        key(egui::Key::A, egui::Modifiers::COMMAND),
        egui::Event::Paste("next pane owns paste".into()),
    ]);
    assert_eq!(app.next_value, "next pane owns paste");
    assert_eq!(app.first, "alpha");
    assert_eq!(app.second, "beta");
    app.frame(vec![tab(true)]);
    app.frame(vec![]);
    app.frame(vec![]);
    app.frame(vec![
        key(egui::Key::A, egui::Modifiers::COMMAND),
        egui::Event::Paste("previous pane".into()),
    ]);
    assert_eq!(app.second, "previous pane");
    app.tooltip = true;
    let point = app.rect.min + egui::vec2(20.0, 15.0);
    app.frame(vec![egui::Event::PointerMoved(point)]);
    app.frame(vec![]);
    app.click(point, egui::PointerButton::Middle);
    assert_eq!(
        app.selected, None,
        "middle-click must not activate a button"
    );
    let bounds = app.pane.overlay_bounds.expect("tooltip opens");
    assert!(bounds.right() > app.rect.right());
    assert!(
        !app.pane.overlay_interactive,
        "tooltip should not intercept input"
    );
    assert_eq!(
        app.pixel(
            app.pane.overlay_surface.as_ref().unwrap(),
            egui::pos2(550.0, 400.0)
        )[3],
        0,
        "overlay background stays transparent"
    );
    app.frame(vec![egui::Event::PointerMoved(egui::pos2(550.0, 400.0))]);
    assert!(
        app.pane.overlay_surface.is_none(),
        "tooltip surface released on exit"
    );
}

#[test]
#[ignore = "requires a Vulkan adapter"]
fn primary_drag_selection_releases_outside_pane() {
    let mut app = App::new();
    app.frame(vec![]);
    let start = app.rect.min + egui::vec2(6.0, 15.0);
    let end = app.rect.min + egui::vec2(350.0, 15.0);
    app.frame(vec![
        egui::Event::PointerMoved(start),
        pointer(start, egui::PointerButton::Primary, true),
    ]);
    app.frame(vec![egui::Event::PointerMoved(end)]);
    app.frame(vec![pointer(end, egui::PointerButton::Primary, false)]);
    assert_eq!(&*app.selection.borrow(), "alpha");
    // Once released, movement alone cannot change selection or reclaim PRIMARY.
    *app.selection.borrow_mut() = "external owner".into();
    app.frame(vec![egui::Event::PointerMoved(start)]);
    assert_eq!(&*app.selection.borrow(), "external owner");
    let output = app.frame(vec![egui::Event::Copy]);
    assert!(
        output.platform_output.commands.iter().any(
            |command| matches!(command, egui::OutputCommand::CopyText(text) if text == "alpha")
        )
    );
}

#[test]
#[ignore = "requires a Vulkan adapter"]
fn delayed_tooltip_wakes_demand_rendering() {
    let mut app = App::new();
    app.tooltip = true;
    app.tooltip_delay = std::time::Duration::from_millis(20);
    app.pane.set_redraw_on_demand(true);
    app.frame(vec![]);
    app.frame(vec![egui::Event::PointerMoved(
        app.rect.min + egui::vec2(15.0, 15.0),
    )]);
    assert!(
        app.pane.scheduled_redraw.is_some(),
        "Iced deadline is retained"
    );
    std::thread::sleep(std::time::Duration::from_millis(30));
    app.frame(vec![]);
    assert!(
        app.pane.overlay_bounds.is_some(),
        "scheduled frame opens unfocused tooltip without more input"
    );
}
