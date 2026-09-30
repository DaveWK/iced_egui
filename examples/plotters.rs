//! Two live plotters charts and an egui text pane in an `egui_tiles` layout.
//! Drag the tabs around: the charts are ordinary tiles.
//!
//! Run with `cargo run --example plotters --features fira-sans,plotters`.

use std::rc::Rc;
use std::time::Instant;

use iced_egui::{IcedHost, IcedPlotPane, PlotBounds, PlotSource};
use plotters::coord::types::RangedCoordf64;
use plotters::prelude::*;

struct Wave {
    phase: f64,
    frequency: f64,
    label: &'static str,
}

impl PlotSource for Wave {
    fn revision(&self) -> u64 {
        self.phase.to_bits()
    }
    fn bounds(&self) -> PlotBounds {
        PlotBounds::new([0.0, 10.0], [-1.2, 1.2]).unwrap()
    }
    fn draw<DB: DrawingBackend>(
        &self,
        chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    ) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
        let series = (0..500).map(|i| {
            let x = i as f64 / 50.0;
            (x, (x * self.frequency + self.phase).sin())
        });
        chart.draw_series(AreaSeries::new(
            series.clone(),
            0.0,
            RGBColor(70, 130, 180).mix(0.25),
        ))?;
        chart
            .draw_series(LineSeries::new(
                series,
                RGBColor(70, 130, 180).stroke_width(2),
            ))
            .map(|_| ())
    }
}

#[allow(clippy::large_enum_variant)]
enum Pane {
    Chart { pane: IcedPlotPane, wave: Wave },
    Notes,
}

struct Behavior {
    started: Instant,
}

impl egui_tiles::Behavior<Pane> for Behavior {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        match pane {
            Pane::Chart { wave, .. } => wave.label.into(),
            Pane::Notes => "Notes".into(),
        }
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _id: egui_tiles::TileId,
        pane: &mut Pane,
    ) -> egui_tiles::UiResponse {
        match pane {
            Pane::Chart { pane, wave } => {
                wave.phase = self.started.elapsed().as_secs_f64();
                pane.show(ui, wave);
                ui.ctx().request_repaint();
            }
            Pane::Notes => {
                ui.label("The two charts are plotters series drawn by Iced's wgpu renderer");
                ui.label("Drag tabs to dock. Drag charts to pan; wheel/pinch to zoom.");
                ui.label("Shift-drag selects a range; double-click resets. Hover for coordinates.");
            }
        }
        egui_tiles::UiResponse::None
    }
}

struct App {
    tree: egui_tiles::Tree<Pane>,
    behavior: Behavior,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .expect("run with the wgpu backend");
        let host = Rc::new(IcedHost::new(rs));

        let mut tiles = egui_tiles::Tiles::default();
        let sine = tiles.insert_pane(Pane::Chart {
            pane: IcedPlotPane::new(host.clone()),
            wave: Wave {
                phase: 0.0,
                frequency: 1.0,
                label: "sin(x + t)",
            },
        });
        let fast = tiles.insert_pane(Pane::Chart {
            pane: IcedPlotPane::new(host),
            wave: Wave {
                phase: 0.0,
                frequency: 3.0,
                label: "sin(3x + t)",
            },
        });
        let notes = tiles.insert_pane(Pane::Notes);
        let charts = tiles.insert_horizontal_tile(vec![sine, fast]);
        let root = tiles.insert_vertical_tile(vec![charts, notes]);

        Self {
            tree: egui_tiles::Tree::new("dashboard", root, tiles),
            behavior: Behavior {
                started: Instant::now(),
            },
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.tree.ui(&mut self.behavior, ui);
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
        "iced_egui: plotters",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
