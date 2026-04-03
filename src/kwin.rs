use std::io::Write;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use dbus::blocking::{Connection, SyncConnection};
use dbus::channel::MatchingReceiver;
use dbus::message::MatchRule;
use serde::de::DeserializeOwned;

use crate::model::{DoctorReport, ExcludeUpdate, ScreenInfo, ToolPresence, WindowInfo};

const DBUS_TIMEOUT: Duration = Duration::from_secs(5);
const BRIDGE_PATH: &str = "/Bridge";
const BRIDGE_INTERFACE: &str = "org.kde.KWinPortalBridge";

pub struct KWinBackend;

impl KWinBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn doctor(&self) -> Result<DoctorReport> {
        let tools = ["qdbus6", "gdbus", "kwin_wayland", "xdg-desktop-portal"]
            .into_iter()
            .map(command_presence)
            .collect::<Result<Vec<_>>>()?;

        Ok(DoctorReport { tools })
    }

    pub fn list_screens(&self) -> Result<Vec<ScreenInfo>> {
        let payload = run_json_script("kwin-portal-bridge-screens", SCRIPTS.screens)?;
        parse_payload(payload)
    }

    pub fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        let payload = run_json_script("kwin-portal-bridge-windows", SCRIPTS.windows)?;
        parse_payload(payload)
    }

    pub fn set_exclude_from_capture(
        &self,
        windows: &[String],
        value: bool,
    ) -> Result<ExcludeUpdate> {
        if windows.is_empty() {
            bail!("at least one --window must be provided");
        }

        let args_json = serde_json::to_string(windows)?;
        let script = format!(
            "{}\nconst TARGET_WINDOWS = {args_json};\nconst TARGET_VALUE = {};\n{}",
            SCRIPT_HEADER, value, SCRIPT_SET_EXCLUDE
        );

        let payload = run_json_script("kwin-portal-bridge-exclude", &script)?;
        let updated: ExcludeUpdate = parse_payload(payload)?;

        if updated.windows.len() != windows.len() {
            bail!(
                "KWin updated {} window(s), but {} were requested",
                updated.windows.len(),
                windows.len()
            );
        }

        Ok(updated)
    }

    pub fn activate_window(&self, window_id: &str) -> Result<()> {
        let script = format!(
            "{}\nconst TARGET_WINDOW = {:?};\n{}",
            SCRIPT_HEADER, window_id, SCRIPT_ACTIVATE_WINDOW
        );

        run_script("kwin-portal-bridge-activate", &script, false)?;

        let activated = self
            .list_windows()?
            .into_iter()
            .find(|window| window.is_active)
            .map(|window| window.id)
            .ok_or_else(|| anyhow!("KWin did not report an active window after activation"))?;

        if activated != window_id {
            bail!("KWin activated `{activated}`, but `{window_id}` was requested");
        }

        Ok(())
    }
}

fn parse_payload<T: DeserializeOwned>(payload: String) -> Result<T> {
    serde_json::from_str(&payload).context("failed to decode KWin JSON payload")
}

fn command_presence(command: &str) -> Result<ToolPresence> {
    let output = Command::new("which")
        .arg(command)
        .output()
        .with_context(|| format!("failed to probe `{command}` with `which`"))?;

    let available = output.status.success();
    let path = if available {
        Some(String::from_utf8(output.stdout)?.trim().to_owned())
    } else {
        None
    };

    Ok(ToolPresence {
        command: command.to_owned(),
        available,
        path,
    })
}

fn run_json_script(script_name: &str, script_body: &str) -> Result<String> {
    let payload = run_script(script_name, script_body, true)?;
    payload.ok_or_else(|| anyhow!("KWin script finished without a result payload"))
}

fn run_script(script_name: &str, script_body: &str, require_result: bool) -> Result<Option<String>> {
    let kwin_conn =
        Connection::new_session().context("failed to connect to the session bus for KWin")?;
    let kwin_proxy = kwin_conn.with_proxy("org.kde.KWin", "/Scripting", DBUS_TIMEOUT);

    let receiver_conn =
        SyncConnection::new_session().context("failed to create a session bus receiver")?;
    let dbus_addr = receiver_conn.unique_name().to_string();
    let messages = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let message_sink = Arc::clone(&messages);

    let _receiver = receiver_conn.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |message, _connection| {
            if let Some(member) = message.member()
                && let Some(arg) = message.get1::<String>()
            {
                if let Ok(mut guard) = message_sink.lock() {
                    guard.push((member.to_string(), arg));
                }
            }
            true
        }),
    );

    let mut script_file = tempfile::NamedTempFile::with_prefix("kwin-portal-bridge-")?;
    script_file.write_all(render_script(&dbus_addr, script_body).as_bytes())?;
    let script_path = script_file.into_temp_path();

    let unique_name = format!("{script_name}-{}", unique_suffix());
    let (script_id,): (i32,) = kwin_proxy
        .method_call(
            "org.kde.kwin.Scripting",
            "loadScript",
            (script_path.to_str().unwrap(), unique_name),
        )
        .context("failed to load the temporary KWin script")?;

    if script_id < 0 {
        bail!("KWin refused to load script `{script_name}`");
    }

    let script_proxy = kwin_conn.with_proxy(
        "org.kde.KWin",
        format!("/Scripting/Script{script_id}"),
        DBUS_TIMEOUT,
    );

    let _: () = script_proxy
        .method_call("org.kde.kwin.Script", "run", ())
        .context("failed to run the KWin script")?;

    for _ in 0..20 {
        receiver_conn
            .process(Duration::from_millis(250))
            .context("failed while waiting for KWin script output")?;
    }

    if let Err(error) = script_proxy.method_call::<(), _, _, _>("org.kde.kwin.Script", "stop", ()) {
        let message = format!("{error:#}");
        if !message.contains("No such object path") {
            return Err(error).context("failed to stop the KWin script");
        }
    }

    let received = messages
        .lock()
        .map_err(|_| anyhow!("message receiver lock poisoned"))?;

    if let Some((_, error)) = received.iter().find(|(kind, _)| kind == "error") {
        bail!("KWin script error: {error}");
    }

    let payload = received
        .iter()
        .find(|(kind, _)| kind == "result")
        .map(|(_, payload)| payload.clone());

    if require_result && payload.is_none() {
        bail!("KWin script finished without a result payload");
    }

    Ok(payload)
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or_default()
}

fn render_script(dbus_addr: &str, script_body: &str) -> String {
    format!(
        "{SCRIPT_HEADER}\nconst DBUS_DESTINATION = {dbus_addr:?};\nconst BRIDGE_PATH = {BRIDGE_PATH:?};\nconst BRIDGE_INTERFACE = {BRIDGE_INTERFACE:?};\n{script_body}\n"
    )
}

const SCRIPT_HEADER: &str = r#"
function bridgeRect(geometry) {
    return {
        x: Math.round(geometry.x),
        y: Math.round(geometry.y),
        width: Math.round(geometry.width),
        height: Math.round(geometry.height)
    };
}

function bridgeEmit(kind, payload) {
    callDBus(
        DBUS_DESTINATION,
        BRIDGE_PATH,
        BRIDGE_INTERFACE,
        kind,
        JSON.stringify(payload)
    );
}

function bridgeResult(payload) {
    bridgeEmit("result", payload);
}

function bridgeError(message) {
    bridgeEmit("error", { message: message });
}
"#;

struct Scripts<'a> {
    screens: &'a str,
    windows: &'a str,
}

const SCRIPTS: Scripts<'static> = Scripts {
    screens: SCRIPT_SCREENS,
    windows: SCRIPT_WINDOWS,
};

const SCRIPT_SCREENS: &str = r#"
try {
    const screens = workspace.screens.map((screen, index) => ({
        id: screen.name || `screen-${index}`,
        name: screen.name || `Screen ${index + 1}`,
        geometry: bridgeRect(screen.geometry),
        scale: typeof screen.devicePixelRatio === "number" ? screen.devicePixelRatio : screen.scale,
        refresh_millihz: screen.refreshRate,
        is_active: workspace.activeScreen === screen,
        is_primary: index === 0
    }));
    bridgeResult(screens);
} catch (error) {
    bridgeError(String(error));
}
"#;

const SCRIPT_WINDOWS: &str = r#"
try {
    const windows = workspace.windowList().map((window, index) => ({
        id: String(window.internalId),
        title: window.caption || "",
        geometry: bridgeRect(window.frameGeometry),
        pid: window.pid || null,
        desktop_file_name: window.desktopFileName || null,
        resource_class: window.resourceClass || null,
        resource_name: window.resourceName || null,
        window_role: window.windowRole || null,
        window_type: window.windowType ? String(window.windowType) : null,
        is_dock: typeof window.dock === "boolean" ? window.dock : null,
        is_desktop: typeof window.desktopWindow === "boolean" ? window.desktopWindow : null,
        is_visible: typeof window.visible === "boolean" ? window.visible : null,
        is_minimized: typeof window.minimized === "boolean" ? window.minimized : null,
        is_normal_window: typeof window.normalWindow === "boolean" ? window.normalWindow : null,
        is_dialog: typeof window.dialog === "boolean" ? window.dialog : null,
        output: window.output ? window.output.name : null,
        stacking_order: typeof window.stackingOrder === "number" ? window.stackingOrder : index,
        is_active: workspace.activeWindow === window,
        exclude_from_capture: !!window.excludeFromCapture
    }));
    bridgeResult(windows);
} catch (error) {
    bridgeError(String(error));
}
"#;

const SCRIPT_SET_EXCLUDE: &str = r#"
try {
    const changed = [];
    workspace.windowList().forEach((window) => {
        const id = String(window.internalId);
        if (TARGET_WINDOWS.indexOf(id) !== -1) {
            window.excludeFromCapture = TARGET_VALUE;
            changed.push(id);
        }
    });
    bridgeResult({
        windows: changed,
        value: TARGET_VALUE
    });
} catch (error) {
    bridgeError(String(error));
}
"#;

const SCRIPT_ACTIVATE_WINDOW: &str = r#"
try {
    let target = null;
    workspace.windowList().forEach((window) => {
        if (String(window.internalId) === TARGET_WINDOW) {
            target = window;
        }
    });

    if (!target) {
        throw new Error(`No window found for id ${TARGET_WINDOW}`);
    }

    workspace.activeWindow = target;
} catch (error) {
    bridgeError(String(error));
}
"#;
