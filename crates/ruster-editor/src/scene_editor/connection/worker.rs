//! Linux-only bounded CLI owner. No UI-thread waits, retries or raw diagnostics.
use super::consent::RegistrationConsent;
use serde_json::Value;
use std::{
    io::{Read, Write},
    os::{
        fd::OwnedFd,
        unix::{net::UnixStream, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

const LIMIT: usize = 4096;
const DEADLINE: Duration = Duration::from_secs(45);

pub(super) struct Worker {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<RegistrationConsent>,
}

impl Worker {
    /// The explicit installed CLI and matched adapter provenance are caller obligations.
    /// Confirmation is consumed before spawning; thread/spawn failure never authorizes retry.
    pub(super) fn start(cli: PathBuf, mut consent: RegistrationConsent) -> Result<Self, String> {
        if !cli.is_absolute() || !cli.is_file() {
            return Err("Select an absolute supported Hermes executable.".into());
        }
        let invocation = consent.confirm()?;
        let cancel = Arc::new(AtomicBool::new(false));
        let cancellation = cancel.clone();
        let (send, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("hermes-registration".into())
            .spawn(move || {
                let outcome = (|| {
                    let output = run(
                        &cli,
                        &invocation.args,
                        &invocation.stdin,
                        &cancellation,
                        DEADLINE,
                    )?;
                    consent.complete(output.0, &output.1).map_err(|_| ())?;
                    // Never infer saved identity from discovery/receipt alone. Conflict/failure
                    // are terminal and deliberately do not trigger refresh/readback/retry.
                    if consent.succeeded() {
                        let args = vec![
                            "-p".into(),
                            invocation.args[1].clone(),
                            "config".into(),
                            "get".into(),
                            "mcp_servers".into(),
                            "--json".into(),
                        ];
                        let (exit, stdout) = run(&cli, &args, "", &cancellation, DEADLINE)?;
                        // The supported CLI removes the entire empty mcp_servers key.
                        // Only its exact missing-key diagnostic/exit is absence; other
                        // errors, malformed JSON and oversized documents are indeterminate.
                        let servers: Value =
                            if exit == 1 && stdout == "Config key not set: mcp_servers\n" {
                                serde_json::json!({})
                            } else if exit == 0 {
                                serde_json::from_str(&stdout).map_err(|_| ())?
                            } else {
                                return Err(());
                            };
                        let request: Value =
                            serde_json::from_str(&invocation.stdin).map_err(|_| ())?;
                        if !servers.is_object()
                            || servers.get(&invocation.args[4]).unwrap_or(&Value::Null)
                                != &request["entry"]
                        {
                            return Err(());
                        }
                    }
                    Ok::<_, ()>(())
                })();
                if outcome.is_err() {
                    consent.fail();
                }
                let _ = send.send(consent);
            })
            .map_err(|_| "Cannot start registration worker; create a new preview.".to_owned())?;
        Ok(Self { cancel, result })
    }

    pub(super) fn poll(&self) -> Result<Option<RegistrationConsent>, String> {
        match self.result.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Registration worker ended; outcome indeterminate.".into())
            }
        }
    }

    pub(super) fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
    } // Never join on the editor thread.
}

fn run(
    cli: &Path,
    args: &[String],
    input: &str,
    cancel: &AtomicBool,
    deadline: Duration,
) -> Result<(i32, String), ()> {
    if cancel.load(Ordering::Acquire) || input.len() > LIMIT {
        return Err(());
    }
    let (mut output, writer) = UnixStream::pair().map_err(|_| ())?;
    output.set_nonblocking(true).map_err(|_| ())?;
    let mut command = Command::new(cli);
    let errors = writer.try_clone().map_err(|_| ())?;
    command
        .args(args)
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::from(OwnedFd::from(errors)));
    // Never convey editor authority or Python injection settings to the client.
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if name.starts_with("RUSTER_")
            || name == "PYTHONPATH"
            || name == "PYTHONHOME"
            || name == "HERMES_HOME"
            || name == "HERMES_PROFILE"
        {
            command.env_remove(key);
        }
    }
    let mut child = command.spawn().map_err(|_| ())?;
    let result = (|| {
        child
            .stdin
            .take()
            .ok_or(())?
            .write_all(input.as_bytes())
            .map_err(|_| ())?;
        let start = Instant::now();
        let mut bytes = Vec::new();
        loop {
            if cancel.load(Ordering::Acquire) || start.elapsed() >= deadline {
                return Err(());
            }
            let mut buffer = [0; 1024];
            match output.read(&mut buffer) {
                Ok(0) => {
                    // EOF alone is not exit: retain PID ownership until group cleanup.
                }
                Ok(n) => {
                    bytes.extend_from_slice(&buffer[..n]);
                    if bytes.len() > LIMIT {
                        return Err(());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => return Err(()),
            }
            let stat =
                std::fs::read_to_string(format!("/proc/{}/stat", child.id())).map_err(|_| ())?;
            if stat.rsplit_once(')').and_then(|(_, s)| s.as_bytes().get(1)) == Some(&b'Z') {
                // Drain the bounded remaining socket bytes after leader exit.
                loop {
                    match output.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            bytes.extend_from_slice(&buffer[..n]);
                            if bytes.len() > LIMIT {
                                return Err(());
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(_) => return Err(()),
                    }
                }
                return String::from_utf8(bytes).map_err(|_| ());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    // Kill descendants before reaping the leader: no recycled process-group ID.
    let _ = rustix::process::kill_process_group(
        rustix::process::Pid::from_raw(child.id() as i32).ok_or(())?,
        rustix::process::Signal::Kill,
    );
    let status = child.wait().map_err(|_| ())?;
    result.map(|stdout| (status.code().unwrap_or(-1), stdout))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conflict_never_reads_back_and_mismatched_readback_fails_closed() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let cli = directory.path().join("cli");
        let calls = directory.path().join("calls");
        for conflict in [true, false] {
            let body = if conflict {
                "printf '{\"ok\":false,\"error\":\"config_target_conflict\"}'; exit 3"
            } else {
                "if [ \"$3\" = mcp ]; then printf '{\"ok\":true,\"changed\":true,\"tools\":84}'; else printf '{}'; fi"
            };
            std::fs::write(
                &cli,
                format!(
                    "#!/bin/sh\ncat >/dev/null\necho call >> '{}'\n{body}\n",
                    calls.display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
            let consent = RegistrationConsent::preview(
                "named",
                "rusterize",
                Path::new("/adapter"),
                Path::new("/locator"),
                None,
                false,
            )
            .unwrap();
            let worker = Worker::start(cli.clone(), consent).unwrap();
            let start = Instant::now();
            let result = loop {
                if let Some(result) = worker.poll().unwrap() {
                    break result;
                }
                assert!(start.elapsed() < Duration::from_secs(5));
                std::thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(
                result.status(),
                if conflict {
                    "Registration conflict; no refresh or retry"
                } else {
                    "Registration failed or indeterminate; inspect before a new preview"
                }
            );
            assert_eq!(
                std::fs::read_to_string(&calls).unwrap().lines().count(),
                if conflict { 1 } else { 2 }
            );
            std::fs::remove_file(&calls).unwrap();
        }
    }

    #[test]
    fn bounded_process_failures_and_nonblocking_drop() {
        let directory = tempfile::tempdir().unwrap();
        let cli = directory.path().join("cli");
        use std::os::unix::fs::PermissionsExt;
        for body in ["printf 'bad'; exit 0", "yes secret", "sleep 10 & wait"] {
            std::fs::write(&cli, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
            let result = run(
                &cli,
                &[],
                "",
                &AtomicBool::new(false),
                Duration::from_millis(100),
            );
            if body.starts_with("printf") {
                assert_eq!(result.unwrap().1, "bad");
            } else {
                assert!(result.is_err());
            }
        }
        let consent = RegistrationConsent::preview(
            "named",
            "rusterize",
            Path::new("/adapter"),
            Path::new("/locator"),
            None,
            false,
        )
        .unwrap();
        let worker = Worker::start(cli, consent).unwrap();
        let start = Instant::now();
        assert!(worker.poll().unwrap().is_none());
        worker.cancel();
        drop(worker);
        assert!(start.elapsed() < Duration::from_millis(100));
    }

    #[test]
    #[ignore = "explicit disposable supported-Hermes profile qualification"]
    fn installed_cli_disposable_profile() {
        let profile = std::env::var("RUSTER_TEST_HERMES_PROFILE").unwrap();
        assert_eq!(profile, "e2a-cp2-worker-20261114");
        let adapter =
            Path::new("/home/akilf/Projects/Ruster-Audits/e2a-cp1-20261114/final-bin/ruster-mcp");
        let locator =
            Path::new("/home/akilf/Projects/Ruster-Audits/e2a-cp2-20261114/worker/unbound.json");
        let entry = serde_json::json!({"command":adapter,"enabled":true,"env":{"RUSTER_MCP_LOCATOR":locator}});
        for (expected, remove, status) in [
            (
                None,
                false,
                "Registered; client restart required; editor binding separate",
            ),
            (
                Some(entry.clone()),
                false,
                "Registration reused; client restart required; editor binding separate",
            ),
            (None, false, "Registration conflict; no refresh or retry"),
            (
                Some(entry.clone()),
                true,
                "Registration removed; client restart required; editor binding separate",
            ),
        ] {
            let consent = RegistrationConsent::preview(
                &profile,
                "rusterize",
                adapter,
                locator,
                expected,
                remove,
            )
            .unwrap();
            let worker =
                Worker::start(PathBuf::from("/home/akilf/.local/bin/hermes"), consent).unwrap();
            let start = Instant::now();
            let result = loop {
                if let Some(result) = worker.poll().unwrap() {
                    break result;
                }
                assert!(start.elapsed() < Duration::from_secs(95));
                std::thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(result.status(), status);
            println!("{}", result.status());
        }
        assert!(!locator.exists());
    }
}
