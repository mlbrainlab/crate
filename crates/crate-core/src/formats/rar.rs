//! RAR (read only) through the UnRAR library (`unrar_sys`). UnRAR's licence
//! allows extraction in any software but forbids writing RAR; see README.
//!
//! UnRAR pushes file data through a callback while it processes an entry. To
//! give the extraction engine an ordinary `Read`, a worker thread runs UnRAR
//! and sends the data in chunks over a small bounded channel. UnRAR never
//! writes to disk itself; everything goes through `crate::safety`.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::time::{Duration, SystemTime};

use unrar_sys as native;

use super::{EntrySink, FormatReader, RawEntry, RawKind, civil_to_system_time};
use crate::entry::Format;
use crate::error::{Error, Result};

/// RAR5 link types (`RARHeaderDataEx::RedirType`).
const REDIR_UNIX_SYMLINK: u32 = 1;
const REDIR_WIN_SYMLINK: u32 = 2;
const REDIR_JUNCTION: u32 = 3;
const REDIR_HARDLINK: u32 = 4;
const REDIR_FILECOPY: u32 = 5;
const HOST_UNIX: u32 = 3;
const RAR_VOL_ASK: native::LPARAM = 0;

pub(crate) struct RarReader {
    path: PathBuf,
    count: usize,
}

impl RarReader {
    pub fn new(path: PathBuf) -> Self {
        Self { path, count: 0 }
    }
}

/// An open UnRAR handle, closed on drop.
struct Handle(*const native::Handle);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { native::RARCloseArchive(self.0) };
    }
}

/// What the UnRAR callback writes into. `active` is true only while a wanted
/// entry is being processed, so skipped entries never reach the channel.
struct CallbackData {
    data: Option<SyncSender<Msg>>,
    active: std::cell::Cell<bool>,
}

enum Msg {
    Start(usize),
    Data(Vec<u8>),
    /// The entry is done; `Some(code)` when UnRAR reported an error.
    End(Option<i32>),
    /// Opening or reading headers failed.
    Failed(Error),
}

extern "C" fn callback(
    msg: native::UINT,
    user: native::LPARAM,
    p1: native::LPARAM,
    p2: native::LPARAM,
) -> i32 {
    if user == 0 {
        return -1;
    }
    let user = unsafe { &*(user as *const CallbackData) };
    match msg {
        native::UCM_PROCESSDATA => {
            if !user.active.get() {
                return 0;
            }
            let Some(tx) = &user.data else { return -1 };
            let chunk = unsafe { std::slice::from_raw_parts(p1 as *const u8, p2 as usize) };
            // A closed channel means the consumer stopped (cancel, limit,
            // error): abort UnRAR.
            if tx.send(Msg::Data(chunk.to_vec())).is_ok() {
                0
            } else {
                -1
            }
        }
        // No password support until M2; a missing next volume stops too.
        native::UCM_NEEDPASSWORD | native::UCM_NEEDPASSWORDW => -1,
        native::UCM_CHANGEVOLUME | native::UCM_CHANGEVOLUMEW if p2 == RAR_VOL_ASK => -1,
        _ => 0,
    }
}

fn open(path: &Path, mode: u32, user: *const CallbackData) -> Result<Handle> {
    let name = archive_name(path)?;
    let mut data = native::OpenArchiveDataEx::new(name.as_ptr(), mode);
    data.callback = Some(callback);
    data.user_data = user as native::LPARAM;
    let handle = unsafe { native::RAROpenArchiveEx(&data) };
    if handle.is_null() || data.open_result != native::ERAR_SUCCESS as u32 {
        if !handle.is_null() {
            drop(Handle(handle));
        }
        return Err(rar_error(data.open_result as i32));
    }
    Ok(Handle(handle))
}

#[cfg(unix)]
fn archive_name(path: &Path) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| Error::UnknownFormat(path.to_owned()))
}

#[cfg(not(unix))]
fn archive_name(_path: &Path) -> Result<std::ffi::CString> {
    // Wide-character paths for Windows come with M7.
    Err(Error::Unsupported("RAR"))
}

fn rar_error(code: i32) -> Error {
    match code {
        native::ERAR_MISSING_PASSWORD | native::ERAR_BAD_PASSWORD => Error::PasswordRequired,
        native::ERAR_EOPEN => Error::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "cannot open the RAR file",
        )),
        native::ERAR_NO_MEMORY => Error::Io(io::Error::from(io::ErrorKind::OutOfMemory)),
        native::ERAR_EREAD => Error::Io(io::Error::other("read error")),
        native::ERAR_BAD_DATA => Error::Corrupt("checksum error".into()),
        native::ERAR_EREFERENCE => Error::Corrupt("missing file the archive refers to".into()),
        native::ERAR_UNKNOWN_FORMAT => Error::Corrupt("unknown RAR format".into()),
        native::ERAR_END_ARCHIVE => {
            Error::Corrupt("unexpected end of archive (missing volume?)".into())
        }
        _ => Error::Corrupt(format!("damaged RAR archive (UnRAR error {code})")),
    }
}

fn wide_to_string(w: &[libc::wchar_t]) -> String {
    w.iter()
        .take_while(|&&c| c != 0)
        .map(|&c| char::from_u32(c as u32).unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

fn filetime(low: u32, high: u32) -> Option<SystemTime> {
    let ticks = (u64::from(high) << 32) | u64::from(low);
    let secs = (ticks / 10_000_000).checked_sub(11_644_473_600)?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
}

fn dos_time(t: u32) -> Option<SystemTime> {
    civil_to_system_time(
        1980 + (t >> 25) as i64,
        (t >> 21) & 0xF,
        (t >> 16) & 0x1F,
        (t >> 11) & 0x1F,
        (t >> 5) & 0x3F,
        (t & 0x1F) * 2,
    )
}

/// Reads the next header into `header`; `Ok(false)` at the end.
fn next_header(handle: &Handle, header: &mut native::HeaderDataEx) -> Result<bool> {
    match unsafe { native::RARReadHeaderEx(handle.0, header) } {
        native::ERAR_SUCCESS => Ok(true),
        native::ERAR_END_ARCHIVE => Ok(false),
        code => Err(rar_error(code)),
    }
}

impl FormatReader for RarReader {
    fn format(&self) -> Format {
        Format::Rar
    }

    fn list(&mut self) -> Result<Vec<RawEntry>> {
        let user = CallbackData {
            data: None,
            active: false.into(),
        };
        let handle = open(&self.path, native::RAR_OM_LIST, &user)?;
        let mut header: Box<native::HeaderDataEx> = Box::default();
        let mut redir = vec![0 as libc::wchar_t; 4096];
        let mut out = Vec::new();
        loop {
            header.redir_name = redir.as_mut_ptr();
            header.redir_name_size = redir.len() as u32;
            if !next_header(&handle, &mut header)? {
                break;
            }
            let name = wide_to_string(&header.filename_w);
            let target = || wide_to_string(&redir).into_bytes();
            let unix = header.host_os == HOST_UNIX;
            let kind = match header.redir_type {
                REDIR_UNIX_SYMLINK | REDIR_WIN_SYMLINK | REDIR_JUNCTION => {
                    RawKind::Symlink(target())
                }
                REDIR_HARDLINK | REDIR_FILECOPY => RawKind::Hardlink(target()),
                0 if header.flags & native::RHDF_DIRECTORY != 0 => RawKind::Dir,
                0 => RawKind::File,
                _ => RawKind::Other,
            };
            let size = (u64::from(header.unp_size_high) << 32) | u64::from(header.unp_size);
            let packed = (u64::from(header.pack_size_high) << 32) | u64::from(header.pack_size);
            out.push(RawEntry {
                name: name.into_bytes(),
                size: if kind == RawKind::File { size } else { 0 },
                kind,
                packed_size: Some(packed),
                modified: filetime(header.mtime_low, header.mtime_high)
                    .or_else(|| dos_time(header.file_time)),
                mode: unix.then_some(header.file_attr & 0o7777),
                crc32: Some(header.file_crc),
                encrypted: header.flags & native::RHDF_ENCRYPTED != 0,
            });
            let code = unsafe {
                native::RARProcessFileW(
                    handle.0,
                    native::RAR_SKIP,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
            if code != native::ERAR_SUCCESS {
                return Err(rar_error(code));
            }
        }
        // UnRAR reports a header cut off mid-way as the end of the archive; an
        // empty RAR is rare enough to call damaged.
        if out.is_empty() {
            return Err(Error::Corrupt(
                "no entries found (damaged or truncated RAR)".into(),
            ));
        }
        self.count = out.len();
        Ok(out)
    }

    fn read(&mut self, wanted: &dyn Fn(usize) -> bool, sink: &mut EntrySink<'_>) -> Result<()> {
        let wanted: Vec<bool> = (0..self.count).map(wanted).collect();
        if !wanted.contains(&true) {
            return Ok(());
        }
        let (tx, rx) = sync_channel::<Msg>(8);
        let path = self.path.clone();
        let worker = std::thread::spawn(move || unrar_worker(&path, &wanted, tx));
        let result = consume(&rx, sink);
        // Dropping the receiver makes the worker's next send fail, which
        // aborts UnRAR if we stopped early.
        drop(rx);
        let _ = worker.join();
        result
    }
}

/// Worker thread: walks the archive and streams the wanted entries.
fn unrar_worker(path: &Path, wanted: &[bool], tx: SyncSender<Msg>) {
    let user = CallbackData {
        data: Some(tx.clone()),
        active: false.into(),
    };
    let handle = match open(path, native::RAR_OM_EXTRACT, &user) {
        Ok(h) => h,
        Err(e) => {
            let _ = tx.send(Msg::Failed(e));
            return;
        }
    };
    let mut header: Box<native::HeaderDataEx> = Box::default();
    for (i, &want) in wanted.iter().enumerate() {
        match next_header(&handle, &mut header) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                let _ = tx.send(Msg::Failed(e));
                return;
            }
        }
        let op = if want {
            native::RAR_TEST
        } else {
            native::RAR_SKIP
        };
        if want && tx.send(Msg::Start(i)).is_err() {
            return;
        }
        user.active.set(want);
        let code =
            unsafe { native::RARProcessFileW(handle.0, op, std::ptr::null(), std::ptr::null()) };
        user.active.set(false);
        if want {
            let end = (code != native::ERAR_SUCCESS).then_some(code);
            if tx.send(Msg::End(end)).is_err() {
                return;
            }
        } else if code != native::ERAR_SUCCESS {
            let _ = tx.send(Msg::Failed(rar_error(code)));
            return;
        }
    }
}

/// Main thread: hands each streamed entry to the sink.
fn consume(rx: &Receiver<Msg>, sink: &mut EntrySink<'_>) -> Result<()> {
    while let Ok(msg) = rx.recv() {
        match msg {
            Msg::Start(i) => {
                let mut reader = ChannelReader {
                    rx,
                    buf: Vec::new(),
                    pos: 0,
                    done: false,
                };
                sink(i, &mut reader)?;
                // Drain what the sink didn't read, so the next entry lines up.
                io::copy(&mut reader, &mut io::sink())?;
            }
            Msg::Failed(e) => return Err(e),
            Msg::Data(_) | Msg::End(_) => return Err(Error::Corrupt("unexpected RAR data".into())),
        }
    }
    Ok(())
}

/// One entry's data, read from the worker's chunks.
struct ChannelReader<'a> {
    rx: &'a Receiver<Msg>,
    buf: Vec<u8>,
    pos: usize,
    done: bool,
}

impl Read for ChannelReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.pos == self.buf.len() {
            if self.done {
                return Ok(0);
            }
            match self.rx.recv() {
                Ok(Msg::Data(chunk)) => {
                    self.buf = chunk;
                    self.pos = 0;
                }
                Ok(Msg::End(None)) => self.done = true,
                Ok(Msg::End(Some(code))) => {
                    self.done = true;
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        rar_error(code).to_string(),
                    ));
                }
                Ok(Msg::Failed(e)) => {
                    self.done = true;
                    return Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string()));
                }
                Ok(Msg::Start(_)) | Err(_) => {
                    self.done = true;
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "RAR data ended early",
                    ));
                }
            }
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}
