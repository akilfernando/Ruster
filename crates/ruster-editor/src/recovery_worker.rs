//! One serial recovery worker. The UI never queues copied checkpoint states.
use crate::recovery::{Journal, Snapshot, Store};
use eframe::egui;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
    Arc,
};
use std::thread::{self, JoinHandle};

pub(super) type Capture = Box<dyn FnOnce() -> Result<Snapshot, String> + Send>;
pub(super) enum Command {
    Checkpoint {
        epoch: u64,
        capture: Capture,
    },
    Boundary {
        id: u64,
        epoch: u64,
        adopt: Option<Journal>,
    },
    Shutdown,
}
pub(super) enum Reply {
    Checkpoint {
        epoch: u64,
        result: Result<(), String>,
    },
    Boundary {
        id: u64,
        epoch: u64,
        result: Result<(), String>,
    },
    Shutdown,
}
pub(super) struct Worker {
    sender: Option<SyncSender<Command>>,
    receiver: Receiver<Reply>,
    epoch: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start(store: Store, ctx: Option<egui::Context>) -> Result<Self, String> {
        let (sender, commands) = mpsc::sync_channel(1);
        let (replies, receiver) = mpsc::sync_channel(2);
        let epoch = Arc::new(AtomicU64::new(0));
        let current = epoch.clone();
        let thread = thread::Builder::new()
            .name("ruster-recovery".into())
            .spawn(move || {
                let mut journal: Option<Journal> = None;
                while let Ok(command) = commands.recv() {
                    let (reply, stop) = match command {
                        Command::Checkpoint { epoch, capture } => {
                            let result = if current.load(Ordering::Acquire) != epoch {
                                Ok(())
                            } else {
                                capture().and_then(|snapshot| {
                                    if current.load(Ordering::Acquire) != epoch {
                                        return Ok(());
                                    }
                                    if journal.is_none() {
                                        journal = Some(store.allocate()?);
                                    }
                                    // A boundary may be requested once this write has
                                    // entered; the serial boundary retires it afterward.
                                    journal.as_mut().unwrap().checkpoint(snapshot)
                                })
                            };
                            (Reply::Checkpoint { epoch, result }, false)
                        }
                        Command::Boundary { id, epoch, adopt } => {
                            let result = journal.take().map(Journal::retire).unwrap_or(Ok(()));
                            journal = adopt;
                            (Reply::Boundary { id, epoch, result }, false)
                        }
                        Command::Shutdown => (Reply::Shutdown, true),
                    };
                    if replies.send(reply).is_err() {
                        break;
                    }
                    if let Some(ctx) = &ctx {
                        ctx.request_repaint();
                    }
                    if stop {
                        break;
                    }
                }
                // Disconnect/crash simulation deliberately keeps the last checkpoint.
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            epoch,
            thread: Some(thread),
        })
    }
    pub fn set_epoch(&self, epoch: u64) {
        self.epoch.store(epoch, Ordering::Release);
    }
    pub fn send(&self, command: Command) -> Result<(), TrySendError<Command>> {
        match &self.sender {
            Some(sender) => sender.try_send(command),
            None => Err(TrySendError::Disconnected(command)),
        }
    }
    pub fn poll(&self) -> Result<Reply, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
    #[cfg(test)]
    pub fn wait(&self) -> Result<Reply, mpsc::RecvTimeoutError> {
        self.receiver
            .recv_timeout(std::time::Duration::from_secs(5))
    }
    #[cfg(test)]
    pub fn disconnect_wait(&mut self) {
        self.sender = None;
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
    pub fn finished(&mut self) -> bool {
        if self
            .thread
            .as_ref()
            .is_some_and(|thread| thread.is_finished())
        {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            self.sender = None;
            true
        } else {
            self.thread.is_none()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        Snapshot::capture(
            &ruster_scene::Session::new(ruster_scene::Scene::demo()).unwrap(),
            None,
            &serde_json::json!({}),
        )
        .unwrap()
    }
    #[test]
    fn epoch_boundaries_adopt_retire_and_ack_before_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("recovery")).unwrap();
        let mut adopted = store.allocate().unwrap();
        adopted.checkpoint(snapshot()).unwrap();
        let mut worker = Worker::start(store.clone(), None).unwrap();
        let (started, begin) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        assert!(worker
            .send(Command::Checkpoint {
                epoch: 0,
                capture: Box::new(move || {
                    started.send(()).unwrap();
                    gate.recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    Ok(snapshot())
                })
            })
            .is_ok());
        begin
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        worker.set_epoch(1);
        assert!(worker
            .send(Command::Boundary {
                id: 10,
                epoch: 1,
                adopt: Some(adopted)
            })
            .is_ok());
        release.send(()).unwrap();
        assert!(matches!(
            worker.wait().unwrap(),
            Reply::Checkpoint {
                epoch: 0,
                result: Ok(())
            }
        ));
        assert!(matches!(
            worker.wait().unwrap(),
            Reply::Boundary {
                id: 10,
                epoch: 1,
                result: Ok(())
            }
        ));
        assert!(
            store.scan().unwrap().is_empty(),
            "adoption retains the claimed lease"
        );
        // The old epoch cannot capture/publish after the acknowledged boundary.
        assert!(worker
            .send(Command::Checkpoint {
                epoch: 0,
                capture: Box::new(|| panic!("obsolete capture executed"))
            })
            .is_ok());
        assert!(matches!(
            worker.wait().unwrap(),
            Reply::Checkpoint {
                epoch: 0,
                result: Ok(())
            }
        ));
        worker.set_epoch(2);
        assert!(worker
            .send(Command::Boundary {
                id: 11,
                epoch: 2,
                adopt: None
            })
            .is_ok());
        assert!(matches!(
            worker.wait().unwrap(),
            Reply::Boundary {
                id: 11,
                epoch: 2,
                result: Ok(())
            }
        ));
        assert!(store.scan().unwrap().is_empty());
        assert!(worker.send(Command::Shutdown).is_ok());
        assert!(matches!(worker.wait().unwrap(), Reply::Shutdown));
        worker.disconnect_wait();
        assert!(worker.finished());
    }
    #[test]
    fn disconnect_does_not_join_or_delete_a_completed_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("recovery")).unwrap();
        let mut worker = Worker::start(store.clone(), None).unwrap();
        let (started, begin) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        assert!(worker
            .send(Command::Checkpoint {
                epoch: 0,
                capture: Box::new(move || {
                    started.send(()).unwrap();
                    gate.recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    Ok(snapshot())
                })
            })
            .is_ok());
        begin
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let thread = worker.thread.take().unwrap();
        drop(worker); // A blocking Drop would deadlock before releasing this gate.
        release.send(()).unwrap();
        thread.join().unwrap();
        assert_eq!(store.scan().unwrap().len(), 1);
    }
}
