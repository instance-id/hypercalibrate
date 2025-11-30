//! HyperCalibrate - Low-latency TV screen calibration for Hyperion
//!
//! This application captures video from a USB camera, applies perspective
//! correction based on user-defined calibration points, and outputs the
//! corrected video to a virtual camera device for Hyperion to consume.

mod calibration;
mod capture;
mod config;
mod output;
mod server;
mod transform;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// HyperCalibrate - TV screen calibration for Hyperion ambient lighting
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input video device (e.g., /dev/video0)
    #[arg(short, long, default_value = "/dev/video0")]
    input: String,

    /// Output video device (v4l2loopback, e.g., /dev/video10)
    #[arg(short, long, default_value = "/dev/video10")]
    output: String,

    /// Capture width
    #[arg(long, default_value_t = 640)]
    width: u32,

    /// Capture height
    #[arg(long, default_value_t = 480)]
    height: u32,

    /// Target FPS
    #[arg(long, default_value_t = 30)]
    fps: u32,

    /// Web server host
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Web server port
    #[arg(short, long, default_value_t = 8091)]
    port: u16,

    /// Configuration file path
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { Level::DEBUG } else { Level::INFO };
    let _subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .compact()
        .init();

    info!("HyperCalibrate v{}", env!("CARGO_PKG_VERSION"));

    // Load or create configuration
    let config = config::Config::load_or_create(&args.config)?;

    // Use config file values, with CLI args as overrides
    let input_device = config.video.input_device.clone();
    let output_device = config.video.output_device.clone();
    let width = config.video.width;
    let height = config.video.height;
    let fps = config.video.fps;
    let host = config.server.host.clone();
    let port = config.server.port;

    info!("Input device: {}", input_device);
    info!("Output device: {}", output_device);
    info!("Resolution: {}x{} @ {}fps", width, height, fps);

    let config = Arc::new(parking_lot::RwLock::new(config));

    // Create shared state for live calibration updates
    let state = Arc::new(crate::server::AppState::new(
        config.clone(),
        args.config.clone(),
        width,
        height,
    ));

    // Start the video processing pipeline
    let pipeline_state = state.clone();
    let input_dev = input_device.clone();
    let output_dev = output_device.clone();

    let pipeline_handle = tokio::task::spawn_blocking(move || {
        crate::capture::run_pipeline(
            &input_dev,
            &output_dev,
            width,
            height,
            fps,
            pipeline_state,
        )
    });

    // Start the web server
    let addr = format!("{}:{}", host, port);
    info!("Starting web server at http://{}", addr);

    let server_handle = tokio::spawn(async move {
        server::run_server(&addr, state).await
    });

    // Wait for either to finish (or error)
    tokio::select! {
        result = pipeline_handle => {
            match result {
                Ok(Ok(())) => info!("Pipeline exited normally"),
                Ok(Err(e)) => tracing::error!("Pipeline error: {}", e),
                Err(e) => tracing::error!("Pipeline task panicked: {}", e),
            }
        }
        result = server_handle => {
            match result {
                Ok(Ok(())) => info!("Server exited normally"),
                Ok(Err(e)) => tracing::error!("Server error: {}", e),
                Err(e) => tracing::error!("Server task panicked: {}", e),
            }
        }
    }

    Ok(())
}
