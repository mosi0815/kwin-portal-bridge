mod capture;
mod cli;
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
use crate::executor::ExecutorBackend;
use crate::json::print_json;
use crate::kwin::KWinBackend;
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
    }

    Ok(())
}
