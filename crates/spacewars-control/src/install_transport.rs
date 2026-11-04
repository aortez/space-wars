//! Bounded newline JSON over authenticated local sockets. All calls are made
//! off the UI thread. A rejected server receives no command bytes.
use super::{MAX_MESSAGE_BYTES, Message};
use futures_lite::{AsyncReadExt, AsyncWriteExt};
use serde::Serialize;
use std::{
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileTypeExt, MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub struct Peer {
    pub uid: u32,
    pub pid: u32,
}

pub fn peer(stream: &UnixStream) -> io::Result<Peer> {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of_val(&cred) as libc::socklen_t;
    // SAFETY: the live stream and correctly sized writable credentials buffer
    // are retained throughout this getsockopt call.
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    if len as usize != std::mem::size_of_val(&cred) || cred.pid <= 0 {
        return Err(invalid());
    }
    Ok(Peer {
        uid: cred.uid,
        pid: cred.pid as u32,
    })
}

pub fn exchange<Q: Serialize, R: Message>(
    path: &Path,
    uid: u32,
    pid: Option<u32>,
    request: &Q,
    timeout: Duration,
) -> io::Result<R> {
    let mut bytes = serde_json::to_vec(request).map_err(|_| invalid())?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(invalid());
    }
    bytes.push(b'\n');
    futures_lite::future::block_on(futures_lite::future::or(
        async {
            let mut stream = async_io::Async::<UnixStream>::connect(path).await?;
            let observed = peer(stream.get_ref())?;
            if observed.uid != uid || pid.is_some_and(|pid| pid != observed.pid) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "unexpected install peer",
                ));
            }
            stream.write_all(&bytes).await?;
            let mut response = Vec::new();
            let mut byte = [0];
            loop {
                if stream.read(&mut byte).await? != 1 {
                    return Err(invalid());
                }
                if byte[0] == b'\n' {
                    break;
                }
                if response.len() == MAX_MESSAGE_BYTES {
                    return Err(invalid());
                }
                response.push(byte[0]);
            }
            R::from_json(&response).map_err(|_| invalid())
        },
        async {
            async_io::Timer::after(timeout).await;
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "install request timed out",
            ))
        },
    ))
}

pub type Handler = Arc<dyn Fn(&[u8], Peer) -> Vec<u8> + Send + Sync>;

pub struct Server {
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
    path: PathBuf,
    inode: u64,
    device: u64,
}
impl Server {
    /// Caller owns the containing runtime directory and service lifetime.
    /// Existing paths are never removed here, including sockets and symlinks.
    /// The handler must bound its own work; the transport bounds framing and
    /// concurrent clients, but cannot forcibly stop an application callback.
    pub fn bind(
        path: &Path,
        authorize: Arc<dyn Fn(Peer) -> bool + Send + Sync>,
        handler: Handler,
    ) -> io::Result<Self> {
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))?;
        let metadata = std::fs::symlink_metadata(path)?;
        let inode = metadata.ino();
        let device = metadata.dev();
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let accept = thread::spawn(move || {
            let mut workers: Vec<JoinHandle<()>> = Vec::new();
            while !stopped.load(Ordering::Relaxed) {
                let mut index = 0;
                while index < workers.len() {
                    if workers[index].is_finished() {
                        let _ = workers.swap_remove(index).join();
                    } else {
                        index += 1;
                    }
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let Ok(peer) = peer(&stream) else { continue };
                        if workers.len() >= 4 || !authorize(peer) {
                            continue;
                        }
                        let handler = handler.clone();
                        let stopped = stopped.clone();
                        workers.push(thread::spawn(move || {
                            let result = (|| -> io::Result<()> {
                                let deadline = Instant::now() + Duration::from_secs(2);
                                let mut bytes = Vec::new();
                                let mut byte = [0];
                                loop {
                                    if stopped.load(Ordering::Relaxed) {
                                        return Err(invalid());
                                    }
                                    let remaining = deadline
                                        .checked_duration_since(Instant::now())
                                        .ok_or_else(invalid)?;
                                    stream.set_read_timeout(Some(remaining))?;
                                    stream.read_exact(&mut byte)?;
                                    if byte[0] == b'\n' {
                                        break;
                                    }
                                    if bytes.len() == MAX_MESSAGE_BYTES {
                                        return Err(invalid());
                                    }
                                    bytes.push(byte[0]);
                                }
                                let mut response = handler(&bytes, peer);
                                if response.len() > MAX_MESSAGE_BYTES {
                                    return Err(invalid());
                                }
                                response.push(b'\n');
                                stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                                stream.write_all(&response)
                            })();
                            let _ = result;
                        }));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
            for worker in workers {
                let _ = worker.join();
            }
        });
        Ok(Self {
            stop,
            accept: Some(accept),
            path: path.into(),
            inode,
            device,
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.accept.take() {
            let _ = thread.join();
        }
        if std::fs::symlink_metadata(&self.path).is_ok_and(|m| {
            m.file_type().is_socket() && m.ino() == self.inode && m.dev() == self.device
        }) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub fn boot_ms() -> io::Result<u64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime initializes the live timespec buffer.
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut time) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if time.tv_sec < 0 || !(0..1_000_000_000).contains(&time.tv_nsec) {
        return Err(invalid());
    }
    (time.tv_sec as u64)
        .checked_mul(1000)
        .and_then(|n| n.checked_add(time.tv_nsec as u64 / 1_000_000))
        .ok_or_else(invalid)
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid install frame")
}

#[cfg(test)]
#[path = "install_transport_tests.rs"]
mod tests;
