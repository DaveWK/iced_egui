//! Plain Iced widgets (button, slider, checkbox) inside an egui window,
//! with their messages flowing back into egui-side state.
//!
//! Run with `cargo run --example widgets --features fira-sans`.

use std::rc::Rc;

use iced_egui::{IcedElement, IcedHost, IcedPane};
use iced_widget::{button, checkbox, column, container, slider, text, text_input};

#[derive(Clone, Debug)]
enum Msg {
    Bump,
    Edit(String),
    Slide(f32),
    Toggle(bool),
}

struct App {
    pane: IcedPane<Msg>,
    clicks: u32,
    level: f32,
    enabled: bool,
    iced_text: String,
    egui_text: String,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .expect("run with the wgpu backend");
        let host = Rc::new(IcedHost::new(rs));
        Self {
            pane: IcedPane::new(host),
            clicks: 0,
            level: 0.5,
            enabled: true,
            iced_text: "Copy or cut this Iced text".into(),
            egui_text: "Paste between egui and Iced".into(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("egui").show(ctx, |ui| {
            ui.heading("egui side");
            ui.text_edit_singleline(&mut self.egui_text);
            ui.label(format!("clicks: {}", self.clicks));
            ui.add(egui::Slider::new(&mut self.level, 0.0..=1.0).text("level (egui)"));
            ui.checkbox(&mut self.enabled, "enabled (egui)");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Iced side");
            let (clicks, level, enabled) = (self.clicks, self.level, self.enabled);
            let iced_text = self.iced_text.clone();
            let view = || -> IcedElement<'_, Msg> {
                let mut bump = button("bump");
                if enabled {
                    bump = bump.on_press(Msg::Bump);
                }
                container(
                    column![
                        text(format!("clicks: {clicks}")),
                        text_input("Clipboard-enabled Iced input", &iced_text).on_input(Msg::Edit),
                        bump,
                        slider(0.0..=1.0, level, Msg::Slide).step(0.01_f32),
                        checkbox(enabled)
                            .label("enabled (iced)")
                            .on_toggle(Msg::Toggle),
                    ]
                    .spacing(12),
                )
                .padding(16)
                .into()
            };
            let out = self.pane.show(ui, view);
            for msg in out.messages {
                match msg {
                    Msg::Edit(value) => self.iced_text = value,
                    Msg::Bump => self.clicks += 1,
                    Msg::Slide(v) => self.level = v,
                    Msg::Toggle(v) => self.enabled = v,
                }
            }
        });
    }
}

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "iced_egui: widgets",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
