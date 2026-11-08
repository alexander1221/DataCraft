use clap::Parser;
use datacraft_desktop::Desktop;
use datacraft_engine::WorkerConfig;
use eframe::egui;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "DataCraft native local pipeline viewer")]
struct Options {
    project: Option<PathBuf>,
    #[arg(long, env = "DATACRAFT_JAVA", default_value = "java")]
    java: PathBuf,
    #[arg(
        long,
        env = "DATACRAFT_WORKER_CLASSPATH",
        default_value = "workers/spark/target/classpath.txt"
    )]
    worker_classpath: PathBuf,
    /// Close after rendering this many frames (native startup verification).
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    smoke_frames: Option<u32>,
}

fn main() -> eframe::Result {
    let options = Options::parse();
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "DataCraft",
        native,
        Box::new(move |cc| {
            cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
            Ok(Box::new(Desktop::new(
                options.project,
                WorkerConfig {
                    java: options.java,
                    classpath_file: options.worker_classpath,
                },
                options.smoke_frames,
            )))
        }),
    )
}
