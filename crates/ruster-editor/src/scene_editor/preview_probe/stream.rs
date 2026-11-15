//! Fixture-only single-flight Unix stream. All blocking IO stays off the editor thread.
use serde_json::{json, Value};
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};

const REQUEST_LIMIT: usize = 4096;
const REPLY_LIMIT: usize = 8 * 1024 * 1024;

struct Deadline<'a> {
    socket: &'a mut UnixStream,
    until: Instant,
}
impl Deadline<'_> {
    fn remaining(&self) -> io::Result<Duration> {
        self.until
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "absolute frame deadline"))
    }
}
impl Read for Deadline<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(self.remaining()?))?;
        self.socket.read(bytes)
    }
}
impl Write for Deadline<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.socket.set_write_timeout(Some(self.remaining()?))?;
        self.socket.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn read_frame(reader: &mut impl Read, limit: usize) -> io::Result<Value> {
    let mut header = [0; 4];
    reader.read_exact(&mut header)?;
    let size = u32::from_be_bytes(header) as usize;
    if size == 0 || size > limit {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame length"));
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}
fn write_frame(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > REPLY_LIMIT {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "reply length"));
    }
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(&bytes)
}

pub(super) struct Bridge {
    pub requests: mpsc::Receiver<Value>,
    pub replies: mpsc::SyncSender<Value>,
    pub live: Arc<AtomicBool>,
    socket: UnixStream,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Bridge {
    pub fn connect(incarnation: &str) -> Self {
        let mut socket =
            UnixStream::connect(std::env::var_os("LOCAL_BRIDGE_SOCKET").expect("explicit socket"))
                .unwrap();
        // Idle may wait indefinitely; partial frames and writes have a finite deadline.
        socket
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        write_frame(&mut socket, &json!({"token":std::env::var("LOCAL_BRIDGE_TOKEN").unwrap(),"pid":std::process::id(),"incarnation":incarnation})).unwrap();
        let control = socket.try_clone().unwrap();
        let (tx, requests) = mpsc::sync_channel(1);
        let (replies, rx) = mpsc::sync_channel(1);
        let live = Arc::new(AtomicBool::new(true));
        let worker_live = live.clone();
        let editor = std::thread::current();
        let worker = std::thread::spawn(move || {
            let result = (|| -> io::Result<()> {
                loop {
                    // Wait for first byte without timing out a healthy idle link.
                    socket.set_read_timeout(None)?;
                    let mut first = [0];
                    socket.read_exact(&mut first)?;
                    let until = Instant::now() + Duration::from_secs(8);
                    let request = read_frame(
                        &mut first.as_slice().chain(Deadline {
                            socket: &mut socket,
                            until,
                        }),
                        REQUEST_LIMIT,
                    )?;
                    if !request.is_object()
                        || request.as_object().is_none_or(|v| {
                            !(v.len() == 2
                                || (v.len() == 4
                                    && request["browser"].as_str().is_some_and(|s| {
                                        s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
                                    })
                                    && request["origin"].as_str().is_some_and(|s| {
                                        s.len() <= 256 && s.starts_with("http://127.0.0.1:")
                                    })))
                        })
                        || !request["op"].is_string()
                        || !request["args"].is_object()
                    {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "request schema"));
                    }
                    tx.try_send(request).map_err(io::Error::other)?;
                    editor.unpark();
                    let reply = rx
                        .recv_timeout(Duration::from_secs(8))
                        .map_err(io::Error::other)?;
                    write_frame(
                        &mut Deadline {
                            socket: &mut socket,
                            until: Instant::now() + Duration::from_secs(8),
                        },
                        &reply,
                    )?;
                }
            })();
            eprintln!("local bridge closed: {result:?}");
            worker_live.store(false, Ordering::Release);
            let _ = socket.shutdown(Shutdown::Both);
            editor.unpark();
        });
        Self {
            requests,
            replies,
            live,
            socket: control,
            worker: Some(worker),
        }
    }
    pub fn live(&self) -> bool {
        self.live.load(Ordering::Acquire)
    }
    pub fn reply(&self, value: Value) {
        if self.replies.try_send(value).is_err() {
            self.live.store(false, Ordering::Release);
            let _ = self.socket.shutdown(Shutdown::Both);
        }
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.live.store(false, Ordering::Release);
        let _ = self.socket.shutdown(Shutdown::Both);
        // Wake a worker waiting for an editor reply before joining.
        let _ = self.replies.try_send(json!({"closed":true}));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[test]
fn framing_handles_fragmentation_limits_and_eof() {
    struct Fragment(std::io::Cursor<Vec<u8>>);
    impl Read for Fragment {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            let n = b.len().min(1);
            self.0.read(&mut b[..n])
        }
    }
    impl Write for Fragment {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            let n = b.len().min(1);
            self.0.write(&b[..n])
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let v = json!({"op":"status","args":{}});
    let mut f = Fragment(std::io::Cursor::new(Vec::new()));
    write_frame(&mut f, &v).unwrap();
    let bytes = f.0.into_inner();
    assert_eq!(
        read_frame(
            &mut Fragment(std::io::Cursor::new(bytes.clone())),
            REQUEST_LIMIT
        )
        .unwrap(),
        v
    );
    for end in [0, 1, 3, 4, bytes.len() - 1] {
        assert!(read_frame(&mut &bytes[..end], REQUEST_LIMIT).is_err());
    }
    for size in [0_u32, 4097, u32::MAX] {
        assert!(read_frame(&mut size.to_be_bytes().as_slice(), REQUEST_LIMIT).is_err());
    }
    let (mut a, mut b) = UnixStream::pair().unwrap();
    a.write_all(&[0, 0]).unwrap();
    assert!(read_frame(
        &mut Deadline {
            socket: &mut b,
            until: Instant::now() + Duration::from_millis(20)
        },
        REQUEST_LIMIT,
    )
    .is_err());
    // A non-reading bridge cannot stall editor execution: the worker's write
    // has the same absolute deadline, including all partial writes.
    assert!(write_frame(
        &mut Deadline {
            socket: &mut a,
            until: Instant::now() + Duration::from_millis(20)
        },
        &json!({"payload":"x".repeat(1024*1024)}),
    )
    .is_err());
    drop(a);
    assert!(read_frame(&mut b, REQUEST_LIMIT).is_err());
}
