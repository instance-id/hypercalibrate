//! Web server for calibration UI and API

use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use parking_lot::RwLock;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

use crate::calibration::{calibration_to_ui_points, update_calibration_point, CalibrationPoint};
use crate::config::{Calibration, Config};
use crate::transform::PerspectiveTransform;

/// Embedded static files for the web UI
#[derive(RustEmbed)]
#[folder = "static/"]
struct StaticAssets;

/// Shared application state
pub struct AppState {
    /// Configuration (with calibration data)
    pub config: Arc<RwLock<Config>>,
    /// Path to save configuration
    config_path: PathBuf,
    /// Current perspective transform (cached)
    transform: RwLock<PerspectiveTransform>,
    /// Latest preview frame (JPEG encoded)
    preview_frame: RwLock<Vec<u8>>,
    /// Latest raw (uncalibrated) preview frame (JPEG encoded)
    raw_preview_frame: RwLock<Vec<u8>>,
    /// Frame dimensions
    width: u32,
    height: u32,
}

impl AppState {
    pub fn new(config: Arc<RwLock<Config>>, config_path: PathBuf, width: u32, height: u32) -> Self {
        let transform = {
            let cfg = config.read();
            PerspectiveTransform::from_calibration(&cfg.calibration, width, height)
        };

        Self {
            config,
            config_path,
            transform: RwLock::new(transform),
            preview_frame: RwLock::new(Vec::new()),
            raw_preview_frame: RwLock::new(Vec::new()),
            width,
            height,
        }
    }

    /// Get the current perspective transform
    pub fn get_transform(&self) -> PerspectiveTransform {
        self.transform.read().clone()
    }

    /// Update the cached transform from current calibration
    pub fn update_transform(&self) {
        let config = self.config.read();
        let new_transform =
            PerspectiveTransform::from_calibration(&config.calibration, self.width, self.height);
        *self.transform.write() = new_transform;
    }

    /// Update the preview frame (called from capture thread)
    pub fn update_preview(&self, rgb_data: &[u8], width: u32, height: u32) {
        if let Ok(jpeg) = encode_jpeg(rgb_data, width, height, 70) {
            *self.preview_frame.write() = jpeg;
        }
    }

    /// Update the raw preview frame (uncalibrated, for calibration UI)
    pub fn update_raw_preview(&self, rgb_data: &[u8], width: u32, height: u32) {
        if let Ok(jpeg) = encode_jpeg(rgb_data, width, height, 70) {
            *self.raw_preview_frame.write() = jpeg;
        }
    }

    /// Get the latest preview frame
    pub fn get_preview(&self) -> Vec<u8> {
        self.preview_frame.read().clone()
    }

    /// Get the latest raw preview frame
    pub fn get_raw_preview(&self) -> Vec<u8> {
        self.raw_preview_frame.read().clone()
    }

    /// Save configuration to file
    pub fn save_config(&self) -> Result<()> {
        let config = self.config.read();
        config.save(&self.config_path)
    }
}

/// Encode RGB data to JPEG
fn encode_jpeg(rgb_data: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>> {
    use image::{ImageBuffer, Rgb, ImageOutputFormat};
    use std::io::Cursor;

    let img: ImageBuffer<Rgb<u8>, _> =
        ImageBuffer::from_raw(width, height, rgb_data.to_vec())
            .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;

    let mut jpeg_data = Vec::new();
    let mut cursor = Cursor::new(&mut jpeg_data);
    img.write_to(&mut cursor, ImageOutputFormat::Jpeg(quality))?;

    Ok(jpeg_data)
}

/// Run the web server
pub async fn run_server(addr: &str, state: Arc<AppState>) -> Result<()> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Static files and UI
        .route("/", get(index_handler))
        .route("/static/*path", get(static_handler))
        // API endpoints
        .route("/api/calibration", get(get_calibration))
        .route("/api/calibration", post(set_calibration))
        .route("/api/calibration/point/:id", post(update_point))
        .route("/api/calibration/point/:id", axum::routing::delete(delete_point))
        .route("/api/calibration/point/add", post(add_point))
        .route("/api/calibration/reset", post(reset_calibration))
        .route("/api/calibration/save", post(save_calibration))
        .route("/api/calibration/enable", post(enable_calibration))
        .route("/api/calibration/disable", post(disable_calibration))
        // Preview streams
        .route("/api/preview", get(get_preview))
        .route("/api/preview/raw", get(get_raw_preview))
        .route("/api/preview/stream", get(preview_stream))
        // System info
        .route("/api/info", get(get_info))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Web server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Serve the main index page
async fn index_handler() -> impl IntoResponse {
    match StaticAssets::get("index.html") {
        Some(content) => Html(content.data.to_vec()).into_response(),
        None => (StatusCode::NOT_FOUND, "Index not found").into_response(),
    }
}

/// Serve static files
async fn static_handler(Path(path): Path<String>) -> impl IntoResponse {
    let path = path.trim_start_matches('/');

    match StaticAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(axum::http::header::CONTENT_TYPE, mime.as_ref())],
                content.data.to_vec(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

/// API response for calibration data
#[derive(Serialize)]
struct CalibrationResponse {
    enabled: bool,
    points: Vec<CalibrationPoint>,
    width: u32,
    height: u32,
}

/// Get current calibration
async fn get_calibration(State(state): State<Arc<AppState>>) -> Json<CalibrationResponse> {
    let config = state.config.read();
    let points = calibration_to_ui_points(&config.calibration);

    Json(CalibrationResponse {
        enabled: config.calibration.enabled,
        points,
        width: state.width,
        height: state.height,
    })
}

/// Request to update full calibration
#[derive(Deserialize)]
struct SetCalibrationRequest {
    points: Vec<PointUpdate>,
}

#[derive(Deserialize)]
struct PointUpdate {
    id: usize,
    x: f64,
    y: f64,
}

/// Set full calibration data
async fn set_calibration(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetCalibrationRequest>,
) -> impl IntoResponse {
    {
        let mut config = state.config.write();
        for point in req.points {
            update_calibration_point(&mut config.calibration, point.id, point.x, point.y);
        }
    }

    // Update transform
    state.update_transform();

    StatusCode::OK
}

/// Update a single calibration point
async fn update_point(
    State(state): State<Arc<AppState>>,
    Path(id): Path<usize>,
    Json(point): Json<PointUpdate>,
) -> impl IntoResponse {
    {
        let mut config = state.config.write();
        update_calibration_point(&mut config.calibration, id, point.x, point.y);
    }

    // Update transform immediately for live preview
    state.update_transform();

    StatusCode::OK
}

/// Request to add a new edge point
#[derive(Deserialize)]
struct AddPointRequest {
    edge: usize,
    x: f64,
    y: f64,
}

/// Response when adding a point
#[derive(Serialize)]
struct AddPointResponse {
    id: usize,
}

/// Add a new edge point
async fn add_point(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddPointRequest>,
) -> impl IntoResponse {
    let id = {
        let mut config = state.config.write();
        config.calibration.add_edge_point(req.edge, req.x, req.y)
    };

    state.update_transform();

    Json(AddPointResponse { id })
}

/// Delete an edge point
async fn delete_point(
    State(state): State<Arc<AppState>>,
    Path(id): Path<usize>,
) -> impl IntoResponse {
    let removed = {
        let mut config = state.config.write();
        config.calibration.remove_edge_point(id)
    };

    if removed {
        state.update_transform();
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

/// Reset calibration to defaults
async fn reset_calibration(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    {
        let mut config = state.config.write();
        config.calibration = Calibration::default();
    }

    state.update_transform();

    StatusCode::OK
}

/// Save calibration to file
async fn save_calibration(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.save_config() {
        Ok(_) => (StatusCode::OK, "Saved").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Enable calibration processing
async fn enable_calibration(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    {
        let mut config = state.config.write();
        config.calibration.enabled = true;
    }
    StatusCode::OK
}

/// Disable calibration processing
async fn disable_calibration(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    {
        let mut config = state.config.write();
        config.calibration.enabled = false;
    }
    StatusCode::OK
}

/// Get current preview frame (JPEG)
async fn get_preview(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let frame = state.get_preview();
    if frame.is_empty() {
        return (StatusCode::SERVICE_UNAVAILABLE, "No frame available").into_response();
    }

    (
        [(axum::http::header::CONTENT_TYPE, "image/jpeg")],
        frame,
    )
        .into_response()
}

/// Get raw (uncalibrated) preview frame (JPEG)
async fn get_raw_preview(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let frame = state.get_raw_preview();
    if frame.is_empty() {
        return (StatusCode::SERVICE_UNAVAILABLE, "No frame available").into_response();
    }

    (
        [(axum::http::header::CONTENT_TYPE, "image/jpeg")],
        frame,
    )
        .into_response()
}

/// MJPEG stream endpoint for continuous preview
async fn preview_stream(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    use axum::body::Body;
    use tokio_stream::StreamExt;

    let stream = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
        std::time::Duration::from_millis(100), // ~10 fps for preview
    ))
    .map(move |_| {
        let frame = state.get_raw_preview();
        if frame.is_empty() {
            return Ok::<_, std::convert::Infallible>(
                "--frame\r\nContent-Type: image/jpeg\r\n\r\n".to_string().into_bytes(),
            );
        }

        let mut response = Vec::new();
        response.extend_from_slice(b"--frame\r\nContent-Type: image/jpeg\r\nContent-Length: ");
        response.extend_from_slice(frame.len().to_string().as_bytes());
        response.extend_from_slice(b"\r\n\r\n");
        response.extend_from_slice(&frame);
        response.extend_from_slice(b"\r\n");

        Ok(response)
    });

    let body = Body::from_stream(stream);

    (
        [(
            axum::http::header::CONTENT_TYPE,
            "multipart/x-mixed-replace; boundary=frame",
        )],
        body,
    )
}

/// System information response
#[derive(Serialize)]
struct InfoResponse {
    version: String,
    width: u32,
    height: u32,
    calibration_enabled: bool,
}

/// Get system information
async fn get_info(State(state): State<Arc<AppState>>) -> Json<InfoResponse> {
    let config = state.config.read();

    Json(InfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        width: state.width,
        height: state.height,
        calibration_enabled: config.calibration.enabled,
    })
}
