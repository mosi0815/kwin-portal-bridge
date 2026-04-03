use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use crate::capture::{screenshot_result_from_frame, zoom_result_from_frame};
use crate::model::{PortalSessionInfo, ScreenInfo, ScreenshotCapture, ScreenshotResult};
use crate::portal::LivePortalSession;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionRequest {
    SessionInfo,
    Shutdown,
    MovePointerAbsolute {
        stream: Option<u32>,
        x: f64,
        y: f64,
    },
    PointerButton {
        button: i32,
        pressed: bool,
    },
    KeyboardKeycode {
        keycode: i32,
        pressed: bool,
    },
    ClickScreenPoint {
        screen: ScreenInfo,
        x: i32,
        y: i32,
        button: i32,
        count: u32,
    },
    ScrollScreenPoint {
        screen: ScreenInfo,
        x: i32,
        y: i32,
        dx: f64,
        dy: f64,
    },
    KeySequence {
        keycodes: Vec<i32>,
        repeat: u32,
    },
    HoldKeyCodes {
        keycodes: Vec<i32>,
        duration_ms: u64,
    },
    DragScreenPoints {
        from_screen: ScreenInfo,
        from_x: i32,
        from_y: i32,
        to_screen: ScreenInfo,
        to_x: i32,
        to_y: i32,
    },
    CaptureStillFrame {
        screen: ScreenInfo,
    },
    CaptureZoom {
        screen: ScreenInfo,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionResponse {
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub async fn start_session_daemon() -> Result<PortalSessionInfo> {
    if let Ok(info) = request::<PortalSessionInfo>(SessionRequest::SessionInfo).await {
        bail!("a portal session is already active: {}", info.session_id);
    }

    let socket = socket_path()?;
    cleanup_stale_socket(&socket).await?;

    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let mut command = Command::new(current_exe);
    command
        .arg("serve-session")
        .arg("--socket")
        .arg(&socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    command
        .spawn()
        .context("failed to spawn session daemon")?;

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match request::<PortalSessionInfo>(SessionRequest::SessionInfo).await {
            Ok(info) => return Ok(info),
            Err(error) => {
                if Instant::now() >= deadline {
                    return Err(error).context("session daemon did not become ready in time");
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

pub async fn stop_session_daemon() -> Result<()> {
    let _: Value = request(SessionRequest::Shutdown).await?;
    Ok(())
}

pub async fn serve_session_daemon(socket: PathBuf) -> Result<()> {
    if socket.exists() {
        std::fs::remove_file(&socket).with_context(|| {
            format!("failed to remove stale session socket `{}`", socket.display())
        })?;
    }
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create session socket directory `{}`", parent.display())
        })?;
    }

    let listener = UnixListener::bind(&socket)
        .with_context(|| format!("failed to bind session socket `{}`", socket.display()))?;
    let mut session = LivePortalSession::open().await?;

    let serve_result = async {
        loop {
            let (mut stream, _) = listener.accept().await.context("failed to accept IPC client")?;
            let request = read_request(&mut stream).await?;
            let (response, should_shutdown) = handle_request(&mut session, request).await;
            write_response(&mut stream, response).await?;
            if should_shutdown {
                break;
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    session.shutdown().await.ok();
    std::fs::remove_file(&socket).ok();
    serve_result
}

pub async fn request<T: DeserializeOwned>(request: SessionRequest) -> Result<T> {
    let socket = socket_path()?;
    if !socket.exists() {
        bail!("no active portal session");
    }

    let mut stream = UnixStream::connect(&socket)
        .await
        .with_context(|| format!("failed to connect to session socket `{}`", socket.display()))?;
    let payload = serde_json::to_vec(&request)?;
    stream
        .write_all(&payload)
        .await
        .context("failed to write session IPC request")?;
    stream
        .shutdown()
        .await
        .context("failed to finish writing session IPC request")?;

    let mut response_bytes = Vec::new();
    stream
        .read_to_end(&mut response_bytes)
        .await
        .context("failed to read session IPC response")?;
    let response: SessionResponse =
        serde_json::from_slice(&response_bytes).context("failed to decode session IPC response")?;

    if !response.ok {
        bail!(
            "{}",
            response
                .error
                .unwrap_or_else(|| "session IPC request failed".to_owned())
        );
    }

    let value = response.result.unwrap_or(Value::Null);
    serde_json::from_value(value).context("failed to decode session IPC payload")
}

pub fn socket_path() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    Ok(base.join("kwin-portal-bridge").join("session.sock"))
}

async fn cleanup_stale_socket(socket: &Path) -> Result<()> {
    if !socket.exists() {
        return Ok(());
    }

    match UnixStream::connect(socket).await {
        Ok(_) => {}
        Err(_) => {
            std::fs::remove_file(socket).with_context(|| {
                format!("failed to remove stale session socket `{}`", socket.display())
            })?;
        }
    }

    Ok(())
}

async fn read_request(stream: &mut UnixStream) -> Result<SessionRequest> {
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .await
        .context("failed to read session IPC request")?;
    serde_json::from_slice(&bytes).context("failed to decode session IPC request")
}

async fn write_response(stream: &mut UnixStream, response: SessionResponse) -> Result<()> {
    let bytes = serde_json::to_vec(&response)?;
    stream
        .write_all(&bytes)
        .await
        .context("failed to write session IPC response")
}

async fn handle_request(
    session: &mut LivePortalSession,
    request: SessionRequest,
) -> (SessionResponse, bool) {
    let result = match request {
        SessionRequest::SessionInfo => respond_ok(session.info()),
        SessionRequest::Shutdown => {
            return (
                SessionResponse {
                    ok: true,
                    result: Some(Value::Null),
                    error: None,
                },
                true,
            );
        }
        SessionRequest::MovePointerAbsolute { stream, x, y } => {
            respond_async(session.move_pointer_absolute(stream, x, y).await)
        }
        SessionRequest::PointerButton { button, pressed } => {
            respond_async(session.pointer_button(button, pressed).await)
        }
        SessionRequest::KeyboardKeycode { keycode, pressed } => {
            respond_async(session.keyboard_keycode(keycode, pressed).await)
        }
        SessionRequest::ClickScreenPoint {
            screen,
            x,
            y,
            button,
            count,
        } => respond_async(session.click_screen_point(&screen, x, y, button, count).await),
        SessionRequest::ScrollScreenPoint {
            screen,
            x,
            y,
            dx,
            dy,
        } => respond_async(session.scroll_screen_point(&screen, x, y, dx, dy).await),
        SessionRequest::KeySequence { keycodes, repeat } => {
            respond_async(session.key_sequence(&keycodes, repeat).await)
        }
        SessionRequest::HoldKeyCodes {
            keycodes,
            duration_ms,
        } => respond_async(session.hold_key_codes(&keycodes, duration_ms).await),
        SessionRequest::DragScreenPoints {
            from_screen,
            from_x,
            from_y,
            to_screen,
            to_x,
            to_y,
        } => {
            respond_async(
                session
                    .drag_screen_points(&from_screen, from_x, from_y, &to_screen, to_x, to_y)
                    .await,
            )
        }
        SessionRequest::CaptureStillFrame { screen } => {
            respond_async(capture_still(session, &screen).await)
        }
        SessionRequest::CaptureZoom { screen, x, y, w, h } => {
            respond_async(capture_zoom(session, &screen, x, y, w, h).await)
        }
    };

    (result, false)
}

async fn capture_still(
    session: &mut LivePortalSession,
    screen: &ScreenInfo,
) -> Result<ScreenshotResult> {
    let captured = session.capture_screen_frame(screen).await?;
    screenshot_result_from_frame(screen, &captured.frame)
}

async fn capture_zoom(
    session: &mut LivePortalSession,
    screen: &ScreenInfo,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Result<ScreenshotCapture> {
    let captured = session.capture_screen_frame(screen).await?;
    zoom_result_from_frame(screen, &captured.frame, x, y, w, h)
}

fn respond_ok<T: Serialize>(value: T) -> SessionResponse {
    match serde_json::to_value(value) {
        Ok(result) => SessionResponse {
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => SessionResponse {
            ok: false,
            result: None,
            error: Some(format!("failed to serialize session response: {error}")),
        },
    }
}

fn respond_async<T: Serialize>(result: Result<T>) -> SessionResponse {
    match result {
        Ok(value) => respond_ok(value),
        Err(error) => SessionResponse {
            ok: false,
            result: None,
            error: Some(format!("{error:#}")),
        },
    }
}
