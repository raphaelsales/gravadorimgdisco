// No console window behind the program on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod checks;
mod drive;
mod image;
mod mechanics;
mod media;
mod picture;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut screenshot: Option<PathBuf> = None;
    let mut demo_step: Option<usize> = None;
    let mut demo_picture = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--screenshot" if i + 1 < args.len() => {
                screenshot = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--demo-picture" => demo_picture = true,
            "--step" if i + 1 < args.len() => {
                demo_step = args[i + 1].parse().ok();
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Gravador de Imagens — passo a passo")
        .with_inner_size([1120.0, 760.0])
        .with_min_inner_size([900.0, 620.0]);
    if screenshot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }

    let mut options = eframe::NativeOptions { viewport, run_and_return: false, ..Default::default() };
    if screenshot.is_some() {
        #[cfg(target_os = "macos")]
        {
            options.event_loop_builder = Some(Box::new(|b| {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                b.with_activate_ignoring_other_apps(false);
            }));
        }
    }

    eframe::run_native(
        "Gravador de Imagens",
        options,
        Box::new(move |cc| Ok(Box::new(app::GuidedBurner::new(cc, screenshot, demo_step, demo_picture)))),
    )
}
