mod animation;
mod app;
mod canvas;
mod layers;
mod path_data;
mod project;
mod svg_edit;
mod svg_ops;
mod watcher;

use std::path::PathBuf;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "svg-forge", about = "Collaborative SVG editor for Claude + humans")]
pub struct Cli {
    /// Project directory containing SVG files
    #[arg(default_value = ".")]
    project_dir: String,

    /// Create a new project (static or animated)
    #[arg(long, value_name = "MODE")]
    new: Option<String>,
}

fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let project_dir = PathBuf::from(&cli.project_dir)
        .canonicalize()
        .unwrap_or_else(|_| {
            let p = PathBuf::from(&cli.project_dir);
            if let Err(e) = std::fs::create_dir_all(&p) {
                eprintln!("Failed to create project directory: {}", e);
                std::process::exit(1);
            }
            match p.canonicalize() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to resolve project directory: {}", e);
                    std::process::exit(1);
                }
            }
        });

    // Handle --new flag
    if let Some(mode) = &cli.new {
        project::create_new_project(&project_dir, mode);
    }

    // Ensure there's at least one SVG file
    if project::discover_svg_files(&project_dir).is_empty() {
        project::create_new_project(&project_dir, "static");
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("SVG Forge")
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "SVG Forge",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(app::ForgeApp::new(project_dir)))
        }),
    )
    .expect("Failed to start SVG Forge");
}
