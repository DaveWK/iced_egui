//! Plain Iced widgets (button, slider, checkbox) inside an egui window,
//! with their messages flowing back into egui-side state.
//!
//! Run with `cargo run --example widgets --features fira-sans`.

use std::rc::Rc;

use iced_egui::{IcedElement, IcedHost, IcedPane};
use iced_widget::{
    button, checkbox, column, container, pick_list, slider, text, text_input, tooltip,
};

#[derive(Clone, Debug)]
enum Msg {
    Bump,
    Edit(String),
    EditSecond(String),
    Chart(&'static str),
    Slide(f32),
    Toggle(bool),
}

struct App {
    pane: IcedPane<Msg>,
    host: Rc<IcedHost>,
    clipboard_error: Option<String>,
    second_text: String,
    chart: Option<&'static str>,
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
        #[cfg(all(feature = "primary-selection", target_os = "linux"))]
        let clipboard_error = host.enable_primary_selection().err();
        #[cfg(not(all(feature = "primary-selection", target_os = "linux")))]
        let clipboard_error = None;
        Self {
            pane: IcedPane::new(host.clone()),
            host,
            clipboard_error,
            second_text: "Tab or Shift+Tab between inputs".into(),
            chart: None,
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
        if let Some(error) = self.host.take_primary_selection_error() {
            self.clipboard_error = Some(error);
        }
        egui::SidePanel::left("egui").show(ctx, |ui| {
            ui.heading("egui side");
            ui.label("Try Tab, IME composition, copy/cut/paste, the dropdown, and tooltip.");
            ui.label("With primary-selection enabled: select text, then middle-click to paste.");
            if let Some(error) = &self.clipboard_error {
                ui.colored_label(egui::Color32::RED, error);
            }
            ui.text_edit_singleline(&mut self.egui_text);
            ui.label(format!("clicks: {}", self.clicks));
            ui.add(egui::Slider::new(&mut self.level, 0.0..=1.0).text("level (egui)"));
            ui.checkbox(&mut self.enabled, "enabled (egui)");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Iced side");
            let (clicks, level, enabled) = (self.clicks, self.level, self.enabled);
            let iced_text = self.iced_text.clone();
            let second_text = self.second_text.clone();
            let chart = self.chart;
            let view = || -> IcedElement<'_, Msg> {
                let mut bump = button("bump");
                if enabled {
                    bump = bump.on_press(Msg::Bump);
                }
                container(
                    column![
                        text(format!("clicks: {clicks}")),
                        text_input("Clipboard-enabled Iced input", &iced_text).on_input(Msg::Edit),
                        text_input("Second Iced input", &second_text).on_input(Msg::EditSecond),
                        tooltip(
                            bump,
                            "An Iced tooltip rendered over egui",
                            tooltip::Position::Right
                        ),
                        slider(0.0..=1.0, level, Msg::Slide).step(0.01_f32),
                        checkbox(enabled)
                            .label("enabled (iced)")
                            .on_toggle(Msg::Toggle),
                        pick_list(
                            ["Line", "Candlestick", "Histogram", "Heatmap", "Scatter"],
                            chart,
                            Msg::Chart
                        )
                        .placeholder("Chart type"),
                    ]
                    .spacing(12),
                )
                .padding(16)
                .into()
            };
            let out = self
                .pane
                .show_sized(ui, egui::vec2(ui.available_width(), 300.0), view);
            for msg in out.messages {
                match msg {
                    Msg::EditSecond(value) => self.second_text = value,
                    Msg::Chart(value) => self.chart = Some(value),
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
