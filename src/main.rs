#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod diagram;
mod electrical;
mod export;
mod help;
mod interfaces;
mod interface_catalog;
mod interface_review;
mod interface_ui;
mod model;
mod power;
mod power_report;
mod power_ui;
mod review;
mod rail;
mod rail_ui;
mod storage;
mod subsystem;
mod spec;
mod spec_ui;
mod signal;
mod signal_ui;
mod status_ui;
mod units;
#[cfg(target_os = "linux")]
mod updater;
mod wires;

fn main() -> eframe::Result<()> {
    let argument = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let app_id = if cfg!(feature = "flatpak") { "io.github.citrineio.DarahEDA" } else { "darah-eda" };
    eframe::run_native(
        "Darah EDA",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_app_id(app_id)
                .with_inner_size([1370.0, 870.0])
                .with_min_inner_size([900.0, 600.0])
                .with_icon(eframe::egui::IconData {
                    rgba: include_bytes!("../packaging/darah-eda.rgba").to_vec(),
                    width: 256,
                    height: 256,
                }),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(app::Darah::new(cc, argument.as_deref())))),
    )
}
