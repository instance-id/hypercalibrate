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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::{Any, CorsLayer};

use crate::calibration::{calibration_to_ui_points, update_calibration_point, CalibrationPoint};
use crate::camera_controls::{CameraControl, CameraControlsManager, ControlValue};
use crate::config::{Calibration, Config};
use crate::system_stats::SystemStats;
use crate::transform::PerspectiveTransform;
use crate::video_settings::{query_camera_capabilities, CameraCapabilities, PendingVideoSettings};

/// Embedded static files for the web UI
#[derive(RustEmbed)]
#[folder = "static/"]
struct StaticAssets;

/// Performance statistics
#[derive(Debug, Default)]
pub struct PerformanceStats {
    /// Number of frames processed
    pub frames_processed: AtomicU64,
    /// Total time waiting for camera to deliver frames (microseconds)
    pub total_frame_wait_us: AtomicU64,
    /// Total decode/conversion time in microseconds (MJPEG decode or YUYV→RGB)
    pub total_decode_us: AtomicU64,
    /// Total transform time in microseconds (perspective warp)
    pub total_transform_us: AtomicU64,
    /// Total output write time in microseconds
    pub total_output_us: AtomicU64,
    /// Total preview encode time in microseconds
    pub total_preview_encode_us: AtomicU64,
    /// Frames where preview was encoded
    pub preview_frames_encoded: AtomicU64,
    /// Start time for FPS calculation
    pub start_time: RwLock<Option<Instant>>,
    /// Last frame timestamp for latency tracking
    pub last_frame_time: RwLock<Option<Instant>>,
}

impl PerformanceStats {
    pub fn reset(&self) {
        self.frames_processed.store(0, Ordering::Relaxed);
        self.total_frame_wait_us.store(0, Ordering::Relaxed);
        self.total_decode_us.store(0, Ordering::Relaxed);
        self.total_transform_us.store(0, Ordering::Relaxed);
        self.total_output_us.store(0, Ordering::Relaxed);
        self.total_preview_encode_us.store(0, Ordering::Relaxed);
        self.preview_frames_encoded.store(0, Ordering::Relaxed);
        *self.start_time.write() = Some(Instant::now());
        *self.last_frame_time.write() = None;
    }
}

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
    /// Target FPS
    fps: u32,
    /// Input device path (for camera controls)
    input_device: String,
    /// Camera controls manager
    camera_controls: RwLock<Option<CameraControlsManager>>,
    /// Whether preview clients are connected (enables preview encoding)
    preview_clients_active: AtomicBool,
    /// Performance statistics
    pub stats: PerformanceStats,
    /// Pending video settings that require a restart
    pending_video_settings: RwLock<PendingVideoSettings>,
    /// Flag to signal a restart is requested
    pub restart_requested: AtomicBool,
}

impl AppState {
    pub fn new(config: Arc<RwLock<Config>>, config_path: PathBuf, width: u32, height: u32, fps: u32) -> Self {
        let (transform, input_device) = {
            let cfg = config.read();
            (
                PerspectiveTransform::from_calibration(&cfg.calibration, width, height),
                cfg.video.input_device.clone(),
            )
        };

        let stats = PerformanceStats::default();
        *stats.start_time.write() = Some(Instant::now());

        Self {
            config,
            config_path,
            transform: RwLock::new(transform),
            preview_frame: RwLock::new(Vec::new()),
            raw_preview_frame: RwLock::new(Vec::new()),
            width,
            height,
            fps,
            input_device,
            camera_controls: RwLock::new(None),
            preview_clients_active: AtomicBool::new(false),
            stats,
            pending_video_settings: RwLock::new(PendingVideoSettings::default()),
            restart_requested: AtomicBool::new(false),
        }
    }

    /// Initialize camera controls (should be called after camera is opened)
    pub fn init_camera_controls(&self) -> Result<()> {
        let mut manager = CameraControlsManager::new(&self.input_device);
        manager.query_controls()?;

        // Apply saved settings from config
        {
            let config = self.config.read();
            for (name, value) in &config.camera.controls {
                let name_normalized = name.to_lowercase().replace('_', " ");
                if let Some(control) = manager
                    .get_controls()
                    .iter()
                    .find(|c| c.name.to_lowercase() == name_normalized)
                {
                    if control.flags.read_only || control.flags.inactive {
                        continue;
                    }

                    let ctrl_value =
                        if control.control_type == crate::camera_controls::ControlType::Boolean {
                            ControlValue::Boolean(*value != 0)
                        } else {
                            ControlValue::Integer(*value)
                        };

                    if let Err(e) = manager.set_control(control.id, ctrl_value) {
                        tracing::warn!("Failed to apply saved camera setting '{}': {}", name, e);
                    }
                }
            }
        }

        *self.camera_controls.write() = Some(manager);
        Ok(())
    }

    /// Get camera controls
    pub fn get_camera_controls(&self) -> Option<Vec<CameraControl>> {
        self.camera_controls
            .read()
            .as_ref()
            .map(|m| m.get_controls().clone())
    }

    /// Set a camera control value
    pub fn set_camera_control(&self, id: u32, value: ControlValue) -> Result<()> {
        let mut controls = self.camera_controls.write();
        if let Some(manager) = controls.as_mut() {
            manager.set_control(id, value.clone())?;

            // Update config with new value
            if let Some(control) = manager.get_control(id) {
                let key = control.name.to_lowercase().replace(' ', "_");
                if let Some(val) = value.as_i64() {
                    let mut config = self.config.write();
                    config.camera.controls.insert(key, val);
                }
            }
        }
        Ok(())
    }

    /// Reset camera controls to defaults
    pub fn reset_camera_controls(&self) -> Result<()> {
        let mut controls = self.camera_controls.write();
        if let Some(manager) = controls.as_mut() {
            manager.reset_all_controls()?;

            // Clear saved settings
            {
                let mut config = self.config.write();
                config.camera.controls.clear();
            }
        }
        Ok(())
    }

    /// Refresh camera control values
    pub fn refresh_camera_controls(&self) -> Result<()> {
        let mut controls = self.camera_controls.write();
        if let Some(manager) = controls.as_mut() {
            manager.refresh_values()?;
        }
        Ok(())
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

    /// Check if preview encoding should be active
    pub fn should_encode_preview(&self) -> bool {
        self.preview_clients_active.load(Ordering::Relaxed)
    }

    /// Set preview client active state
    pub fn set_preview_active(&self, active: bool) {
        self.preview_clients_active.store(active, Ordering::Relaxed);
    }

    /// Record frame timing stats
    /// - frame_wait_us: time waiting for camera to deliver frame
    /// - decode_us: time to decode/convert pixel format
    /// - transform_us: time for perspective warp
    /// - output_us: time to write to virtual camera
    pub fn record_frame_stats(
        &self,
        frame_wait_us: u64,
        decode_us: u64,
        transform_us: u64,
        output_us: u64,
        preview_encode_us: Option<u64>,
    ) {
        self.stats.frames_processed.fetch_add(1, Ordering::Relaxed);
        self.stats.total_frame_wait_us.fetch_add(frame_wait_us, Ordering::Relaxed);
        self.stats.total_decode_us.fetch_add(decode_us, Ordering::Relaxed);
        self.stats.total_transform_us.fetch_add(transform_us, Ordering::Relaxed);
        self.stats.total_output_us.fetch_add(output_us, Ordering::Relaxed);

        if let Some(preview_us) = preview_encode_us {
            self.stats.total_preview_encode_us.fetch_add(preview_us, Ordering::Relaxed);
            self.stats.preview_frames_encoded.fetch_add(1, Ordering::Relaxed);
        }

        *self.stats.last_frame_time.write() = Some(Instant::now());
    }

    /// Save configuration to file
    pub fn save_config(&self) -> Result<()> {
        let config = self.config.read();
        config.save(&self.config_path)
    }

    /// Get camera capabilities (supported resolutions, framerates)
    pub fn get_camera_capabilities(&self) -> Result<CameraCapabilities> {
        query_camera_capabilities(&self.input_device, self.width, self.height, self.fps)
    }

    /// Get pending video settings
    pub fn get_pending_video_settings(&self) -> PendingVideoSettings {
        self.pending_video_settings.read().clone()
    }

    /// Set pending video settings (will be applied on restart)
    pub fn set_pending_video_settings(&self, width: Option<u32>, height: Option<u32>, fps: Option<u32>) -> Result<()> {
        let mut pending = self.pending_video_settings.write();

        // Determine if there are actual changes
        let width_changed = width.map(|w| w != self.width).unwrap_or(false);
        let height_changed = height.map(|h| h != self.height).unwrap_or(false);
        let fps_changed = fps.map(|f| f != self.fps).unwrap_or(false);

        if width_changed || height_changed || fps_changed {
            pending.width = if width_changed { width } else { None };
            pending.height = if height_changed { height } else { None };
            pending.fps = if fps_changed { fps } else { None };
            pending.needs_restart = true;

            // Also update the config file so it persists across restarts
            {
                let mut config = self.config.write();
                if let Some(w) = width {
                    config.video.width = w;
                }
                if let Some(h) = height {
                    config.video.height = h;
                }
                if let Some(f) = fps {
                    config.video.fps = f;
                }
            }

            // Save to file
            self.save_config()?;

            tracing::info!(
                "Video settings changed: {}x{} @ {} fps -> will apply on restart",
                width.unwrap_or(self.width),
                height.unwrap_or(self.height),
                fps.unwrap_or(self.fps)
            );
        } else {
            pending.clear();
        }

        Ok(())
    }

    /// Clear pending video settings
    pub fn clear_pending_video_settings(&self) {
        self.pending_video_settings.write().clear();
    }

    /// Request a service restart
    pub fn request_restart(&self) {
        tracing::info!("Service restart requested via API");
        self.restart_requested.store(true, Ordering::SeqCst);
    }

    /// Check if restart was requested
    pub fn is_restart_requested(&self) -> bool {
        self.restart_requested.load(Ordering::SeqCst)
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
        // API endpoints - Calibration
        .route("/api/calibration", get(get_calibration))
        .route("/api/calibration", post(set_calibration))
        .route("/api/calibration/point/:id", post(update_point))
        .route("/api/calibration/point/:id", axum::routing::delete(delete_point))
        .route("/api/calibration/point/add", post(add_point))
        .route("/api/calibration/reset", post(reset_calibration))
        .route("/api/calibration/save", post(save_calibration))
        .route("/api/calibration/enable", post(enable_calibration))
        .route("/api/calibration/disable", post(disable_calibration))
        // API endpoints - Camera Controls
        .route("/api/camera/controls", get(get_camera_controls))
        .route("/api/camera/control/:id", post(set_camera_control))
        .route("/api/camera/controls/reset", post(reset_camera_controls))
        .route("/api/camera/controls/refresh", post(refresh_camera_controls))
        // API endpoints - Video Settings (Resolution/Framerate)
        .route("/api/video/capabilities", get(get_video_capabilities))
        .route("/api/video/settings", get(get_video_settings))
        .route("/api/video/settings", post(set_video_settings))
        .route("/api/system/restart", post(request_system_restart))
        // Preview streams
        .route("/api/preview", get(get_preview))
        .route("/api/preview/raw", get(get_raw_preview))
        .route("/api/preview/stream", get(preview_stream))
        // System info and stats
        .route("/api/info", get(get_info))
        .route("/api/stats", get(get_stats))
        .route("/api/stats/reset", post(reset_stats))
        .route("/api/system/stats", get(get_system_stats))
        .route("/api/preview/activate", post(activate_preview))
        .route("/api/preview/deactivate", post(deactivate_preview))
        // Debug logging
        .route("/api/debug/log", post(post_debug_log))
        .route("/api/debug/clear", post(clear_debug_log))
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
    fps: u32,
    calibration_enabled: bool,
    camera_controls_available: bool,
}

/// Get system information
async fn get_info(State(state): State<Arc<AppState>>) -> Json<InfoResponse> {
    let config = state.config.read();
    let has_controls = state.camera_controls.read().is_some();

    Json(InfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        width: state.width,
        height: state.height,
        fps: state.fps,
        calibration_enabled: config.calibration.enabled,
        camera_controls_available: has_controls,
    })
}

// ============================================================================
// Camera Controls API
// ============================================================================

/// Camera controls response
#[derive(Serialize)]
struct CameraControlsResponse {
    available: bool,
    controls: Vec<CameraControl>,
}

/// Get all camera controls
async fn get_camera_controls(State(state): State<Arc<AppState>>) -> Json<CameraControlsResponse> {
    match state.get_camera_controls() {
        Some(controls) => Json(CameraControlsResponse {
            available: true,
            controls,
        }),
        None => Json(CameraControlsResponse {
            available: false,
            controls: Vec::new(),
        }),
    }
}

/// Request to set a camera control
#[derive(Deserialize)]
struct SetCameraControlRequest {
    value: serde_json::Value,
}

/// Set a camera control value
async fn set_camera_control(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u32>,
    Json(req): Json<SetCameraControlRequest>,
) -> impl IntoResponse {
    // Convert JSON value to ControlValue
    let control_value = match &req.value {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                ControlValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                ControlValue::Integer(f as i64)
            } else {
                return (StatusCode::BAD_REQUEST, "Invalid number value").into_response();
            }
        }
        serde_json::Value::Bool(b) => ControlValue::Boolean(*b),
        serde_json::Value::String(s) => {
            // Try to parse as integer first (for menu values sent as strings)
            if let Ok(i) = s.parse::<i64>() {
                ControlValue::Integer(i)
            } else {
                ControlValue::String(s.clone())
            }
        }
        _ => return (StatusCode::BAD_REQUEST, "Invalid value type").into_response(),
    };

    match state.set_camera_control(id, control_value) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Reset all camera controls to defaults
async fn reset_camera_controls(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.reset_camera_controls() {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Refresh camera control values from hardware
async fn refresh_camera_controls(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.refresh_camera_controls() {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ============================================================================
// Video Settings API (Resolution/Framerate)
// ============================================================================

/// Video capabilities response
#[derive(Serialize)]
struct VideoCapabilitiesResponse {
    capabilities: CameraCapabilities,
    current: CurrentVideoSettings,
}

/// Current video settings
#[derive(Serialize)]
struct CurrentVideoSettings {
    width: u32,
    height: u32,
    fps: u32,
}

/// Request to change video settings
#[derive(Deserialize)]
struct VideoSettingsRequest {
    width: u32,
    height: u32,
    fps: u32,
}

/// Video settings response
#[derive(Serialize)]
struct VideoSettingsResponse {
    current: CurrentVideoSettings,
    pending: Option<PendingVideoSettings>,
    restart_required: bool,
    message: String,
}

/// Get video capabilities for the camera
async fn get_video_capabilities(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.get_camera_capabilities() {
        Ok(capabilities) => {
            let response = VideoCapabilitiesResponse {
                capabilities,
                current: CurrentVideoSettings {
                    width: state.width,
                    height: state.height,
                    fps: state.fps,
                },
            };
            Json(response).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to get video capabilities: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// Get current video settings and any pending changes
async fn get_video_settings(State(state): State<Arc<AppState>>) -> Json<VideoSettingsResponse> {
    let pending = state.get_pending_video_settings();
    let has_pending = pending.width.is_some() || pending.height.is_some() || pending.fps.is_some();

    Json(VideoSettingsResponse {
        current: CurrentVideoSettings {
            width: state.width,
            height: state.height,
            fps: state.fps,
        },
        pending: if has_pending { Some(pending) } else { None },
        restart_required: has_pending,
        message: if has_pending {
            "Settings changes are pending. Restart the service to apply them.".to_string()
        } else {
            "No pending changes.".to_string()
        },
    })
}

/// Set new video settings (requires restart to take effect)
async fn set_video_settings(
    State(state): State<Arc<AppState>>,
    Json(request): Json<VideoSettingsRequest>,
) -> impl IntoResponse {
    // Validate the requested settings against camera capabilities
    match state.get_camera_capabilities() {
        Ok(caps) => {
            // Find matching resolution
            let resolution_valid = caps.resolutions.iter().any(|r| {
                r.width == request.width && r.height == request.height
            });

            if !resolution_valid {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "Invalid resolution",
                        "message": format!("{}x{} is not supported by this camera", request.width, request.height)
                    }))
                ).into_response();
            }

            // Find matching framerate for this resolution
            let fps_valid = caps.resolutions.iter()
                .find(|r| r.width == request.width && r.height == request.height)
                .map(|r| r.framerates.iter().any(|fr| fr.fps as u32 == request.fps))
                .unwrap_or(false);

            if !fps_valid {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "Invalid framerate",
                        "message": format!("{} fps is not supported at {}x{}", request.fps, request.width, request.height)
                    }))
                ).into_response();
            }
        }
        Err(e) => {
            tracing::warn!("Could not validate settings against camera capabilities: {}", e);
            // Continue anyway - the settings might still work
        }
    }

    // Check if settings are actually different from current
    let same_as_current =
        request.width == state.width &&
        request.height == state.height &&
        request.fps == state.fps;

    if same_as_current {
        // Clear any pending settings since we're back to current
        state.clear_pending_video_settings();

        return Json(serde_json::json!({
            "success": true,
            "restart_required": false,
            "message": "Settings are already at the requested values."
        })).into_response();
    }

    // Store pending settings
    let _ = state.set_pending_video_settings(Some(request.width), Some(request.height), Some(request.fps));

    // Save to config file
    match save_video_settings_to_config(&state.config_path, request.width, request.height, request.fps) {
        Ok(_) => {
            tracing::info!(
                "Video settings saved: {}x{} @ {} fps (restart required)",
                request.width, request.height, request.fps
            );

            Json(serde_json::json!({
                "success": true,
                "restart_required": true,
                "message": format!(
                    "Settings saved. Restart the service to apply {}x{} @ {} fps.",
                    request.width, request.height, request.fps
                )
            })).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to save video settings to config: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to save settings",
                    "message": e.to_string()
                }))
            ).into_response()
        }
    }
}

/// Save video settings to config file
fn save_video_settings_to_config(config_path: &std::path::Path, width: u32, height: u32, fps: u32) -> Result<()> {
    use anyhow::Context;
    use std::fs;
    use std::io::Write;

    let content = fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

    let mut new_lines = Vec::new();
    let mut in_camera_section = false;
    let mut found_width = false;
    let mut found_height = false;
    let mut found_fps = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Track section changes
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // If leaving camera section, add any missing settings
            if in_camera_section {
                if !found_width {
                    new_lines.push(format!("width = {}", width));
                }
                if !found_height {
                    new_lines.push(format!("height = {}", height));
                }
                if !found_fps {
                    new_lines.push(format!("fps = {}", fps));
                }
            }
            in_camera_section = trimmed == "[camera]";
        }

        // Handle camera section settings
        if in_camera_section {
            if trimmed.starts_with("width") && trimmed.contains('=') {
                new_lines.push(format!("width = {}", width));
                found_width = true;
                continue;
            }
            if trimmed.starts_with("height") && trimmed.contains('=') {
                new_lines.push(format!("height = {}", height));
                found_height = true;
                continue;
            }
            if trimmed.starts_with("fps") && trimmed.contains('=') {
                new_lines.push(format!("fps = {}", fps));
                found_fps = true;
                continue;
            }
        }

        new_lines.push(line.to_string());
    }

    // If we ended in camera section, add any missing settings
    if in_camera_section {
        if !found_width {
            new_lines.push(format!("width = {}", width));
        }
        if !found_height {
            new_lines.push(format!("height = {}", height));
        }
        if !found_fps {
            new_lines.push(format!("fps = {}", fps));
        }
    }

    // Write back
    let mut file = fs::File::create(config_path)
        .with_context(|| format!("Failed to open config file for writing: {}", config_path.display()))?;

    for (i, line) in new_lines.iter().enumerate() {
        if i > 0 {
            writeln!(file)?;
        }
        write!(file, "{}", line)?;
    }
    writeln!(file)?;

    Ok(())
}

/// Request a system restart
async fn request_system_restart(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Check if there are pending settings
    let pending = state.get_pending_video_settings();
    let has_pending = pending.width.is_some() || pending.height.is_some() || pending.fps.is_some();

    if !has_pending {
        return Json(serde_json::json!({
            "success": false,
            "message": "No pending changes to apply."
        })).into_response();
    }

    // Request restart
    state.request_restart();

    tracing::info!("System restart requested to apply video settings");

    Json(serde_json::json!({
        "success": true,
        "message": "Restart initiated. The service will restart shortly."
    })).into_response()
}

// ============================================================================
// Performance Stats API
// ============================================================================

/// Performance statistics response
#[derive(Serialize)]
struct StatsResponse {
    /// Frames per second
    fps: f64,
    /// Total frames processed since last reset
    frames_processed: u64,
    /// Preview encoding active
    preview_active: bool,
    /// Frames where preview was encoded
    preview_frames_encoded: u64,
    /// Timing breakdown (in milliseconds)
    timing: TimingStats,
    /// Time since stats reset (seconds)
    uptime_secs: f64,
}

#[derive(Serialize)]
struct TimingStats {
    /// Average time waiting for camera (ms) - hardware limited, not improvable
    avg_frame_wait_ms: f64,
    /// Average decode/conversion time (ms) - MJPEG decode or YUYV→RGB
    avg_decode_ms: f64,
    /// Average transform time per frame (ms) - perspective warp
    avg_transform_ms: f64,
    /// Average output write time per frame (ms)
    avg_output_ms: f64,
    /// Average preview encode time per frame (ms) - only when encoding
    avg_preview_encode_ms: f64,
    /// Total processing time per frame (ms) - excludes frame wait
    avg_processing_ms: f64,
    /// Total pipeline time per frame (ms) - includes frame wait
    avg_pipeline_ms: f64,
}

/// Get performance statistics
async fn get_stats(State(state): State<Arc<AppState>>) -> Json<StatsResponse> {
    let frames = state.stats.frames_processed.load(Ordering::Relaxed);
    let preview_frames = state.stats.preview_frames_encoded.load(Ordering::Relaxed);

    let frame_wait_us = state.stats.total_frame_wait_us.load(Ordering::Relaxed);
    let decode_us = state.stats.total_decode_us.load(Ordering::Relaxed);
    let transform_us = state.stats.total_transform_us.load(Ordering::Relaxed);
    let output_us = state.stats.total_output_us.load(Ordering::Relaxed);
    let preview_us = state.stats.total_preview_encode_us.load(Ordering::Relaxed);

    let start_time = state.stats.start_time.read();
    let uptime_secs = start_time
        .map(|t| t.elapsed().as_secs_f64())
        .unwrap_or(0.0);

    let fps = if uptime_secs > 0.0 {
        frames as f64 / uptime_secs
    } else {
        0.0
    };

    // Calculate averages (convert from microseconds to milliseconds)
    let frames_f = frames.max(1) as f64;
    let preview_frames_f = preview_frames.max(1) as f64;

    let avg_frame_wait_ms = (frame_wait_us as f64 / frames_f) / 1000.0;
    let avg_decode_ms = (decode_us as f64 / frames_f) / 1000.0;
    let avg_transform_ms = (transform_us as f64 / frames_f) / 1000.0;
    let avg_output_ms = (output_us as f64 / frames_f) / 1000.0;
    let avg_preview_encode_ms = (preview_us as f64 / preview_frames_f) / 1000.0;

    // Processing time = decode + transform + output (what we can optimize)
    let avg_processing_ms = avg_decode_ms + avg_transform_ms + avg_output_ms;
    // Pipeline time = frame wait + processing (total frame-to-frame time)
    let avg_pipeline_ms = avg_frame_wait_ms + avg_processing_ms;

    Json(StatsResponse {
        fps,
        frames_processed: frames,
        preview_active: state.preview_clients_active.load(Ordering::Relaxed),
        preview_frames_encoded: preview_frames,
        timing: TimingStats {
            avg_frame_wait_ms,
            avg_decode_ms,
            avg_transform_ms,
            avg_output_ms,
            avg_preview_encode_ms,
            avg_processing_ms,
            avg_pipeline_ms,
        },
        uptime_secs,
    })
}

/// Reset performance statistics
async fn reset_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.stats.reset();
    StatusCode::OK
}

/// Activate preview encoding (called when UI opens)
async fn activate_preview(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.set_preview_active(true);
    tracing::info!("Preview encoding activated (client connected)");
    StatusCode::OK
}

/// Deactivate preview encoding (called when UI closes)
async fn deactivate_preview(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.set_preview_active(false);
    tracing::info!("Preview encoding deactivated (client disconnected)");
    StatusCode::OK
}

/// Get system statistics (CPU temp, memory, etc.)
async fn get_system_stats() -> Json<SystemStats> {
    // Run in blocking task since it may call vcgencmd
    let stats = tokio::task::spawn_blocking(SystemStats::gather)
        .await
        .unwrap_or_else(|_| SystemStats::gather());
    Json(stats)
}

// ============================================================================
// Debug Logging
// ============================================================================

/// Debug log file path
const DEBUG_LOG_PATH: &str = "/tmp/hypercalibrate-debug.log";

/// Debug log request
#[derive(Deserialize)]
struct DebugLogRequest {
    entries: Vec<DebugLogEntry>,
}

#[derive(Deserialize)]
struct DebugLogEntry {
    time: String,
    #[serde(rename = "type")]
    log_type: String,
    message: String,
}

/// Post debug log entries to file
async fn post_debug_log(Json(payload): Json<DebugLogRequest>) -> impl IntoResponse {
    use std::fs::OpenOptions;
    use std::io::Write;

    let result = tokio::task::spawn_blocking(move || {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(DEBUG_LOG_PATH)?;

        for entry in &payload.entries {
            writeln!(file, "{} [{}] {}", entry.time, entry.log_type, entry.message)?;
        }

        Ok::<_, std::io::Error>(())
    })
    .await;

    match result {
        Ok(Ok(())) => StatusCode::OK,
        Ok(Err(e)) => {
            tracing::error!("Failed to write debug log: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
        Err(e) => {
            tracing::error!("Debug log task failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// Clear the debug log file
async fn clear_debug_log() -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(|| {
        std::fs::write(DEBUG_LOG_PATH, "")?;
        Ok::<_, std::io::Error>(())
    })
    .await;

    match result {
        Ok(Ok(())) => {
            tracing::info!("Debug log cleared");
            StatusCode::OK
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to clear debug log: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
        Err(e) => {
            tracing::error!("Clear debug log task failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
