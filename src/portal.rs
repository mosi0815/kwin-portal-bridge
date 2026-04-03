use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{sync::mpsc as std_mpsc, time::Duration};

use anyhow::{Context, Result, bail};
use ashpd::desktop::PersistMode;
use ashpd::desktop::screencast::CursorMode;
use lamco_pipewire::{
    FrameBuffer, PipeWireThreadCommand, PipeWireThreadManager, PixelFormat,
    SourceType as PwSourceType, StreamConfig as PwStreamConfig, StreamInfo as PwStreamInfo,
    VideoFrame,
};
use lamco_portal::{PortalConfig, PortalManager, PortalSessionHandle};

use crate::model::{
    CapturedFrame, FrameInfo, FrameProbeResult, PortalActionResult, PortalSessionInfo,
    PortalStream, ScreenInfo, StreamSelection,
};
use crate::token_store::TokenStore;

pub struct PortalBackend;

impl PortalBackend {
    pub fn new() -> Self {
        Self
    }

    pub async fn create_session(&self) -> Result<PortalSessionInfo> {
        let (manager, session, restore_token) = start_session().await?;
        let info = session_info(&session, restore_token);
        manager.cleanup().await.ok();
        drop(session);
        Ok(info)
    }

    pub async fn move_pointer_absolute(
        &self,
        stream: Option<u32>,
        x: f64,
        y: f64,
    ) -> Result<PortalActionResult> {
        let (manager, session, restore_token) = start_session().await?;
        let stream = selected_stream(&session, stream)?;

        manager
            .remote_desktop()
            .notify_pointer_motion_absolute(session.ashpd_session(), stream.stream.node_id, x, y)
            .await
            .context("failed to send absolute pointer motion through the portal")?;

        let result = PortalActionResult {
            action: "mouse-move".to_owned(),
            session: session_info(&session, restore_token),
            target_stream: Some(stream),
        };

        manager.cleanup().await.ok();
        drop(session);
        Ok(result)
    }

    pub async fn pointer_button(&self, button: i32, pressed: bool) -> Result<PortalActionResult> {
        let (manager, session, restore_token) = start_session().await?;
        manager
            .remote_desktop()
            .notify_pointer_button(session.ashpd_session(), button, pressed)
            .await
            .context("failed to send pointer button event through the portal")?;

        let result = PortalActionResult {
            action: if pressed {
                "mouse-button-press".to_owned()
            } else {
                "mouse-button-release".to_owned()
            },
            session: session_info(&session, restore_token),
            target_stream: None,
        };

        manager.cleanup().await.ok();
        drop(session);
        Ok(result)
    }

    pub async fn click_screen_point(
        &self,
        screen: &ScreenInfo,
        x: i32,
        y: i32,
        button: i32,
        count: u32,
    ) -> Result<PortalActionResult> {
        let (manager, session, restore_token) = start_session().await?;
        let info = session_info(&session, restore_token);
        let target_stream = match_stream_to_screen(&info.streams, screen)?;
        let (local_x, local_y) = local_stream_point(screen, &target_stream, x, y)?;

        manager
            .remote_desktop()
            .notify_pointer_motion_absolute(
                session.ashpd_session(),
                target_stream.stream.node_id,
                local_x,
                local_y,
            )
            .await
            .context("failed to move pointer before click")?;

        for _ in 0..count.max(1) {
            manager
                .remote_desktop()
                .notify_pointer_button(session.ashpd_session(), button, true)
                .await
                .context("failed to send pointer press through the portal")?;
            manager
                .remote_desktop()
                .notify_pointer_button(session.ashpd_session(), button, false)
                .await
                .context("failed to send pointer release through the portal")?;
        }

        let result = PortalActionResult {
            action: "click".to_owned(),
            session: info,
            target_stream: Some(target_stream),
        };

        manager.cleanup().await.ok();
        drop(session);
        Ok(result)
    }

    pub async fn scroll_screen_point(
        &self,
        screen: &ScreenInfo,
        x: i32,
        y: i32,
        dx: f64,
        dy: f64,
    ) -> Result<PortalActionResult> {
        let (manager, session, restore_token) = start_session().await?;
        let info = session_info(&session, restore_token);
        let target_stream = match_stream_to_screen(&info.streams, screen)?;
        let (local_x, local_y) = local_stream_point(screen, &target_stream, x, y)?;

        manager
            .remote_desktop()
            .notify_pointer_motion_absolute(
                session.ashpd_session(),
                target_stream.stream.node_id,
                local_x,
                local_y,
            )
            .await
            .context("failed to move pointer before scroll")?;

        manager
            .remote_desktop()
            .notify_pointer_axis(session.ashpd_session(), dx, dy)
            .await
            .context("failed to send scroll event through the portal")?;

        let result = PortalActionResult {
            action: "scroll".to_owned(),
            session: info,
            target_stream: Some(target_stream),
        };

        manager.cleanup().await.ok();
        drop(session);
        Ok(result)
    }

    pub async fn keyboard_keycode(
        &self,
        keycode: i32,
        pressed: bool,
    ) -> Result<PortalActionResult> {
        let (manager, session, restore_token) = start_session().await?;
        manager
            .remote_desktop()
            .notify_keyboard_keycode(session.ashpd_session(), keycode, pressed)
            .await
            .context("failed to send keyboard keycode through the portal")?;

        let result = PortalActionResult {
            action: if pressed {
                "key-press".to_owned()
            } else {
                "key-release".to_owned()
            },
            session: session_info(&session, restore_token),
            target_stream: None,
        };

        manager.cleanup().await.ok();
        drop(session);
        Ok(result)
    }

    pub async fn key_sequence(&self, keycodes: &[i32], repeat: u32) -> Result<PortalActionResult> {
        let (manager, session, restore_token) = start_session().await?;

        for _ in 0..repeat.max(1) {
            for &keycode in keycodes {
                manager
                    .remote_desktop()
                    .notify_keyboard_keycode(session.ashpd_session(), keycode, true)
                    .await
                    .with_context(|| format!("failed to press keycode {keycode} through the portal"))?;
            }

            for &keycode in keycodes.iter().rev() {
                manager
                    .remote_desktop()
                    .notify_keyboard_keycode(session.ashpd_session(), keycode, false)
                    .await
                    .with_context(|| format!("failed to release keycode {keycode} through the portal"))?;
            }
        }

        let result = PortalActionResult {
            action: "key-sequence".to_owned(),
            session: session_info(&session, restore_token),
            target_stream: None,
        };

        manager.cleanup().await.ok();
        drop(session);
        Ok(result)
    }

    pub async fn hold_key_codes(
        &self,
        keycodes: &[i32],
        duration_ms: u64,
    ) -> Result<PortalActionResult> {
        let (manager, session, restore_token) = start_session().await?;

        for &keycode in keycodes {
            manager
                .remote_desktop()
                .notify_keyboard_keycode(session.ashpd_session(), keycode, true)
                .await
                .with_context(|| format!("failed to press keycode {keycode} through the portal"))?;
        }

        tokio::time::sleep(Duration::from_millis(duration_ms)).await;

        for &keycode in keycodes.iter().rev() {
            manager
                .remote_desktop()
                .notify_keyboard_keycode(session.ashpd_session(), keycode, false)
                .await
                .with_context(|| format!("failed to release keycode {keycode} through the portal"))?;
        }

        let result = PortalActionResult {
            action: "hold-key".to_owned(),
            session: session_info(&session, restore_token),
            target_stream: None,
        };

        manager.cleanup().await.ok();
        drop(session);
        Ok(result)
    }

    pub async fn read_first_frame(
        &self,
        stream: Option<u32>,
        poke_pointer: bool,
    ) -> Result<FrameProbeResult> {
        let captured = self
            .capture_raw_frame_with_options(stream, poke_pointer)
            .await?;
        Ok(FrameProbeResult {
            session: captured.session,
            target_stream: captured.target_stream,
            frame: frame_info(&captured.frame),
        })
    }

    pub async fn capture_raw_frame(&self, stream: Option<u32>) -> Result<CapturedFrame> {
        self.capture_raw_frame_with_options(stream, false).await
    }

    pub async fn capture_screen_frame(
        &self,
        screen: &crate::model::ScreenInfo,
    ) -> Result<CapturedFrame> {
        let (manager, session, restore_token) = start_session().await?;
        let info = session_info(&session, restore_token);
        let target_stream = match_stream_to_screen(&info.streams, screen)?;
        let fd = dup_fd(session.pipewire_fd())?;
        let pw_stream = to_pipewire_stream(&target_stream);
        let frame = tokio::task::spawn_blocking(move || read_one_pipewire_frame(fd, pw_stream))
            .await
            .context("PipeWire frame worker task failed to join")??;

        manager.cleanup().await.ok();
        drop(session);

        let frame_byte_len = match &frame.buffer {
            FrameBuffer::Memory(data) => data.len(),
            FrameBuffer::DmaBuf(_) => 0,
        };

        Ok(CapturedFrame {
            session: info,
            target_stream,
            frame,
            frame_byte_len,
        })
    }

    async fn poke_pointer(
        &self,
        manager: &PortalManager,
        session: &PortalSessionHandle,
        target_stream: &StreamSelection,
    ) -> Result<()> {
        let width = f64::from(target_stream.stream.size[0].max(2));
        let height = f64::from(target_stream.stream.size[1].max(2));
        let x = (width / 2.0).floor();
        let y = (height / 2.0).floor();

        manager
            .remote_desktop()
            .notify_pointer_motion_absolute(
                session.ashpd_session(),
                target_stream.stream.node_id,
                x,
                y,
            )
            .await
            .context("failed to move pointer for PipeWire poke")?;

        manager
            .remote_desktop()
            .notify_pointer_motion(session.ashpd_session(), 1.0, 0.0)
            .await
            .context("failed to send relative pointer poke")?;

        manager
            .remote_desktop()
            .notify_pointer_motion(session.ashpd_session(), -1.0, 0.0)
            .await
            .context("failed to restore pointer after poke")?;

        Ok(())
    }

    async fn capture_raw_frame_with_options(
        &self,
        stream: Option<u32>,
        poke_pointer: bool,
    ) -> Result<CapturedFrame> {
        let (manager, session, restore_token) = start_session().await?;
        let info = session_info(&session, restore_token);
        let target_stream = selected_stream(&session, stream)?;
        let fd = dup_fd(session.pipewire_fd())?;
        let pw_stream = to_pipewire_stream(&target_stream);
        let frame_task =
            tokio::task::spawn_blocking(move || read_one_pipewire_frame(fd, pw_stream));

        if poke_pointer {
            tokio::time::sleep(Duration::from_millis(300)).await;
            self.poke_pointer(&manager, &session, &target_stream)
                .await
                .ok();
        }

        let frame = frame_task
            .await
            .context("PipeWire frame worker task failed to join")??;

        manager.cleanup().await.ok();
        drop(session);

        let frame_byte_len = match &frame.buffer {
            FrameBuffer::Memory(data) => data.len(),
            FrameBuffer::DmaBuf(_) => 0,
        };

        Ok(CapturedFrame {
            session: info,
            target_stream,
            frame,
            frame_byte_len,
        })
    }
}

async fn start_session() -> Result<(PortalManager, PortalSessionHandle, Option<String>)> {
    let token_store = TokenStore::new()?;
    let saved_token = token_store.load().unwrap_or(None);

    match try_start_session(saved_token.clone(), true).await {
        Ok((manager, session, restore_token)) => {
            if let Some(ref token) = restore_token {
                token_store.save(token).ok();
            }
            Ok((manager, session, restore_token))
        }
        Err(error) if is_persistence_rejection(&error) => {
            let (manager, session, restore_token) = try_start_session(None, false)
                .await
                .context("failed to create session after persistence fallback")?;

            if let Some(ref token) = restore_token {
                token_store.save(token).ok();
            }

            Ok((manager, session, restore_token))
        }
        Err(error) => Err(error),
    }
}

async fn try_start_session(
    restore_token: Option<String>,
    with_persistence: bool,
) -> Result<(PortalManager, PortalSessionHandle, Option<String>)> {
    let manager = PortalManager::new(default_config(restore_token, with_persistence)).await?;
    let session_id = generate_session_id()?;
    let (session, restore_token) = manager
        .create_session(session_id, None)
        .await
        .context("failed to create combined RemoteDesktop/ScreenCast portal session")?;

    Ok((manager, session, restore_token))
}

fn default_config(restore_token: Option<String>, with_persistence: bool) -> PortalConfig {
    let mut config = PortalConfig::default();
    config.cursor_mode = CursorMode::Embedded;
    config.restore_token = restore_token;
    config.persist_mode = if with_persistence {
        PersistMode::ExplicitlyRevoked
    } else {
        PersistMode::DoNot
    };
    config
}

fn generate_session_id() -> Result<String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_millis();

    Ok(format!("kwin-portal-bridge-{timestamp}"))
}

fn session_info(session: &PortalSessionHandle, restore_token: Option<String>) -> PortalSessionInfo {
    PortalSessionInfo {
        session_id: session.session_id().to_owned(),
        pipewire_fd: session.pipewire_fd().as_raw_fd(),
        restore_token,
        remote_desktop_session: session.remote_desktop_session().map(ToOwned::to_owned),
        streams: session
            .streams()
            .iter()
            .map(|stream| PortalStream {
                node_id: stream.node_id,
                source_type: format!("{:?}", stream.source_type),
                position: [stream.position.0, stream.position.1],
                size: [
                    i32::try_from(stream.size.0).unwrap_or(i32::MAX),
                    i32::try_from(stream.size.1).unwrap_or(i32::MAX),
                ],
            })
            .collect(),
    }
}

fn selected_stream(
    session: &PortalSessionHandle,
    stream_node: Option<u32>,
) -> Result<StreamSelection> {
    let selected = if let Some(node_id) = stream_node {
        session
            .streams()
            .iter()
            .find(|stream| stream.node_id == node_id)
            .ok_or_else(|| {
                anyhow::anyhow!("portal stream node {node_id} was not returned by the session")
            })?
    } else {
        session
            .streams()
            .first()
            .ok_or_else(|| anyhow::anyhow!("portal session returned no streams"))?
    };

    if selected.size.0 <= 0 || selected.size.1 <= 0 {
        bail!(
            "portal stream {} has invalid logical size",
            selected.node_id
        );
    }

    Ok(StreamSelection {
        stream: PortalStream {
            node_id: selected.node_id,
            source_type: format!("{:?}", selected.source_type),
            position: [selected.position.0, selected.position.1],
            size: [
                i32::try_from(selected.size.0).unwrap_or(i32::MAX),
                i32::try_from(selected.size.1).unwrap_or(i32::MAX),
            ],
        },
    })
}

fn to_pipewire_stream(stream: &StreamSelection) -> PwStreamInfo {
    PwStreamInfo {
        node_id: stream.stream.node_id,
        position: (stream.stream.position[0], stream.stream.position[1]),
        size: (
            u32::try_from(stream.stream.size[0]).unwrap_or_default(),
            u32::try_from(stream.stream.size[1]).unwrap_or_default(),
        ),
        source_type: match stream.stream.source_type.as_str() {
            "Window" => PwSourceType::Window,
            "Virtual" => PwSourceType::Virtual,
            _ => PwSourceType::Monitor,
        },
    }
}

fn read_one_pipewire_frame(fd: OwnedFd, stream_info: PwStreamInfo) -> Result<VideoFrame> {
    let mut manager = PipeWireThreadManager::new(fd.as_raw_fd())?;
    std::mem::forget(fd);

    let (response_tx, response_rx) = std_mpsc::sync_channel(1);
    let config = PwStreamConfig {
        name: format!("monitor-{}", stream_info.node_id),
        width: stream_info.size.0,
        height: stream_info.size.1,
        framerate: 60,
        use_dmabuf: false,
        buffer_count: 3,
        preferred_format: Some(PixelFormat::BGRx),
        dmabuf_passthrough: false,
    };

    manager.send_command(PipeWireThreadCommand::CreateStream {
        stream_id: stream_info.node_id,
        node_id: stream_info.node_id,
        config,
        response_tx,
    })?;

    response_rx
        .recv()
        .context("PipeWire create-stream response channel closed")?
        .context("PipeWire rejected stream creation")?;

    let frame = if let Some(frame) = manager.recv_frame_timeout(Duration::from_secs(10)) {
        frame
    } else {
        let states = manager
            .drain_state_events()
            .into_iter()
            .map(|event| format!("stream {} -> {:?}", event.stream_id, event.state))
            .collect::<Vec<_>>();

        let states_suffix = if states.is_empty() {
            "no state events observed".to_owned()
        } else {
            format!("state events: {}", states.join(", "))
        };

        manager.shutdown().ok();
        bail!("timed out waiting for first PipeWire frame ({states_suffix})");
    };

    manager.shutdown()?;
    Ok(frame)
}

fn dup_fd(raw_fd: i32) -> Result<OwnedFd> {
    let duplicated = unsafe { libc::dup(raw_fd) };
    if duplicated < 0 {
        return Err(std::io::Error::last_os_error())
            .context("failed to duplicate portal PipeWire file descriptor");
    }

    let owned = unsafe { OwnedFd::from_raw_fd(duplicated) };
    Ok(owned)
}

fn frame_info(frame: &VideoFrame) -> FrameInfo {
    let (buffer_kind, bytes, dmabuf_planes) = match &frame.buffer {
        FrameBuffer::Memory(data) => ("memory".to_owned(), Some(data.len()), None),
        FrameBuffer::DmaBuf(descriptor) => {
            ("dmabuf".to_owned(), None, Some(descriptor.planes.len()))
        }
    };

    FrameInfo {
        frame_id: frame.frame_id,
        width: frame.width,
        height: frame.height,
        stride: frame.stride,
        format: format!("{:?}", frame.format),
        buffer_kind,
        bytes,
        dmabuf_planes,
        flags: frame.flags.bits(),
        damage_regions: frame.damage_regions.len(),
    }
}

fn local_stream_point(
    screen: &ScreenInfo,
    target_stream: &StreamSelection,
    x: i32,
    y: i32,
) -> Result<(f64, f64)> {
    if !point_in_screen(screen, x, y) {
        bail!(
            "point {x},{y} is outside display `{}` bounds",
            screen.id
        );
    }

    let local_x = x - screen.geometry.x;
    let local_y = y - screen.geometry.y;
    let logical_w = screen.geometry.width.max(1) as f64;
    let logical_h = screen.geometry.height.max(1) as f64;
    let stream_w = target_stream.stream.size[0].max(1) as f64;
    let stream_h = target_stream.stream.size[1].max(1) as f64;

    Ok((
        ((local_x as f64) / logical_w) * stream_w,
        ((local_y as f64) / logical_h) * stream_h,
    ))
}

fn is_persistence_rejection(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}");
    message.contains("cannot persist") || message.contains("InvalidArgument")
}

fn match_stream_to_screen(
    streams: &[PortalStream],
    screen: &crate::model::ScreenInfo,
) -> Result<StreamSelection> {
    let logical_w = screen.geometry.width;
    let logical_h = screen.geometry.height;
    let scale = screen.scale.unwrap_or(1.0);
    let physical_w = ((logical_w as f64) * scale).round() as i32;
    let physical_h = ((logical_h as f64) * scale).round() as i32;

    let exact = streams.iter().find(|stream| {
        stream.position[0] == screen.geometry.x
            && stream.position[1] == screen.geometry.y
            && (stream.size[0] == logical_w || stream.size[0] == physical_w)
            && (stream.size[1] == logical_h || stream.size[1] == physical_h)
    });

    let fallback = streams.iter().find(|stream| {
        stream.position[0] == screen.geometry.x && stream.position[1] == screen.geometry.y
    });

    let chosen = exact
        .or(fallback)
        .ok_or_else(|| anyhow::anyhow!("no portal stream matched screen `{}`", screen.id))?;

    Ok(StreamSelection {
        stream: chosen.clone(),
    })
}

fn point_in_screen(screen: &ScreenInfo, x: i32, y: i32) -> bool {
    x >= screen.geometry.x
        && x < screen.geometry.x.saturating_add(screen.geometry.width)
        && y >= screen.geometry.y
        && y < screen.geometry.y.saturating_add(screen.geometry.height)
}
