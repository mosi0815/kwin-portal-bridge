use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenInfo {
    pub id: String,
    pub name: String,
    pub geometry: Rect,
    pub scale: Option<f64>,
    pub refresh_millihz: Option<u32>,
    pub is_active: bool,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: String,
    pub title: String,
    pub geometry: Rect,
    pub pid: Option<u32>,
    pub desktop_file_name: Option<String>,
    pub resource_class: Option<String>,
    pub resource_name: Option<String>,
    pub window_role: Option<String>,
    pub window_type: Option<String>,
    pub is_dock: Option<bool>,
    pub is_desktop: Option<bool>,
    pub is_visible: Option<bool>,
    pub is_minimized: Option<bool>,
    pub is_normal_window: Option<bool>,
    pub is_dialog: Option<bool>,
    pub output: Option<String>,
    pub stacking_order: usize,
    pub is_active: bool,
    pub exclude_from_capture: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcludeUpdate {
    pub windows: Vec<String>,
    pub value: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotResult {
    pub base64: String,
    pub width: u32,
    pub height: u32,
    pub display_width: u32,
    pub display_height: u32,
    pub display_id: String,
    pub origin_x: i32,
    pub origin_y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotCapture {
    pub base64: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRef {
    pub bundle_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrepareActionResult {
    pub hidden: Vec<String>,
    pub activated: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RaiseWindowAtPointResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topmost: Option<AppRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raised: Option<AppRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<AppRef>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointerActionResult {
    pub action: String,
    pub x: i32,
    pub y: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raised: Option<AppRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<AppRef>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardActionResult {
    pub action: String,
    pub keys: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragActionResult {
    pub action: String,
    pub from_x: i32,
    pub from_y: i32,
    pub to_x: i32,
    pub to_y: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raised: Option<AppRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<AppRef>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvePrepareCaptureResult {
    pub base64: String,
    pub width: u32,
    pub height: u32,
    pub display_width: u32,
    pub display_height: u32,
    pub display_id: String,
    pub origin_x: i32,
    pub origin_y: i32,
    pub hidden: Vec<String>,
    pub activated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolPresence {
    pub command: String,
    pub available: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub tools: Vec<ToolPresence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalStream {
    pub node_id: u32,
    pub source_type: String,
    pub position: [i32; 2],
    pub size: [i32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalSessionInfo {
    pub session_id: String,
    pub pipewire_fd: i32,
    pub restore_token: Option<String>,
    pub remote_desktop_session: Option<String>,
    pub streams: Vec<PortalStream>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamSelection {
    pub stream: PortalStream,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalActionResult {
    pub action: String,
    pub session: PortalSessionInfo,
    pub target_stream: Option<StreamSelection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameProbeResult {
    pub session: PortalSessionInfo,
    pub target_stream: StreamSelection,
    pub frame: FrameInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameInfo {
    pub frame_id: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: String,
    pub buffer_kind: String,
    pub bytes: Option<usize>,
    pub dmabuf_planes: Option<usize>,
    pub flags: u32,
    pub damage_regions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedImageResult {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub bytes: usize,
}

#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub session: PortalSessionInfo,
    pub target_stream: StreamSelection,
    pub frame: lamco_pipewire::VideoFrame,
    pub frame_byte_len: usize,
}
