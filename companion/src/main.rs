#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backends;
mod backlog;
mod clipboard;
mod config;
mod diag;
mod explain;
mod focus;
mod fonts;
mod layout;
mod markup;
mod names;
mod protocol;
mod strip;
#[cfg(test)]
mod testing;
mod text;
mod theme;
mod translate;
mod worker;

use eframe::egui;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// The title bar and taskbar icon, built into the binary.
fn window_icon() -> egui::IconData {
    let image = xcap::image::load_from_memory(include_bytes!("../icon.png"))
        .expect("icon.png is a valid PNG")
        .to_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData { rgba: image.into_raw(), width, height }
}

fn decode_file(path: &str) {
    let img = match xcap::image::open(path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => return println!("Could not open {path}: {e}"),
    };
    let report = match strip::find(&img) {
        None => format!("No pixel strip in {path}\n"),
        Some(lock) => {
            let mut report =
                format!("Strip at {},{} ({} px blocks): {:?}\n", lock.x, lock.y, lock.block, strip::decode(&img, lock));
            for line in strip::diagnose(&img, lock) {
                report.push_str(&format!("  {line}\n"));
            }
            report
        }
    };
    print!("{report}");
    // The Windows release is a windowed program, whose printing a terminal does not show:
    // the report is also written beside the picture.
    let beside = format!("{path}.decode.txt");
    if let Err(e) = std::fs::write(&beside, &report) {
        eprintln!("could not write {beside}: {e}");
    }
}

/// For a picture of the overlay at work: with TRANSLATE_GAME_SHOT set to a file, saves
/// the game's window as it is on screen, with whatever is drawn over it, and quits. A
/// copy of the companion that is already running keeps drawing the overlay meanwhile.
fn game_shot(path: &std::path::Path) -> Result<(), String> {
    let problem = |e: xcap::XCapError| e.to_string();
    let windows = xcap::Window::all().map_err(problem)?;
    let game = windows
        .iter()
        .find(|window| focus::is_wow(&window.title().unwrap_or_default(), &window.app_name().unwrap_or_default()))
        .ok_or("the game's window was not found")?;
    let monitor = game.current_monitor().map_err(problem)?;
    let screen = monitor.capture_image().map_err(problem)?;
    // Windows are measured in points, the picture in pixels.
    let scale = screen.width() as f32 / monitor.width().map_err(problem)? as f32;
    let left = (game.x().map_err(problem)? - monitor.x().map_err(problem)?).max(0) as f32 * scale;
    let top = (game.y().map_err(problem)? - monitor.y().map_err(problem)?).max(0) as f32 * scale;
    let width = game.width().map_err(problem)? as f32 * scale;
    let height = game.height().map_err(problem)? as f32 * scale;
    let picture = xcap::image::imageops::crop_imm(&screen, left as u32, top as u32, width as u32, height as u32).to_image();
    picture.save(path).map_err(|e| e.to_string())
}

fn main() -> eframe::Result {
    // `translate-companion --decode shot.png` reads a strip from a screenshot.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--decode" {
        decode_file(&args[2]);
        return Ok(());
    }
    if let Some(path) = std::env::var_os("TRANSLATE_GAME_SHOT") {
        if let Err(problem) = game_shot(std::path::Path::new(&path)) {
            eprintln!("no picture of the game: {problem}");
        }
        return Ok(());
    }
    diag::start();
    let config = Arc::new(Mutex::new(config::load()));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Nǎi Mā Speak")
            .with_inner_size(app::START_SIZE)
            .with_min_inner_size([app::MIN_WIDTH, app::MIN_HEIGHT])
            .with_icon(window_icon())
            // On macOS the window has no title bar of the system's: the app's own bar
            // is at the top, moves the window when dragged, and has Hide where the
            // system's buttons would be. The yellow button minimized the window, from
            // where a click on the app's icon did not bring it back.
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_titlebar_buttons_shown(false)
            // The renderer only gets an alpha channel when the root window asks
            // for one; without it the overlay is drawn solid black.
            .with_transparent(true),
        ..Default::default()
    };

    eframe::run_native(
        "Nai Ma Speak",
        options,
        Box::new(move |cc| {
            fonts::add(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            let (ui_tx, ui_rx) = mpsc::channel();
            let ui = worker::UiSender::new(ui_tx, cc.egui_ctx.clone());
            let (clip_tx, clip_rx) = mpsc::channel();
            let (strip_tx, strip_rx) = mpsc::channel();
            let (interactive_tx, interactive_rx) = mpsc::channel();

            worker::spawn(config.clone(), strip_rx, ui.clone(), clip_tx.clone());
            worker::spawn(config.clone(), interactive_rx, ui.clone(), clip_tx);
            strip::spawn(strip_tx, interactive_tx.clone(), ui.clone());
            focus::spawn(ui.clone());
            clipboard::spawn(clip_rx, ui);

            Ok(Box::new(app::TranslateApp::new(config, ui_rx, interactive_tx)))
        }),
    )
}
