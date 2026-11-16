//! Background Lua worker. The GUI never executes Lua on its render thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use crate::api::{LuaContext, LuaEvent, ScriptJob, ScriptSession, build_live_context, run_script};

pub enum LuaCommand {
    SetSession(Box<ScriptSession>),
    Run(Box<ScriptJob>),
    Shutdown,
}

pub struct LuaWorker {
    commands: Sender<LuaCommand>,
    events: Receiver<LuaEvent>,
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl LuaWorker {
    pub fn spawn() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let handle = thread::Builder::new()
            .name("fc27le-lua".to_string())
            .spawn(move || worker_loop(command_rx, event_tx, worker_cancel))
            .expect("spawn Lua worker");
        Self {
            commands: command_tx,
            events: event_rx,
            cancel,
            handle: Some(handle),
        }
    }

    pub fn set_session(&self, session: ScriptSession) {
        let _ = self
            .commands
            .send(LuaCommand::SetSession(Box::new(session)));
    }

    pub fn run(&self, job: ScriptJob) {
        let _ = self.commands.send(LuaCommand::Run(Box::new(job)));
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn try_recv(&self) -> Option<LuaEvent> {
        self.events.try_recv().ok()
    }

    pub fn shutdown(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        let _ = self.commands.send(LuaCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for LuaWorker {
    fn drop(&mut self) {
        if self.handle.is_some() {
            self.shutdown();
        }
    }
}

fn worker_loop(commands: Receiver<LuaCommand>, events: Sender<LuaEvent>, cancel: Arc<AtomicBool>) {
    let mut context: Option<LuaContext> = None;
    while let Ok(command) = commands.recv() {
        match command {
            LuaCommand::SetSession(session) => {
                context = None;
                match build_live_context(&session, cancel.clone(), events.clone()) {
                    Ok(new_context) => {
                        let pid = new_context.pid;
                        let tables = new_context.tables;
                        context = Some(new_context);
                        let _ = events.send(LuaEvent::SessionReady { pid, tables });
                    }
                    Err(error) => {
                        let _ = events.send(LuaEvent::SessionError {
                            message: error.to_string(),
                        });
                    }
                }
            }
            LuaCommand::Run(job) => {
                let Some(context) = context.as_ref() else {
                    let _ = events.send(LuaEvent::Finished {
                        success: false,
                        error: Some(
                            "no active Lua database session; connect to FC 27 and load metadata first"
                                .to_string(),
                        ),
                    });
                    continue;
                };
                let _ = events.send(LuaEvent::Started {
                    name: job.name.clone(),
                });
                match run_script(&context.lua, &job, cancel.clone()) {
                    Ok(()) => {
                        let _ = events.send(LuaEvent::Finished {
                            success: true,
                            error: None,
                        });
                    }
                    Err(error) => {
                        let _ = events.send(LuaEvent::Finished {
                            success: false,
                            error: Some(error),
                        });
                    }
                }
            }
            LuaCommand::Shutdown => break,
        }
    }
}
