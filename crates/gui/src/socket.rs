use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use eframe::egui::Context;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleCommand {
    Toggle,
    Show,
    Hide,
}

pub struct ToggleListener {
    events: Receiver<ToggleCommand>,
    _handle: Option<JoinHandle<()>>,
}

impl ToggleListener {
    pub fn try_recv(&self) -> Option<ToggleCommand> {
        self.events.try_recv().ok()
    }
}

pub fn socket_path() -> PathBuf {
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime).join("fc27le-gui.sock");
    }
    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
    PathBuf::from(format!("/tmp/fc27le-gui-{user}.sock"))
}

pub fn start_listener(repaint: Context) -> Result<ToggleListener, String> {
    let path = socket_path();
    if path.exists() {
        if UnixStream::connect(&path).is_ok() {
            return Err(format!(
                "another FC27LE GUI is already listening at {}",
                path.display()
            ));
        }
        let _ = std::fs::remove_file(&path);
    }
    let listener = UnixListener::bind(&path)
        .map_err(|error| format!("bind toggle socket {}: {error}", path.display()))?;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    let (tx, rx) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("fc27le-toggle".to_string())
        .spawn(move || accept_loop(listener, tx, repaint))
        .map_err(|error| format!("spawn toggle listener: {error}"))?;
    Ok(ToggleListener {
        events: rx,
        _handle: Some(handle),
    })
}

fn accept_loop(listener: UnixListener, events: Sender<ToggleCommand>, repaint: Context) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        let mut request = String::new();
        let _ = stream.read_to_string(&mut request);
        match request.trim() {
            "toggle" => {
                let _ = events.send(ToggleCommand::Toggle);
            }
            "show" => {
                let _ = events.send(ToggleCommand::Show);
            }
            "hide" => {
                let _ = events.send(ToggleCommand::Hide);
            }
            _ => {}
        }
        repaint.request_repaint();
    }
}

pub fn send_command(command: &str) -> Result<(), String> {
    let path = socket_path();
    let mut stream = UnixStream::connect(&path).map_err(|error| {
        format!(
            "no running FC27LE GUI at {} ({error}); launch fc27le-gui first",
            path.display()
        )
    })?;
    stream
        .write_all(command.as_bytes())
        .and_then(|()| stream.write_all(b"\n"))
        .map_err(|error| format!("send toggle command: {error}"))?;
    let _ = stream.flush();
    Ok(())
}
