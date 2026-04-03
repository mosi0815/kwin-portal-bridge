mod capture;
mod cli;
mod daemon;
mod exclude_state;
mod executor;
mod json;
mod kwin;
mod model;
mod portal;
mod token_store;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::capture::CaptureBackend;
use crate::cli::{Cli, Command};
use crate::daemon::{serve_session_daemon, start_session_daemon, stop_session_daemon};
use crate::executor::ExecutorBackend;
use crate::json::print_json;
use crate::kwin::KWinBackend;
use crate::model::SessionBatchRequest;
use crate::portal::PortalBackend;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let cli = Cli::parse();
    let kwin = KWinBackend::new();
    let capture = CaptureBackend::new();
    let executor = ExecutorBackend::new()?;
    let portal = PortalBackend::new();

    match cli.command {
        Command::SessionStart => {
            print_json(&start_session_daemon().await?)?;
        }
        Command::SessionEnd => {
            stop_session_daemon().await?;
            print_json(&serde_json::json!({ "ended": true }))?;
        }
        Command::SessionBatch { json } => {
            let batch: SessionBatchRequest = serde_json::from_str(&json)?;
            print_json(&executor.session_batch(batch, &capture, &portal, &kwin).await?)?;
        }
        Command::Doctor => {
            print_json(&kwin.doctor()?)?;
        }
        Command::Screens => {
            print_json(&kwin.list_screens()?)?;
        }
        Command::Windows => {
            print_json(&kwin.list_windows()?)?;
        }
        Command::SetExclude { windows, value } => {
            print_json(&kwin.set_exclude_from_capture(&windows, value)?)?;
        }
        Command::PreviewHideSet {
            allowed_bundle_ids,
            host_bundle_id,
            display,
        } => {
            print_json(&executor.preview_hide_set(
                &allowed_bundle_ids,
                &host_bundle_id,
                display.as_deref(),
                &kwin,
            )?)?;
        }
        Command::FrontmostApp => {
            print_json(&executor.frontmost_app(&kwin)?)?;
        }
        Command::AppUnderPoint { x, y } => {
            print_json(&executor.app_under_point(x, y, &kwin)?)?;
        }
        Command::RaiseAllowedAtPoint {
            allowed_bundle_ids,
            host_bundle_id,
            x,
            y,
        } => {
            print_json(&executor.raise_allowed_window_at_point(
                &allowed_bundle_ids,
                &host_bundle_id,
                x,
                y,
                &kwin,
            )?)?;
        }
        Command::Click {
            allowed_bundle_ids,
            host_bundle_id,
            x,
            y,
            button,
            count,
        } => {
            print_json(
                &executor
                    .click(
                        &allowed_bundle_ids,
                        &host_bundle_id,
                        x,
                        y,
                        &button,
                        count,
                        &portal,
                        &kwin,
                    )
                    .await?,
            )?;
        }
        Command::Scroll {
            allowed_bundle_ids,
            host_bundle_id,
            x,
            y,
            dx,
            dy,
        } => {
            print_json(
                &executor
                    .scroll(
                        &allowed_bundle_ids,
                        &host_bundle_id,
                        x,
                        y,
                        dx,
                        dy,
                        &portal,
                        &kwin,
                    )
                    .await?,
            )?;
        }
        Command::KeySequence { keys, repeat } => {
            print_json(&executor.key_sequence(&keys, repeat, &portal).await?)?;
        }
        Command::HoldKey { keys, duration_ms } => {
            print_json(&executor.hold_keys(&keys, duration_ms, &portal).await?)?;
        }
        Command::Drag {
            allowed_bundle_ids,
            host_bundle_id,
            from_x,
            from_y,
            to_x,
            to_y,
        } => {
            print_json(
                &executor
                    .drag(
                        &allowed_bundle_ids,
                        &host_bundle_id,
                        from_x,
                        from_y,
                        to_x,
                        to_y,
                        &portal,
                        &kwin,
                    )
                    .await?,
            )?;
        }
        Command::LeftMouseDown => {
            print_json(&portal.left_mouse_down().await?)?;
        }
        Command::LeftMouseUp => {
            print_json(&portal.left_mouse_up().await?)?;
        }
        Command::PrepareForAction {
            allowed_bundle_ids,
            host_bundle_id,
            display,
        } => {
            print_json(&executor.prepare_for_action(
                &allowed_bundle_ids,
                &host_bundle_id,
                display.as_deref(),
                &kwin,
            )?)?;
        }
        Command::RestorePrepareState => {
            print_json(&executor.restore_prepare_state(&kwin)?)?;
        }
        Command::ResolvePrepareCapture {
            allowed_bundle_ids,
            host_bundle_id,
            display,
            do_hide,
        } => {
            print_json(
                &executor
                    .resolve_prepare_capture(
                        &allowed_bundle_ids,
                        &host_bundle_id,
                        display.as_deref(),
                        do_hide,
                        &capture,
                        &portal,
                        &kwin,
                    )
                    .await?,
            )?;
        }
        Command::Screenshot { display } => {
            print_json(
                &capture
                    .capture_still_frame(display.as_deref(), &portal, &kwin)
                    .await?,
            )?;
        }
        Command::Zoom { display, x, y, w, h } => {
            print_json(
                &capture
                    .capture_zoom(display.as_deref(), x, y, w, h, &portal, &kwin)
                    .await?,
            )?;
        }
        Command::PortalSession => {
            print_json(&portal.create_session().await?)?;
        }
        Command::MouseMove { x, y, stream } => {
            print_json(&portal.move_pointer_absolute(stream, x, y).await?)?;
        }
        Command::MouseButton { button, pressed } => {
            print_json(&portal.pointer_button(button, pressed).await?)?;
        }
        Command::Key { keycode, pressed } => {
            print_json(&portal.keyboard_keycode(keycode, pressed).await?)?;
        }
        Command::PipewireFrame {
            stream,
            poke_pointer,
        } => {
            print_json(&portal.read_first_frame(stream, poke_pointer).await?)?;
        }
        Command::SavePng { stream, output } => {
            print_json(&capture.save_png(&portal, stream, &output).await?)?;
        }
        Command::ServeSession { socket } => {
            serve_session_daemon(std::path::PathBuf::from(socket)).await?;
        }
    }

    Ok(())
}
