//! The file-descriptor seam: real files under `Config::storage`, the asset
//! archive served from memory by name, stdio `FILE*` as opaque handles, and a
//! socket layer that emulates exactly one network condition — the servers are
//! gone.
//!
//! **Paths.** The guest was built for `/data/data/co.kr.eamobile.HeroesLore5/`;
//! every guest path is re-rooted under `Config::storage` component by
//! component (`..` cannot climb above the root), so the guest can name any
//! absolute path it likes and never leaves its sandbox. A path whose final
//! component is `data.vfs.mp3` is served read-only from `Config::vfs` instead
//! of the disk — the arm64 shell ships the archive as an APK asset, not as a
//! file, and this answers the bring-up question of whether the host must parse
//! the archive: it must not, it only has to hand back bytes; the guest's own
//! `MIDASKernelManager::loadAssetFromVFS` does the interpreting.
//!
//! **Sockets.** The reference run (galaxy S9, original APK, 2026-08-18) shows
//! what the guest does about its dead servers: "Connecting", its own timeout,
//! then the title screen. This layer reproduces that world deterministically:
//! `connect` fails (`EINPROGRESS` first when non-blocking), `select` returns
//! empty — advancing the virtual clock by the guest's own timeout so the wait
//! costs no wall time — and `SO_ERROR` reads back `ETIMEDOUT`.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};

use crate::Vm;

// errno values as Linux ARM EABI defines them — bionic passes them through.
pub const ENOENT: u32 = 2;
pub const EIO: u32 = 5;
pub const EBADF: u32 = 9;
pub const EACCES: u32 = 13;
pub const EEXIST: u32 = 17;
pub const EINVAL: u32 = 22;
pub const EMFILE: u32 = 24;
pub const ENOTCONN: u32 = 107;
pub const ETIMEDOUT: u32 = 110;
pub const ECONNREFUSED: u32 = 111;
pub const EINPROGRESS: u32 = 115;
pub const EWOULDBLOCK: u32 = 11; // == EAGAIN on Linux ARM

// open(2) flag bits, generic Linux layout (ARM uses the generic one).
const O_WRONLY: u32 = 0o1;
const O_RDWR: u32 = 0o2;
const O_CREAT: u32 = 0o100;
const O_TRUNC: u32 = 0o1000;
const O_APPEND: u32 = 0o2000;
const O_NONBLOCK: u32 = 0o4000;

/// `recv` flag: look at the bytes without taking them out of the queue.
const MSG_PEEK: u32 = 2;

// fcntl(2) commands.
const F_DUPFD: u32 = 0;
const F_GETFD: u32 = 1;
const F_SETFD: u32 = 2;
const F_GETFL: u32 = 3;
const F_SETFL: u32 = 4;

/// `FILE*` values handed to the guest. They point into the fake-handle range
/// of the JNI window — a plausible, mapped, unused address — but nothing is
/// stored there: every stdio import resolves the handle host-side, and a guest
/// that dereferenced one would read zeros, not garbage.
const STREAM_HANDLE_BASE: u32 = crate::mem::JNI_BASE + 0x0006_0000;

enum FdObj {
    Host(std::fs::File),
    /// Read-only cursor over `Config::vfs`.
    Vfs { pos: u64 },
    /// A guest socket. When `Config::server` is set, `stream` holds a live
    /// TCP connection to the hero3/hero4 backend; otherwise it stays `None`
    /// and every operation reproduces the "servers are gone" world.
    ///
    /// `dial` is a non-blocking connect still in flight on a worker thread.
    /// It exists because a blocking dial here runs **inside `vm.frame()`**:
    /// with `Config::server` set but unreachable (a phone with no data), a
    /// 5-second `connect_timeout` froze frames and input for its whole
    /// duration, twice per boot (ANALYSIS.md §16.6). [`Vm::poll_dials`]
    /// promotes a finished dial into `stream`; a failed one collapses back to
    /// the dead-server world, whose select-timeout/`SO_ERROR` path is the one
    /// the guest is proven to handle.
    ///
    /// The third world — an in-process server with no network at all — is
    /// not a variant of this one: it is [`FdObj::Local`] below.
    Socket {
        nonblocking: bool,
        stream: Option<std::net::TcpStream>,
        dial: Option<std::sync::mpsc::Receiver<std::io::Result<std::net::TcpStream>>>,
    },
    /// A guest socket connected to the **in-process** private server
    /// (`Config::local_server`, `localserver.rs`): `send` feeds the session,
    /// `recv` drains what it answered, no network anywhere. Always
    /// "connected" — `SO_ERROR` reads 0 — and readable exactly when the
    /// session has queued reply bytes (or has dropped the connection, since
    /// EOF is readable). Reads never block regardless of the O_NONBLOCK
    /// flag; the guest's game socket is always non-blocking, so the flag has
    /// nothing left to select.
    Local(crate::localserver::Session),
}

pub struct Files {
    /// Directory for relative guest paths, as a guest-side absolute path.
    cwd: String,
    /// 0/1/2 are stdin/stdout/stderr and never enter the table, so the lowest
    /// descriptor this hands out is 3.
    fds: HashMap<u32, FdObj>,
    /// FILE* handle -> fd.
    streams: HashMap<u32, u32>,
    next_stream: u32,
}

impl Files {
    pub fn new() -> Self {
        Self {
            cwd: "/".into(),
            fds: HashMap::new(),
            streams: HashMap::new(),
            next_stream: 0,
        }
    }

    /// Take the **lowest free** descriptor, as POSIX requires.
    ///
    /// A monotonic counter looks equivalent and is not. Guest code checks
    /// descriptors against `FD_SETSIZE`: `MC_netSocket` (0x14e324) rejects
    /// anything `>= 0x400` outright, because a larger one could not be put in
    /// an `fd_set`. The guest opens and closes the VFS twice per resource
    /// lookup, so a counter sails past 1024 during ordinary play and then
    /// every later `socket()` fails — the symptom being a cash shop that
    /// reports "server is not available" after an hour of working networking.
    fn insert(&mut self, obj: FdObj) -> u32 {
        let fd = (3..).find(|n| !self.fds.contains_key(n)).expect("32-bit fd space");
        self.fds.insert(fd, obj);
        fd
    }

    fn new_stream(&mut self, fd: u32) -> u32 {
        let h = STREAM_HANDLE_BASE + self.next_stream * 4;
        self.next_stream += 1;
        self.streams.insert(h, fd);
        h
    }
}

impl Default for Files {
    fn default() -> Self {
        Self::new()
    }
}

/// Dial a real TCP server with a short connect timeout. On success the stream
/// is put into the requested blocking mode.
fn connect_real(host: &str, port: u16, nonblocking: bool) -> std::io::Result<std::net::TcpStream> {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let addr = (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no address"))?;
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
    stream.set_nonblocking(nonblocking)?;
    Ok(stream)
}

fn errno_of(e: &std::io::Error) -> u32 {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => ENOENT,
        PermissionDenied => EACCES,
        AlreadyExists => EEXIST,
        InvalidInput => EINVAL,
        // A non-blocking send with a full buffer must say EAGAIN:
        // `MC_netSocketWrite` retries (-19) only on errno 11 and treats
        // anything else — including the EIO this used to fall into — as a
        // hard -1, killing a healthy connection on the first large write.
        WouldBlock => EWOULDBLOCK,
        TimedOut => ETIMEDOUT,
        ConnectionRefused => ECONNREFUSED,
        _ => EIO,
    }
}

impl Vm {
    /// Re-root a guest path under `Config::storage`. Purely textual — no
    /// symlinks are followed, `..` merely pops within the sandbox. Guest save
    /// names have been ASCII on every midas title observed; if one ever is
    /// not, the lossy decode shows up in the filename, not as corruption.
    fn host_path(&self, raw: &str) -> std::path::PathBuf {
        let full = if raw.starts_with('/') {
            raw.to_string()
        } else {
            format!("{}/{}", self.files.cwd, raw)
        };
        let mut p = self.cfg.storage.clone();
        let mut depth = 0usize;
        for c in full.split('/') {
            match c {
                "" | "." => {}
                ".." => {
                    if depth > 0 {
                        p.pop();
                        depth -= 1;
                    }
                }
                other => {
                    p.push(other);
                    depth += 1;
                }
            }
        }
        p
    }

    /// Open a fresh read cursor over the in-memory VFS archive and return its
    /// fd. `nativeInitVFS` is handed this fd inside a `FileDescriptor` object;
    /// the guest then `dup`s it and `fdopen`s the copy.
    pub fn open_vfs_fd(&mut self) -> u32 {
        self.files.insert(FdObj::Vfs { pos: 0 })
    }

    // ---- select support ---------------------------------------------------
    //
    // An `fd_set` is an array of 32-bit words, bit `fd & 31` of word `fd >> 5`
    // — which is how the guest builds it (`__knl_net_connect_check_timer`
    // 0x14d220: `1 << (fd & 0x1f)` into `[set + (fd >> 5) * 4]`).

    fn fdset_read(&self, base: u32, nfds: u32) -> Vec<u32> {
        if base == 0 {
            return Vec::new();
        }
        (0..nfds)
            .filter(|fd| self.mem.read_u32(base + (fd >> 5) * 4) & (1 << (fd & 31)) != 0)
            .collect()
    }

    fn fdset_clear(&mut self, base: u32, nfds: u32) {
        if base == 0 {
            return;
        }
        for w in 0..nfds.div_ceil(32) {
            self.mem.write_u32(base + w * 4, 0);
        }
    }

    fn fdset_write(&mut self, base: u32, nfds: u32, fds: &[u32]) {
        self.fdset_clear(base, nfds);
        if base == 0 {
            return;
        }
        for &fd in fds {
            let at = base + (fd >> 5) * 4;
            let v = self.mem.read_u32(at) | 1 << (fd & 31);
            self.mem.write_u32(at, v);
        }
    }

    /// Is there something to read on this fd right now?
    ///
    /// std has no `select`, but it has `TcpStream::peek`, which answers
    /// exactly this question on a non-blocking socket: bytes available, EOF
    /// (`Ok(0)`, and a closed connection *is* readable), or `WouldBlock`.
    fn readable(&mut self, fd: u32) -> bool {
        match self.files.fds.get(&fd) {
            Some(FdObj::Socket { stream: Some(s), nonblocking, .. }) => {
                let restore = *nonblocking;
                s.set_nonblocking(true).ok();
                let mut b = [0u8; 1];
                let ready = !matches!(s.peek(&mut b), Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock);
                s.set_nonblocking(restore).ok();
                ready
            }
            Some(FdObj::Socket { stream: None, .. }) | None => false,
            // A local session is readable exactly when a reply is waiting: the
            // connect-check timer treats readable as "peek one byte and judge
            // the link", so an always-readable local socket would be judged
            // dead the moment its outbox was empty.
            Some(FdObj::Local(s)) => s.readable(),
            // A regular file or the in-memory archive is always ready.
            Some(_) => true,
        }
    }

    fn writable(&self, fd: u32) -> bool {
        match self.files.fds.get(&fd) {
            Some(FdObj::Socket { stream, .. }) => stream.is_some(),
            // A local session accepts bytes at any time.
            Some(FdObj::Local(_)) => true,
            Some(_) => true,
            None => false,
        }
    }

    /// Like [`Files::fd_read`] but leaves the bytes in the socket buffer.
    fn fd_peek(&mut self, fd: u32, n: usize) -> Result<Vec<u8>, u32> {
        self.poll_dials();
        match self.files.fds.get(&fd) {
            Some(FdObj::Socket { stream: Some(s), nonblocking, .. }) => {
                s.set_nonblocking(*nonblocking).ok();
                let mut b = vec![0u8; n];
                match s.peek(&mut b) {
                    Ok(m) => {
                        b.truncate(m);
                        Ok(b)
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Err(EWOULDBLOCK),
                    Err(e) => Err(errno_of(&e)),
                }
            }
            Some(FdObj::Socket { stream: None, .. }) => Err(ENOTCONN),
            Some(FdObj::Local(s)) => {
                // An empty queue on a live session is EWOULDBLOCK, exactly as
                // a non-blocking peek; once the session dropped the
                // connection an empty peek is the EOF the connect check
                // treats as a dead link — which it then is.
                let b = s.peek(n);
                if b.is_empty() && !s.is_closed() {
                    Err(EWOULDBLOCK)
                } else {
                    Ok(b)
                }
            }
            None => Err(EBADF),
            // Peeking a file is not something this guest does; reading it and
            // seeking back would be a guess about the caller's intent.
            Some(_) => Err(EINVAL),
        }
    }

    /// Whether any socket currently holds a live TCP connection — or is still
    /// dialling one. A dial in flight must keep select on its real polling
    /// path: the fast-forwarding dead-server answer would bill the guest's
    /// whole timeout to the clock while the connection was about to succeed.
    fn any_live_socket(&self) -> bool {
        self.files
            .fds
            .values()
            .any(|o| matches!(o, FdObj::Socket { stream: Some(_), .. })
                || matches!(o, FdObj::Socket { dial: Some(_), .. }))
    }

    /// Promote every background dial that has finished. A success becomes the
    /// socket's live stream; a failure is logged and the socket collapses to
    /// the dead-server world (never ready, `SO_ERROR` = `ETIMEDOUT`), which is
    /// the exact shape the guest's own timeout-and-popup path is proven to
    /// handle on the S9 reference. Cheap when nothing is pending.
    fn poll_dials(&mut self) {
        use std::sync::mpsc::TryRecvError;
        let mut failed: Vec<String> = Vec::new();
        for obj in self.files.fds.values_mut() {
            let FdObj::Socket { stream, dial, .. } = obj else { continue };
            let Some(rx) = dial.as_ref() else { continue };
            match rx.try_recv() {
                Ok(Ok(s)) => {
                    *stream = Some(s);
                    *dial = None;
                }
                Ok(Err(e)) => {
                    failed.push(format!("connect failed: {e}"));
                    *dial = None;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => *dial = None,
            }
        }
        self.log.extend(failed);
    }

    fn is_vfs_path(&self, raw: &str) -> bool {
        !self.cfg.vfs.is_empty()
            && raw.rsplit('/').next().is_some_and(|b| b == "data.vfs.mp3")
    }

    // ---- fd-level primitives shared by the syscall and stdio arms ---------

    fn fd_read(&mut self, fd: u32, n: usize) -> Result<Vec<u8>, u32> {
        self.poll_dials();
        match self.files.fds.get_mut(&fd) {
            Some(FdObj::Host(f)) => {
                let mut b = vec![0u8; n];
                let m = f.read(&mut b).map_err(|e| errno_of(&e))?;
                b.truncate(m);
                Ok(b)
            }
            Some(FdObj::Vfs { pos }) => {
                let data = &self.cfg.vfs;
                let start = (*pos as usize).min(data.len());
                let end = start.saturating_add(n).min(data.len());
                *pos = end as u64;
                Ok(data[start..end].to_vec())
            }
            Some(FdObj::Socket { stream: Some(s), nonblocking, .. }) => {
                s.set_nonblocking(*nonblocking).ok();
                let mut b = vec![0u8; n];
                match s.read(&mut b) {
                    Ok(m) => {
                        b.truncate(m);
                        Ok(b)
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Err(EWOULDBLOCK),
                    Err(e) => Err(errno_of(&e)),
                }
            }
            Some(FdObj::Socket { stream: None, .. }) => Err(ENOTCONN),
            Some(FdObj::Local(s)) => {
                // Same shape as the peek above: data, or EWOULDBLOCK while the
                // session lives, or the empty read that is EOF once it closed.
                let b = s.read(n);
                if b.is_empty() && !s.is_closed() {
                    Err(EWOULDBLOCK)
                } else {
                    Ok(b)
                }
            }
            None => Err(EBADF),
        }
    }

    fn fd_write(&mut self, fd: u32, data: &[u8]) -> Result<usize, u32> {
        self.poll_dials();
        match fd {
            1 | 2 => {
                self.log.push(format!("[fd{fd}] {}", String::from_utf8_lossy(data)));
                return Ok(data.len());
            }
            _ => {}
        }
        // The local session first: its replies are generated inside the
        // write, and the log lines it produces go to the host log — which the
        // match below could not reach while the session is borrowed.
        if let Some(FdObj::Local(sess)) = self.files.fds.get_mut(&fd) {
            let lines = sess.write(data);
            self.log.extend(lines);
            return Ok(data.len());
        }
        match self.files.fds.get_mut(&fd) {
            Some(FdObj::Host(f)) => f.write(data).map_err(|e| errno_of(&e)),
            Some(FdObj::Vfs { .. }) => Err(EACCES),
            Some(FdObj::Socket { stream: Some(s), .. }) => {
                s.write(data).map_err(|e| errno_of(&e))
            }
            Some(FdObj::Socket { stream: None, .. }) => Err(ENOTCONN),
            Some(FdObj::Local(_)) => unreachable!("handled above"),
            None => Err(EBADF),
        }
    }

    fn fd_seek(&mut self, fd: u32, off: i64, whence: u32) -> Result<u64, u32> {
        match self.files.fds.get_mut(&fd) {
            Some(FdObj::Host(f)) => {
                let s = match whence {
                    0 => SeekFrom::Start(off.max(0) as u64),
                    1 => SeekFrom::Current(off),
                    2 => SeekFrom::End(off),
                    _ => return Err(EINVAL),
                };
                f.seek(s).map_err(|e| errno_of(&e))
            }
            Some(FdObj::Vfs { pos }) => {
                let len = self.cfg.vfs.len() as i64;
                let base = match whence {
                    0 => 0,
                    1 => *pos as i64,
                    2 => len,
                    _ => return Err(EINVAL),
                };
                let np = base + off;
                if np < 0 {
                    return Err(EINVAL);
                }
                *pos = np as u64;
                Ok(*pos)
            }
            Some(FdObj::Socket { .. }) | Some(FdObj::Local(_)) => Err(EINVAL),
            None => Err(EBADF),
        }
    }

    fn fd_close(&mut self, fd: u32) -> Result<(), u32> {
        self.files.fds.remove(&fd).map(|_| ()).ok_or(EBADF)
    }

    fn open_host(&mut self, raw: &str, flags: u32) -> Result<u32, u32> {
        if self.is_vfs_path(raw) {
            return Ok(self.files.insert(FdObj::Vfs { pos: 0 }));
        }
        let hp = self.host_path(raw);
        let mut opts = std::fs::OpenOptions::new();
        match flags & 0o3 {
            0 => opts.read(true),
            O_WRONLY => opts.write(true),
            _ => opts.read(true).write(true),
        };
        if flags & O_CREAT != 0 {
            opts.create(true);
            if let Some(parent) = hp.parent() {
                std::fs::create_dir_all(parent).ok();
            }
        }
        if flags & O_TRUNC != 0 {
            opts.truncate(true);
        }
        if flags & O_APPEND != 0 {
            opts.append(true);
        }
        match opts.open(&hp) {
            Ok(f) => Ok(self.files.insert(FdObj::Host(f))),
            Err(e) => Err(errno_of(&e)),
        }
    }

    /// Fill a `struct stat` (bionic's ARM layout mirrors the kernel's
    /// `stat64`: 104 bytes; `st_mode` at 16, `st_size` at 48, `st_blksize`
    /// at 56, `st_mtime` at 80). Only the fields a game plausibly reads are
    /// populated; everything else is zero.
    fn write_stat(&mut self, buf: u32, mode: u32, size: u64, mtime_s: u64) {
        if let Some(b) = self.mem.slice_mut(buf, 104) {
            b.fill(0);
        }
        self.mem.write_u32(buf + 16, mode);
        self.mem.write_u32(buf + 20, 1); // st_nlink
        self.mem.write_u32(buf + 48, size as u32);
        self.mem.write_u32(buf + 52, (size >> 32) as u32);
        self.mem.write_u32(buf + 56, 4096); // st_blksize
        self.mem.write_u32(buf + 80, mtime_s as u32);
    }

    // ---- dispatch: files ----------------------------------------------------

    pub(crate) fn dispatch_files(&mut self, name: &str) -> Result<bool, String> {
        match name {
            "open" => {
                let a0 = self.arg(0);
                let raw = self.mem.cstr(a0);
                let flags = self.arg(1);
                match self.open_host(&raw, flags) {
                    Ok(fd) => self.ret(fd),
                    Err(e) => {
                        self.set_errno(e);
                        self.ret(u32::MAX);
                    }
                }
            }
            "close" => {
                let fd = self.arg(0);
                match self.fd_close(fd) {
                    Ok(()) => self.ret(0),
                    Err(e) => {
                        self.set_errno(e);
                        self.ret(u32::MAX);
                    }
                }
            }
            "read" => {
                let (fd, buf, n) = (self.arg(0), self.arg(1), self.arg(2));
                match self.fd_read(fd, n as usize) {
                    Ok(data) => {
                        self.mem.write_bytes(buf, &data);
                        self.ret(data.len() as u32);
                    }
                    Err(e) => {
                        self.set_errno(e);
                        self.ret(u32::MAX);
                    }
                }
            }
            "write" => {
                let (fd, buf, n) = (self.arg(0), self.arg(1), self.arg(2));
                let Some(data) = self.mem.slice(buf, n).map(<[u8]>::to_vec) else {
                    self.set_errno(EINVAL);
                    self.ret(u32::MAX);
                    return Ok(true);
                };
                match self.fd_write(fd, &data) {
                    Ok(m) => self.ret(m as u32),
                    Err(e) => {
                        self.set_errno(e);
                        self.ret(u32::MAX);
                    }
                }
            }
            "lseek" => {
                let (fd, off, whence) = (self.arg(0), self.arg(1) as i32, self.arg(2));
                match self.fd_seek(fd, off as i64, whence) {
                    Ok(p) => self.ret(p as u32),
                    Err(e) => {
                        self.set_errno(e);
                        self.ret(u32::MAX);
                    }
                }
            }
            "stat" => {
                let a0 = self.arg(0);
                let raw = self.mem.cstr(a0);
                let buf = self.arg(1);
                if self.is_vfs_path(&raw) {
                    let n = self.cfg.vfs.len() as u64;
                    self.write_stat(buf, 0o100444, n, 0);
                    self.ret(0);
                    return Ok(true);
                }
                let hp = self.host_path(&raw);
                match std::fs::metadata(&hp) {
                    Ok(m) => {
                        let mode = if m.is_dir() { 0o040755 } else { 0o100644 };
                        let mtime = m
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map_or(0, |d| d.as_secs());
                        self.write_stat(buf, mode, m.len(), mtime);
                        self.ret(0);
                    }
                    Err(e) => {
                        self.set_errno(errno_of(&e));
                        self.ret(u32::MAX);
                    }
                }
            }
            "access" => {
                let a0 = self.arg(0);
                let raw = self.mem.cstr(a0);
                if self.is_vfs_path(&raw) || self.host_path(&raw).exists() {
                    self.ret(0);
                } else {
                    self.set_errno(ENOENT);
                    self.ret(u32::MAX);
                }
            }
            "mkdir" => {
                let a0 = self.arg(0);
                let raw = self.mem.cstr(a0);
                match std::fs::create_dir_all(self.host_path(&raw)) {
                    Ok(()) => self.ret(0),
                    Err(e) => {
                        self.set_errno(errno_of(&e));
                        self.ret(u32::MAX);
                    }
                }
            }
            "rmdir" | "unlink" => {
                let a0 = self.arg(0);
                let raw = self.mem.cstr(a0);
                let hp = self.host_path(&raw);
                let r = if name == "rmdir" { std::fs::remove_dir(&hp) } else { std::fs::remove_file(&hp) };
                match r {
                    Ok(()) => self.ret(0),
                    Err(e) => {
                        self.set_errno(errno_of(&e));
                        self.ret(u32::MAX);
                    }
                }
            }
            "rename" => {
                let a0 = self.arg(0);
                let from = self.mem.cstr(a0);
                let a1 = self.arg(1);
                let to = self.mem.cstr(a1);
                let (hf, ht) = (self.host_path(&from), self.host_path(&to));
                match std::fs::rename(&hf, &ht) {
                    Ok(()) => self.ret(0),
                    Err(e) => {
                        self.set_errno(errno_of(&e));
                        self.ret(u32::MAX);
                    }
                }
            }
            "chdir" => {
                let a0 = self.arg(0);
                let raw = self.mem.cstr(a0);
                self.files.cwd = if raw.starts_with('/') {
                    raw
                } else {
                    format!("{}/{}", self.files.cwd, raw)
                };
                self.ret(0);
            }
            "dup" => {
                let fd = self.arg(0);
                let copy = match self.files.fds.get(&fd) {
                    Some(FdObj::Host(f)) => match f.try_clone() {
                        Ok(c) => Ok(FdObj::Host(c)),
                        Err(_) => Err(EMFILE),
                    },
                    Some(FdObj::Vfs { pos }) => Ok(FdObj::Vfs { pos: *pos }),
                    // A dial in flight is not duplicated: the receiver has one
                    // owner. The guest only ever dups the VFS fd, so a socket
                    // dup is already a courtesy; a mid-dial one would be a
                    // first.
                    Some(FdObj::Socket { nonblocking, stream, .. }) => match stream {
                        Some(s) => match s.try_clone() {
                            Ok(c) => Ok(FdObj::Socket {
                                nonblocking: *nonblocking,
                                stream: Some(c),
                                dial: None,
                            }),
                            Err(_) => Err(EMFILE),
                        },
                        None => Ok(FdObj::Socket {
                            nonblocking: *nonblocking,
                            stream: None,
                            dial: None,
                        }),
                    },
                    // The session's read queue has one owner, and the guest
                    // never dups a socket — the VFS fd is the only thing it
                    // duplicates. Refuse rather than fork the conversation.
                    Some(FdObj::Local(_)) => Err(EMFILE),
                    None => Err(EBADF),
                };
                match copy {
                    Ok(obj) => {
                        let nfd = self.files.insert(obj);
                        self.ret(nfd);
                    }
                    Err(e) => {
                        self.set_errno(e);
                        self.ret(u32::MAX);
                    }
                }
            }
            "fcntl" => {
                let (fd, cmd, val) = (self.arg(0), self.arg(1), self.arg(2));
                match cmd {
                    F_GETFL => {
                        let fl = match self.files.fds.get(&fd) {
                            Some(FdObj::Socket { nonblocking: true, .. }) => O_RDWR | O_NONBLOCK,
                            Some(FdObj::Socket { nonblocking: false, .. }) => O_RDWR,
                            Some(_) => 0,
                            None => {
                                self.set_errno(EBADF);
                                self.ret(u32::MAX);
                                return Ok(true);
                            }
                        };
                        self.ret(fl);
                    }
                    F_SETFL => {
                        if let Some(FdObj::Socket { nonblocking, .. }) = self.files.fds.get_mut(&fd) {
                            *nonblocking = val & O_NONBLOCK != 0;
                        }
                        self.ret(0);
                    }
                    F_DUPFD | F_GETFD | F_SETFD => self.ret(0),
                    _ => self.ret(0),
                }
            }

            // ---- stdio over the same table --------------------------------
            "fdopen" => {
                let fd = self.arg(0);
                if self.files.fds.contains_key(&fd) {
                    let h = self.files.new_stream(fd);
                    self.ret(h);
                } else {
                    self.set_errno(EBADF);
                    self.ret(0);
                }
            }
            "fopen" => {
                let a0 = self.arg(0);
                let raw = self.mem.cstr(a0);
                let a1 = self.arg(1);
                let mode = self.mem.cstr(a1);
                let flags = match mode.trim_end_matches('b').trim_end_matches('t') {
                    "r" => 0,
                    "r+" => O_RDWR,
                    "w" => O_WRONLY | O_CREAT | O_TRUNC,
                    "w+" => O_RDWR | O_CREAT | O_TRUNC,
                    "a" => O_WRONLY | O_CREAT | O_APPEND,
                    "a+" => O_RDWR | O_CREAT | O_APPEND,
                    _ => 0,
                };
                match self.open_host(&raw, flags) {
                    Ok(fd) => {
                        let h = self.files.new_stream(fd);
                        self.ret(h);
                    }
                    Err(e) => {
                        self.set_errno(e);
                        self.ret(0);
                    }
                }
            }
            "fread" | "fwrite" => {
                let (ptr, size, nmemb, h) = (self.arg(0), self.arg(1), self.arg(2), self.arg(3));
                let Some(&fd) = self.files.streams.get(&h) else {
                    self.set_errno(EBADF);
                    self.ret(0);
                    return Ok(true);
                };
                let total = (size as usize).saturating_mul(nmemb as usize);
                if size == 0 || total == 0 {
                    self.ret(0);
                    return Ok(true);
                }
                let done = if name == "fread" {
                    match self.fd_read(fd, total) {
                        Ok(data) => {
                            self.mem.write_bytes(ptr, &data);
                            data.len()
                        }
                        Err(_) => 0,
                    }
                } else {
                    match self.mem.slice(ptr, total as u32).map(<[u8]>::to_vec) {
                        Some(data) => self.fd_write(fd, &data).unwrap_or(0),
                        None => 0,
                    }
                };
                self.ret((done / size as usize) as u32);
            }
            "fseek" => {
                let (h, off, whence) = (self.arg(0), self.arg(1) as i32, self.arg(2));
                let Some(&fd) = self.files.streams.get(&h) else {
                    self.set_errno(EBADF);
                    self.ret(u32::MAX);
                    return Ok(true);
                };
                match self.fd_seek(fd, off as i64, whence) {
                    Ok(_) => self.ret(0),
                    Err(e) => {
                        self.set_errno(e);
                        self.ret(u32::MAX);
                    }
                }
            }
            "ftell" => {
                let h = self.arg(0);
                let Some(&fd) = self.files.streams.get(&h) else {
                    self.set_errno(EBADF);
                    self.ret(u32::MAX);
                    return Ok(true);
                };
                match self.fd_seek(fd, 0, 1) {
                    Ok(p) => self.ret(p as u32),
                    Err(e) => {
                        self.set_errno(e);
                        self.ret(u32::MAX);
                    }
                }
            }
            "fclose" => {
                let h = self.arg(0);
                match self.files.streams.remove(&h) {
                    Some(fd) => {
                        self.fd_close(fd).ok();
                        self.ret(0);
                    }
                    None => {
                        self.set_errno(EBADF);
                        self.ret(u32::MAX);
                    }
                }
            }

            _ => return Ok(false),
        }
        Ok(true)
    }

    // ---- dispatch: sockets --------------------------------------------------

    pub(crate) fn dispatch_sockets(&mut self, name: &str) -> Result<bool, String> {
        // Promote any dial that finished since the guest last looked, so every
        // socket call below sees the socket's current truth.
        self.poll_dials();
        match name {
            "socket" => {
                let fd =
                    self.files.insert(FdObj::Socket { nonblocking: false, stream: None, dial: None });
                self.ret(fd);
            }
            "connect" => {
                let fd = self.arg(0);
                // `Config::server`, when set, is the real backend (the
                // hero3/hero4 server). The guest hardcodes 210.222.18.28
                // inside MX_smartConnect, but that host is gone; the host
                // redirects the dial to the configured endpoint instead.
                let target = self.cfg.server.clone();
                self.poll_dials();
                let nb = match self.files.fds.get(&fd) {
                    Some(FdObj::Socket { stream: Some(_), .. }) => {
                        // Already connected; nothing to do.
                        self.ret(0);
                        return Ok(true);
                    }
                    Some(FdObj::Socket { dial: Some(_), .. }) => {
                        // The previous dial is still in flight.
                        self.set_errno(EINPROGRESS);
                        self.ret(u32::MAX);
                        return Ok(true);
                    }
                    Some(FdObj::Local(_)) => {
                        // Already connected to the in-process server.
                        self.ret(0);
                        return Ok(true);
                    }
                    Some(FdObj::Socket { nonblocking, .. }) => *nonblocking,
                    _ => {
                        self.set_errno(EBADF);
                        self.ret(u32::MAX);
                        return Ok(true);
                    }
                };
                match target {
                    // Non-blocking connect: dial on a worker thread. This runs
                    // inside `vm.frame()`, and a blocking `connect_timeout`
                    // here froze the frame and every input for up to 5 seconds
                    // when the configured server was unreachable — twice per
                    // boot (ANALYSIS.md §16.6). The guest already speaks the
                    // EINPROGRESS-then-select protocol, so give it exactly
                    // that; `poll_dials` promotes the result.
                    Some((host, port)) if nb => {
                        let (tx, rx) = std::sync::mpsc::channel();
                        std::thread::spawn(move || {
                            let r = connect_real(&host, port, true).map_err(|e| {
                                std::io::Error::new(e.kind(), format!("{host}:{port}: {e}"))
                            });
                            // The receiver may be gone (socket closed mid-dial);
                            // that is fine, the connection just drops.
                            let _ = tx.send(r);
                        });
                        self.files.fds.insert(
                            fd,
                            FdObj::Socket { nonblocking: true, stream: None, dial: Some(rx) },
                        );
                        self.set_errno(EINPROGRESS);
                        self.ret(u32::MAX);
                    }
                    // Blocking connect keeps the synchronous dial: the caller
                    // asked to wait for the answer. The guest's game socket
                    // never takes this path (it connects non-blocking and
                    // selects), so nothing here sits inside a frame.
                    Some((host, port)) => match connect_real(&host, port, false) {
                        Ok(stream) => {
                            self.files.fds.insert(
                                fd,
                                FdObj::Socket {
                                    nonblocking: false,
                                    stream: Some(stream),
                                    dial: None,
                                },
                            );
                            self.ret(0);
                        }
                        Err(e) => {
                            self.log.push(format!("connect to {host}:{port} failed: {e}"));
                            self.set_errno(ECONNREFUSED);
                            self.ret(u32::MAX);
                        }
                    },
                    // No server configured but the in-process one is enabled:
                    // the socket becomes a session with the private server's
                    // handlers (`localserver.rs`), and the connect completes
                    // through the same EINPROGRESS-then-select protocol the
                    // guest speaks to a real endpoint — select then reports
                    // it writable and `SO_ERROR` reads 0.
                    //
                    // `free_shop` implies this world rather than being a
                    // fourth one: buying without a network *is* the private
                    // server answering category 6, and the grant list it
                    // answers 6/5 with is the only thing the flag adds to the
                    // conversation (`shop::GrantTable`).
                    None if self.cfg.local_server || self.cfg.free_shop => {
                        // A relic box may roll a weapon, and the weapon gate
                        // compares `ItemInfo+0x155` against the hero's own
                        // class (0xA8248) — so ask the guest now, the one
                        // moment this code holds the whole `Vm` and the
                        // session at once (`shop::hero_class`; the session is
                        // unreachable for it once inside `Files.fds`). Only a
                        // grant-bearing session asks: nothing else reads the
                        // answer, and a `--local-server` replay should not
                        // grow a guest re-entry it never had.
                        let hero_class = if self.shop.grants.is_empty() {
                            None
                        } else {
                            crate::shop::hero_class(self)
                        };
                        let mut sess =
                            if self.cfg.offline_menus {
                                crate::localserver::Session::offline(
                                    self.shop.grants.clone(),
                                    self.warehouse.clone(),
                                )
                            } else {
                                crate::localserver::Session::with_grants(self.shop.grants.clone())
                            };
                        sess.set_hero_class(hero_class);
                        // What the guest's receive buffer holds, read off the
                        // instruction that sizes it — 8 KB shipped, more with
                        // ten pages, whose listing needs it (`tenpages::RECV`).
                        sess.set_frame_limit(crate::tenpages::recv_capacity(self) as usize);
                        self.files.fds.insert(fd, FdObj::Local(sess));
                        self.log.push("connect: in-process server session".into());
                        if nb {
                            self.set_errno(EINPROGRESS);
                            self.ret(u32::MAX);
                        } else {
                            self.ret(0);
                        }
                    }
                    // No server configured: the deterministic dead-server world.
                    None => {
                        let e = if nb { EINPROGRESS } else { ECONNREFUSED };
                        self.set_errno(e);
                        self.ret(u32::MAX);
                    }
                }
            }
            "select" => {
                let nfds = self.arg(0);
                let (readp, writep, exceptp) = (self.arg(1), self.arg(2), self.arg(3));
                let tvp = self.arg(4);
                let timeout_ms = if tvp == 0 {
                    None
                } else {
                    Some(self.mem.read_u32(tvp) as i64 * 1000
                        + self.mem.read_u32(tvp + 4) as i64 / 1000)
                };

                // Blanket "everything is ready" is not good enough, and the way
                // it fails is instructive. `__knl_net_connect_check_timer`
                // (0x14d124) polls with a zero timeout and *branches on which
                // set came back*: readable means it peeks one byte
                // (`recv(s, b, 1, MSG_PEEK)`) and treats a non-positive result
                // as a dead connection, while writable-but-not-readable is the
                // normal completed non-blocking connect. Reporting a freshly
                // connected socket as readable therefore makes the guest hang
                // up on a server that is working perfectly.
                let want_r = self.fdset_read(readp, nfds);
                let want_w = self.fdset_read(writep, nfds);

                // No real stream and no dial in flight: the dead-server world
                // and/or in-process sessions. Neither can change readiness
                // while the guest is parked here — a local session's queue
                // only moves through this thread's own `send` — so there is
                // nothing to wait for: answer from the current state, and
                // bill an idle wait to the virtual clock so it costs no wall
                // time, exactly as the dead world always has.
                if !self.any_live_socket() {
                    let r: Vec<u32> =
                        want_r.iter().copied().filter(|&f| self.readable(f)).collect();
                    let w: Vec<u32> =
                        want_w.iter().copied().filter(|&f| self.writable(f)).collect();
                    if r.is_empty() && w.is_empty() {
                        for s in [readp, writep, exceptp] {
                            self.fdset_clear(s, nfds);
                        }
                        if let Some(ms) = timeout_ms {
                            self.advance_time(ms);
                        }
                        self.ret(0);
                    } else {
                        self.fdset_write(readp, nfds, &r);
                        self.fdset_write(writep, nfds, &w);
                        self.fdset_clear(exceptp, nfds);
                        self.ret((r.len() + w.len()) as u32);
                    }
                    return Ok(true);
                }

                let deadline = std::time::Instant::now()
                    + std::time::Duration::from_millis(timeout_ms.unwrap_or(0).max(0) as u64);
                loop {
                    // A dial can finish while the guest is parked in this very
                    // select; the completed-connect writability it is waiting
                    // for only appears once the stream is promoted.
                    self.poll_dials();
                    let r: Vec<u32> = want_r.iter().copied().filter(|&f| self.readable(f)).collect();
                    let w: Vec<u32> = want_w.iter().copied().filter(|&f| self.writable(f)).collect();
                    if !r.is_empty() || !w.is_empty() {
                        self.fdset_write(readp, nfds, &r);
                        self.fdset_write(writep, nfds, &w);
                        self.fdset_clear(exceptp, nfds);
                        self.ret((r.len() + w.len()) as u32);
                        return Ok(true);
                    }
                    // Nothing ready. A zero timeout is a poll, and `None` means
                    // "block forever" — which a headless replay must not do, so
                    // it waits the same bounded slice and comes back.
                    let now = std::time::Instant::now();
                    if timeout_ms == Some(0) || now >= deadline {
                        break;
                    }
                    let slice = std::time::Duration::from_millis(2)
                        .min(deadline.saturating_duration_since(now));
                    std::thread::sleep(slice);
                    self.advance_time(slice.as_millis() as i64);
                }
                for s in [readp, writep, exceptp] {
                    self.fdset_clear(s, nfds);
                }
                self.ret(0);
            }
            "recv" => {
                let (fd, buf, n, flags) = (self.arg(0), self.arg(1), self.arg(2), self.arg(3));
                // MSG_PEEK has to be honoured, not ignored: the connect check
                // above peeks one byte to decide whether the link is alive, and
                // consuming that byte would eat the first byte of the server's
                // first frame.
                let r = if flags & MSG_PEEK != 0 {
                    self.fd_peek(fd, n as usize)
                } else {
                    self.fd_read(fd, n as usize)
                };
                match r {
                    Ok(data) => {
                        self.mem.write_bytes(buf, &data);
                        self.ret(data.len() as u32);
                    }
                    Err(e) => {
                        self.set_errno(e);
                        self.ret(u32::MAX);
                    }
                }
            }
            "send" => {
                let (fd, buf, n) = (self.arg(0), self.arg(1), self.arg(2));
                let Some(data) = self.mem.slice(buf, n).map(<[u8]>::to_vec) else {
                    self.set_errno(EINVAL);
                    self.ret(u32::MAX);
                    return Ok(true);
                };
                match self.fd_write(fd, &data) {
                    Ok(m) => self.ret(m as u32),
                    Err(e) => {
                        self.set_errno(e);
                        self.ret(u32::MAX);
                    }
                }
            }
            "setsockopt" => self.ret(0),
            "getsockopt" => {
                // (fd, level, optname, optval, optlen). The read that matters
                // is SOL_SOCKET/SO_ERROR after a non-blocking connect: 0 when a
                // real connection is up, ETIMEDOUT in the dead-server world.
                let fd = self.arg(0);
                let (level, opt) = (self.arg(1), self.arg(2));
                let (val, lenp) = (self.arg(3), self.arg(4));
                // A dial still in flight is NOT a failure. `poll_dials` above
                // has already promoted anything that finished, so reaching here
                // with `dial: Some` means the worker is still connecting, and
                // the honest answer is "no error yet" — the guest is expected to
                // keep selecting. Reporting ETIMEDOUT here made a live server
                // indistinguishable from the dead-server world: the guest read
                // it one frame after `connect`, concluded the connection was
                // gone and closed the socket without sending a byte, which is
                // exactly what the test server logged (connect, then close, no
                // data). Before the non-blocking dial (ANALYSIS.md §17.2) this
                // could not happen, because `connect` returned only once
                // `stream` was `Some`.
                let (connected, dialing) = match self.files.fds.get(&fd) {
                    Some(FdObj::Socket { stream, dial, .. }) => (stream.is_some(), dial.is_some()),
                    // The in-process session is a completed connect; ETIMEDOUT
                    // here would make the guest hang up on it unopened.
                    Some(FdObj::Local(_)) => (true, false),
                    _ => (false, false),
                };
                let answer =
                    if level == 1 && opt == 4 && !connected && !dialing { ETIMEDOUT } else { 0 };
                if val != 0 {
                    self.mem.write_u32(val, answer);
                }
                if lenp != 0 {
                    self.mem.write_u32(lenp, 4);
                }
                self.ret(0);
            }
            "shutdown" => self.ret(0),
            "inet_addr" => {
                let a0 = self.arg(0);
                let s = self.mem.cstr(a0);
                let mut parts = [0u32; 4];
                let mut ok = 0;
                for (i, p) in s.trim().split('.').enumerate() {
                    if i >= 4 {
                        ok = 0;
                        break;
                    }
                    match p.parse::<u32>() {
                        Ok(v) if v <= 255 => {
                            parts[i] = v;
                            ok = i + 1;
                        }
                        _ => {
                            ok = 0;
                            break;
                        }
                    }
                }
                // Network byte order in memory = a in the low register byte.
                let r = if ok == 4 {
                    parts[0] | parts[1] << 8 | parts[2] << 16 | parts[3] << 24
                } else {
                    u32::MAX // INADDR_NONE
                };
                self.ret(r);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
