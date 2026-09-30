use std::rc::Rc;
use iced_egui::{IcedElement, IcedHost, IcedPane};
use iced_widget::{button, column, text};

#[derive(Clone, Debug)]
enum Msg { Bump }

struct App { pane: IcedPane<Msg>, n: u32 }

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let n = self.n;
            let out = self.pane.show(ui, || -> IcedElement<'static, Msg> {
                column![text(format!("{n}")), button("bump").on_press(Msg::Bump)].into()
            });
            for m in out.messages { match m { Msg::Bump => self.n += 1 } }
        });
    }
}

fn main() {
    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async move {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document.get_element_by_id("canvas").unwrap();
        let canvas: web_sys::HtmlCanvasElement = wasm_bindgen::JsCast::dyn_into(canvas).unwrap();
        eframe::WebRunner::new()
            .start(canvas, web_options, Box::new(|cc| {
                let rs = cc.wgpu_render_state.as_ref().expect("wgpu");
                let host = Rc::new(IcedHost::new(rs));
                Ok(Box::new(App { pane: IcedPane::new(host), n: 0 }))
            }))
            .await
            .expect("start");
    });
}
