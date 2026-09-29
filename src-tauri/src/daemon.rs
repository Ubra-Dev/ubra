//! Headless agent-runtime daemon (Phase 5a): owns PTYs and the agent
//! watcher, serving a JSON-lines protocol over loopback TCP.
//!
//! Requests carry `op` and a response `id`; authenticated responses echo
//! the id and carry a boolean `ok`. Protocol 2 uses nonce-bound HMAC-SHA256
//! proofs in both directions, with distinct server/client roles. The client
//! proves identity only after verifying the server; credentials never
//! travel over TCP. Pane state and pushed events require authentication.
//! Runtime files have current-user-only access. A persistent OS-locked
//! inode serializes startup and remains present after shutdown/crashes.
//! Connection, pane, frame and outstanding output budgets are bounded.
//! A lagging consumer is disconnected rather than silently dropping bytes.

use crate::agent_status::AgentStatusService;
use crate::pty_manager::{PaneId, PtyEventSink, PtyManager};
use fs2::FileExt;
use hmac::{Hmac, Mac};
use parking_lot::Mutex;
use sha2::Sha256;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

pub const PROTOCOL_VERSION: u32 = 2;
pub const MAX_CLIENTS: usize = 32;
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
pub const MAX_PANES: usize = 64;
pub const MAX_QUEUE_BYTES: usize = 2 * 1024 * 1024;
const MAX_QUEUE_MESSAGES: usize = 256;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const FRAME_TIMEOUT: Duration = Duration::from_secs(10);

pub const PORT_FILE: &str = "daemon.json";
pub const AUTH_FILE: &str = "daemon.auth";
pub const LOCK_DIR: &str = "daemon.lock";
pub const APP_IDENTIFIER: &str = "com.nemoryoliver.ubra";

/// Request envelope: flat object, `op` plus per-op arguments.
#[derive(Debug, serde::Deserialize)]
pub struct Request {
    pub op: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub nonce: Option<String>,
    #[serde(default)]
    pub proof: Option<String>,
    #[serde(default)]
    pub pane: Option<PaneId>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
    #[serde(default)]
    pub keys: Option<Vec<String>>,
    #[serde(default)]
    pub want: Option<Vec<String>>,
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

fn respond(id: Option<u64>, mut value: serde_json::Value) -> String {
    if let Some(id) = id {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("id".to_string(), id.into());
        }
    }
    let encoded = value.to_string();
    if encoded.len() + 1 > MAX_FRAME_BYTES {
        let mut error = serde_json::json!({"ok":false,"error":"response exceeds frame byte limit"});
        if let Some(id) = id {
            error["id"] = id.into();
        }
        error.to_string()
    } else {
        encoded
    }
}

fn ok(id: Option<u64>, value: serde_json::Value) -> String {
    respond(id, value)
}

fn err(id: Option<u64>, msg: impl std::fmt::Display) -> String {
    respond(
        id,
        serde_json::json!({"ok": false, "error": msg.to_string()}),
    )
}

/// Private per-user runtime directory, independent of spoofable USER variables.
pub fn default_state_dir() -> PathBuf {
    #[cfg(unix)]
    let name = format!("ubra-{}", unsafe { libc::geteuid() });
    #[cfg(windows)]
    let name = "ubra-runtime".to_string();
    #[cfg(unix)]
    let root = fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
    #[cfg(windows)]
    let root = app_data_dir();
    root.join(name)
}

/// Contents of `<state-dir>/daemon.json`: where the daemon listens.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PortFile {
    pub port: u16,
    pub pid: u32,
}

fn permission_error(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

fn validate_private(file: &File, directory: bool) -> io::Result<()> {
    let metadata = file.metadata()?;
    if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err(permission_error("runtime object has the wrong type"));
    }
    #[cfg(unix)]
    if metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
        || (!directory && metadata.nlink() != 1)
    {
        return Err(permission_error(
            "runtime object must be private and owned by the current user",
        ));
    }
    #[cfg(windows)]
    windows_security::validate(file)?;
    Ok(())
}

pub fn open_private_file(path: &Path, append: bool, create: bool) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        secure_state_dir(parent)?;
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(append || create)
        .append(append)
        .create(create);
    #[cfg(unix)]
    options
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    validate_private(&file, false)?;
    Ok(file)
}

fn read_private_string(path: &Path) -> Option<String> {
    let mut file = open_private_file(path, false, false).ok()?;
    let mut text = String::new();
    Read::by_ref(&mut file)
        .take(4097)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() <= 4096).then_some(text)
}

pub fn read_port_file(dir: &Path) -> Option<PortFile> {
    serde_json::from_str(&read_private_string(&dir.join(PORT_FILE))?).ok()
}

pub fn read_auth_token(dir: &Path) -> Option<String> {
    let text = read_private_string(&dir.join(AUTH_FILE))?;
    let token = text.trim();
    (token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())).then(|| token.to_string())
}

pub fn secure_state_dir(dir: &Path) -> io::Result<()> {
    let absolute = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        std::env::current_dir()?.join(dir)
    };
    let mut prefix = PathBuf::new();
    for component in absolute.components() {
        prefix.push(component);
        match fs::symlink_metadata(&prefix) {
            Ok(meta) => {
                #[cfg(unix)]
                let trusted_system_link = meta.file_type().is_symlink()
                    && prefix != absolute
                    && meta.uid() == 0
                    && fs::metadata(&prefix).is_ok_and(|target| target.is_dir());
                #[cfg(not(unix))]
                let trusted_system_link = false;
                if (meta.file_type().is_symlink() && !trusted_system_link)
                    || (!meta.is_dir() && !trusted_system_link)
                {
                    return Err(permission_error(
                        "runtime directory must not follow user-controlled symlinks",
                    ));
                }
                #[cfg(unix)]
                if !trusted_system_link
                    && (meta.uid() != 0 && meta.uid() != unsafe { libc::geteuid() }
                        || (meta.mode() & 0o022 != 0 && meta.mode() & 0o1000 == 0))
                {
                    return Err(permission_error(
                        "runtime directory ancestor is writable by another user",
                    ));
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        return Err(permission_error(
                            "runtime directory ancestors must not be reparse points",
                        ));
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    match fs::DirBuilder::new().mode(0o700).create(&prefix) {
                        Ok(()) => {}
                        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(e),
                    }
                }
                #[cfg(windows)]
                match windows_security::create_directory(&prefix) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e),
                }
            }
            Err(e) => return Err(e),
        }
    }
    #[cfg(unix)]
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(&absolute)?;
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(
                windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT
                    | windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS,
            )
            .open(&absolute)?
    };
    validate_private(&file, true)
}

fn write_private_file(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        secure_state_dir(parent)?;
    }
    let tmp = path.with_file_name(format!(".runtime-{}.tmp", new_auth_token()?));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let result = (|| {
        let mut file = options.open(&tmp)?;
        validate_private(&file, false)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    let _ = fs::remove_file(&tmp);
    result
}

pub fn new_auth_token() -> io::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut token, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(token)
}

fn token_matches(expected: &str, provided: &str) -> bool {
    let expected = expected.as_bytes();
    let provided = provided.as_bytes();
    if expected.len() != provided.len() {
        return false;
    }
    expected
        .iter()
        .zip(provided)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

pub fn write_auth_token(dir: &Path, token: &str) -> io::Result<()> {
    write_private_file(&dir.join(AUTH_FILE), &format!("{token}\n"))
}

pub fn write_port_file(dir: &Path, port: u16) -> io::Result<()> {
    write_private_file(
        &dir.join(PORT_FILE),
        &serde_json::json!({"port": port, "pid": std::process::id()}).to_string(),
    )
}

pub fn remove_runtime_files(dir: &Path) {
    let _ = fs::remove_file(dir.join(PORT_FILE));
    let _ = fs::remove_file(dir.join(AUTH_FILE));
}

fn auth_proof(token: &str, role: &str, client: &str, server: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(token.as_bytes()).expect("HMAC accepts any key size");
    mac.update(format!("ubra-v{PROTOCOL_VERSION}:{role}:{client}:{server}").as_bytes());
    let mut proof = String::with_capacity(64);
    for byte in mac.finalize().into_bytes() {
        write!(&mut proof, "{byte:02x}").expect("String write");
    }
    proof
}

fn valid_nonce(nonce: &str) -> bool {
    nonce.len() == 64 && nonce.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Bounded framing with a total deadline, including trickled partial frames.
pub fn read_frame(reader: &mut BufReader<TcpStream>, deadline: Instant) -> io::Result<String> {
    let mut bytes = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "frame deadline exceeded",
            ));
        }
        reader.get_ref().set_read_timeout(Some(remaining))?;
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed",
            ));
        }
        let newline = buffer.iter().position(|b| *b == b'\n');
        let count = newline.map_or(buffer.len(), |n| n + 1);
        if bytes.len() + count > MAX_FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "frame exceeds byte limit",
            ));
        }
        bytes.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if newline.is_some() {
            return String::from_utf8(bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
        }
    }
}

/// Write a complete frame within one total deadline, even with trickled reads.
pub fn write_frame(stream: &mut TcpStream, message: &str, deadline: Instant) -> io::Result<()> {
    if message.len() + 1 > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds byte limit",
        ));
    }
    for mut bytes in [message.as_bytes(), b"\n".as_slice()] {
        while !bytes.is_empty() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "write deadline exceeded",
                ));
            }
            stream.set_write_timeout(Some(remaining))?;
            let count = stream.write(bytes)?;
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "connection closed",
                ));
            }
            bytes = &bytes[count..];
        }
    }
    Ok(())
}

fn auth_reply(
    reader: &mut BufReader<TcpStream>,
    deadline: Instant,
) -> io::Result<serde_json::Value> {
    serde_json::from_str(&read_frame(reader, deadline)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn authenticate_until(
    mut stream: TcpStream,
    state_dir: &Path,
    deadline: Instant,
) -> io::Result<TcpStream> {
    let token = read_auth_token(state_dir).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "missing or unsafe daemon credential",
        )
    })?;
    let client_nonce = new_auth_token()?;
    write_frame(
        &mut stream,
        &serde_json::json!({"op":"hello","id":0,"nonce":client_nonce}).to_string(),
        deadline,
    )?;
    // Capacity one prevents losing events when returning the authenticated socket.
    let mut reader = BufReader::with_capacity(1, stream.try_clone()?);
    let challenge = auth_reply(&mut reader, deadline)?;
    let server_nonce = challenge["nonce"]
        .as_str()
        .filter(|n| valid_nonce(n))
        .ok_or_else(|| permission_error("invalid daemon identity challenge"))?;
    if challenge["id"] != 0
        || challenge["protocol"] != PROTOCOL_VERSION
        || !challenge["proof"].as_str().is_some_and(|proof| {
            token_matches(
                &auth_proof(&token, "server", &client_nonce, server_nonce),
                proof,
            )
        })
    {
        return Err(permission_error("daemon identity verification failed"));
    }
    write_frame(
        &mut stream,
        &serde_json::json!({
            "op":"auth","id":0,"proof":auth_proof(&token,"client",&client_nonce,server_nonce)
        })
        .to_string(),
        deadline,
    )?;
    let reply = auth_reply(&mut reader, deadline)?;
    if reply["id"] != 0 || reply["ok"] != true || reply["protocol"] != PROTOCOL_VERSION {
        return Err(permission_error("daemon authentication failed"));
    }
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    Ok(stream)
}

pub fn authenticate_stream(stream: TcpStream, state_dir: &Path) -> io::Result<TcpStream> {
    authenticate_until(stream, state_dir, Instant::now() + IO_TIMEOUT)
}

pub fn connect_authenticated(state_dir: &Path, timeout: Duration) -> io::Result<TcpStream> {
    let deadline = Instant::now() + timeout;
    let port = read_port_file(state_dir).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "missing or unsafe daemon port file",
        )
    })?;
    let addr = SocketAddr::from(([127, 0, 0, 1], port.port));
    let stream = TcpStream::connect_timeout(&addr, timeout)?;
    authenticate_until(stream, state_dir, deadline)
}

pub struct StartupLock {
    _file: File,
}

pub fn acquire_startup_lock(state_dir: &Path) -> io::Result<StartupLock> {
    let file = open_private_file(&state_dir.join(LOCK_DIR), false, true)?;
    FileExt::try_lock_exclusive(&file).map_err(|e| {
        if e.kind() == io::ErrorKind::WouldBlock {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "daemon owner already running or starting",
            )
        } else {
            e
        }
    })?;
    // Keep the inode forever: unlinking a locked file permits a second owner.
    Ok(StartupLock { _file: file })
}

#[cfg(windows)]
mod windows_security {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::{ffi::OsStrExt, fs::MetadataExt, io::AsRawHandle};
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        GetSecurityInfo, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        EqualSid, GetAce, GetTokenInformation, TokenUser, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL,
        DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateDirectoryW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    struct LocalAllocation(*mut c_void);
    impl Drop for LocalAllocation {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }

    fn user_token() -> io::Result<Vec<u64>> {
        unsafe {
            let mut token = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut length = 0;
            GetTokenInformation(token, TokenUser, null_mut(), 0, &mut length);
            let mut data = vec![0_u64; (length as usize).div_ceil(8)];
            let result = GetTokenInformation(
                token,
                TokenUser,
                data.as_mut_ptr().cast(),
                length,
                &mut length,
            );
            let error = io::Error::last_os_error();
            CloseHandle(token);
            if result == 0 {
                return Err(error);
            }
            Ok(data)
        }
    }

    pub(super) fn create_directory(path: &Path) -> io::Result<()> {
        unsafe {
            let token = user_token()?;
            let sid = (*(token.as_ptr().cast::<TOKEN_USER>())).User.Sid;
            let mut text = null_mut();
            if ConvertSidToStringSidW(sid, &mut text) == 0 {
                return Err(io::Error::last_os_error());
            }
            let _text = LocalAllocation(text.cast());
            let mut length = 0;
            while *text.add(length) != 0 {
                length += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, length));
            // Protected current-user-only DACL, inherited by runtime files.
            let sddl: Vec<u16> = format!("O:{sid}D:P(A;OICI;FA;;;{sid})")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let _descriptor = LocalAllocation(descriptor);
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            };
            let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if CreateDirectoryW(path.as_ptr(), &attributes) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    pub(super) fn validate(file: &File) -> io::Result<()> {
        if file.metadata()?.file_attributes() & 0x400 != 0 {
            return Err(permission_error(
                "runtime object must not be a reparse point",
            ));
        }
        unsafe {
            let mut information = BY_HANDLE_FILE_INFORMATION::default();
            if GetFileInformationByHandle(file.as_raw_handle(), &mut information) == 0 {
                return Err(io::Error::last_os_error());
            }
            if file.metadata()?.is_file() && information.nNumberOfLinks != 1 {
                return Err(permission_error("runtime file must not have hard links"));
            }
            let token = user_token()?;
            let user = (*(token.as_ptr().cast::<TOKEN_USER>())).User.Sid;
            let mut owner: PSID = null_mut();
            let mut acl: *mut ACL = null_mut();
            let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
            let error = GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                &mut acl,
                null_mut(),
                &mut descriptor,
            );
            if error != 0 {
                return Err(io::Error::from_raw_os_error(error as i32));
            }
            let _descriptor = LocalAllocation(descriptor);
            if owner.is_null()
                || EqualSid(owner, user) == 0
                || acl.is_null()
                || (*acl).AceCount == 0
            {
                return Err(permission_error(
                    "runtime object must have current-user-only DACL and owner",
                ));
            }
            for index in 0..(*acl).AceCount {
                let mut ace = null_mut();
                if GetAce(acl, index as u32, &mut ace) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let header = &*(ace.cast::<ACE_HEADER>());
                if header.AceType != 0
                    || usize::from(header.AceSize) < std::mem::size_of::<ACCESS_ALLOWED_ACE>()
                {
                    return Err(permission_error(
                        "runtime DACL must contain only current-user allow entries",
                    ));
                }
                let allowed = &*(ace.cast::<ACCESS_ALLOWED_ACE>());
                let sid = std::ptr::addr_of!(allowed.SidStart).cast_mut().cast();
                if EqualSid(sid, user) == 0 {
                    return Err(permission_error(
                        "runtime DACL grants access outside current user",
                    ));
                }
            }
            Ok(())
        }
    }
}

/// App data dir, mirroring Tauri's `appDataDir` for our identifier so the
/// daemon and GUI share paths (`<platform-data-dir>/<identifier>`).
pub fn app_data_dir() -> PathBuf {
    app_data_dir_with(
        &home_dir(),
        std::env::var("XDG_DATA_HOME").ok(),
        std::env::var("APPDATA").ok(),
    )
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn app_data_dir_with(
    home: &Path,
    xdg_data_home: Option<String>,
    appdata: Option<String>,
) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let _ = (xdg_data_home, appdata);
        home.join("Library")
            .join("Application Support")
            .join(APP_IDENTIFIER)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = xdg_data_home;
        appdata
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
            .join(APP_IDENTIFIER)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = appdata;
        xdg_data_home
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join(APP_IDENTIFIER)
    }
}

struct Queued {
    message: Arc<str>,
    flushed: Option<mpsc::SyncSender<()>>,
}

#[derive(Clone)]
struct Peer {
    tx: mpsc::SyncSender<Queued>,
    socket: Arc<TcpStream>,
    bytes: Arc<AtomicUsize>,
}

impl Peer {
    fn send(&self, message: Arc<str>, flushed: Option<mpsc::SyncSender<()>>) -> bool {
        let size = message.len() + 1;
        if size > MAX_FRAME_BYTES {
            let _ = self.socket.shutdown(std::net::Shutdown::Both);
            return false;
        }
        if self
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(size)
                    .filter(|total| *total <= MAX_QUEUE_BYTES)
            })
            .is_err()
        {
            let _ = self.socket.shutdown(std::net::Shutdown::Both);
            return false;
        }
        if self.tx.try_send(Queued { message, flushed }).is_err() {
            self.bytes.fetch_sub(size, Ordering::AcqRel);
            let _ = self.socket.shutdown(std::net::Shutdown::Both);
            return false;
        }
        true
    }
}

type Peers = Arc<Mutex<HashMap<u64, Peer>>>;

fn broadcast(peers: &Peers, message: String) {
    let message: Arc<str> = message.into();
    peers
        .lock()
        .retain(|_, peer| peer.send(message.clone(), None));
}

/// Daemon state shared by bounded connection threads and the agent poll thread.
pub struct DaemonCore {
    manager: Arc<PtyManager>,
    statuses: AgentStatusService,
    peers: Peers,
    next_peer: AtomicU64,
    connections: AtomicUsize,
    spawn_guard: Mutex<()>,
    state_dir: PathBuf,
}

impl DaemonCore {
    pub fn new(rules_dir: Option<PathBuf>, state_dir: PathBuf) -> Arc<Self> {
        let peers = Arc::new(Mutex::new(HashMap::new()));
        let manager = Arc::new(PtyManager::new_headless(Arc::new(DaemonSink {
            peers: peers.clone(),
        })));
        let status_peers = peers.clone();
        let statuses = AgentStatusService::start(&manager, rules_dir, move |update| {
            broadcast(
                &status_peers,
                serde_json::json!({"event":"agent-states","states":update.states}).to_string(),
            );
            broadcast(
                &status_peers,
                serde_json::json!({
                    "event":"agent-state-update","revision":update.revision,
                    "states":update.states,"transitions":update.transitions
                })
                .to_string(),
            );
        });
        Arc::new(Self {
            manager,
            statuses,
            peers,
            next_peer: AtomicU64::new(1),
            connections: AtomicUsize::new(0),
            spawn_guard: Mutex::new(()),
            state_dir,
        })
    }
}

struct DaemonSink {
    peers: Peers,
}

impl PtyEventSink for DaemonSink {
    fn output(&self, id: PaneId, data: String, sequence: u64) {
        broadcast(
            &self.peers,
            serde_json::json!({
                "event":"pty_output","pane":id,"data":data,"sequence":sequence
            })
            .to_string(),
        );
    }

    fn exited(&self, id: PaneId, success: bool, code: Option<i32>) {
        broadcast(
            &self.peers,
            serde_json::json!({
                "event":"pty_exit","pane":id,"success":success,"code":code
            })
            .to_string(),
        );
    }
}

pub(crate) enum Action {
    Respond(String),
    Shutdown(String),
}

fn need_pane(req: &Request) -> Result<PaneId, String> {
    req.pane.ok_or_else(|| "missing \"pane\"".to_string())
}

/// Encode one `send-keys` key: single characters pass through literally,
/// anything longer must be a known key name.
fn key_to_bytes(key: &str) -> Result<String, String> {
    if key.chars().count() == 1 {
        return Ok(key.to_string());
    }
    let named = match key.to_lowercase().as_str() {
        "enter" => "\r",
        "escape" | "esc" => "\u{1b}",
        "tab" => "\t",
        "backspace" => "\u{7f}",
        "space" => " ",
        "up" => "\u{1b}[A",
        "down" => "\u{1b}[B",
        "right" => "\u{1b}[C",
        "left" => "\u{1b}[D",
        "home" => "\u{1b}[H",
        "end" => "\u{1b}[F",
        "pageup" => "\u{1b}[5~",
        "pagedown" => "\u{1b}[6~",
        "f1" => "\u{1b}OP",
        "f2" => "\u{1b}OQ",
        "f3" => "\u{1b}OR",
        "f4" => "\u{1b}OS",
        "f5" => "\u{1b}[15~",
        "f6" => "\u{1b}[17~",
        "f7" => "\u{1b}[18~",
        "f8" => "\u{1b}[19~",
        "f9" => "\u{1b}[20~",
        "f10" => "\u{1b}[21~",
        "f11" => "\u{1b}[23~",
        "f12" => "\u{1b}[24~",
        name => {
            if let Some(letter) = name.strip_prefix("ctrl-") {
                let mut chars = letter.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    if c.is_ascii_alphabetic() {
                        let byte = (c.to_ascii_lowercase() as u8) - b'a' + 1;
                        return Ok((byte as char).to_string());
                    }
                }
            }
            return Err(format!("unknown key: {key}"));
        }
    };
    Ok(named.to_string())
}

fn pane_alive(core: &DaemonCore, pane: PaneId) -> bool {
    core.manager.pane_roots().iter().any(|p| p.id == pane)
}

fn wait_timeout(req: &Request) -> Duration {
    Duration::from_secs(req.timeout_secs.unwrap_or(30).clamp(1, 600))
}

#[cfg(test)]
fn dispatch(core: &DaemonCore, req: &Request) -> Action {
    dispatch_connected(core, req, None)
}

fn client_connected(socket: Option<&TcpStream>) -> bool {
    let Some(socket) = socket else {
        return true;
    };
    if socket
        .set_read_timeout(Some(Duration::from_millis(1)))
        .is_err()
    {
        return false;
    }
    match socket.peek(&mut [0_u8; 1]) {
        Ok(0) => false,
        Ok(_) => true,
        Err(error) => matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ),
    }
}

fn dispatch_connected(core: &DaemonCore, req: &Request, socket: Option<&TcpStream>) -> Action {
    let id = req.id;
    let respond = |value: serde_json::Value| Action::Respond(ok(id, value));
    match req.op.as_str() {
        "ping" => respond(serde_json::json!({
            "ok": true,
            "version": env!("CARGO_PKG_VERSION"),
            "protocol": PROTOCOL_VERSION,
        })),
        "pty_spawn" => {
            let _guard = core.spawn_guard.lock();
            if core.manager.pane_roots().len() >= MAX_PANES {
                return Action::Respond(err(id, "pane limit reached"));
            }
            match core.manager.spawn(
                req.shell.clone(),
                req.cwd.clone(),
                req.args.clone().unwrap_or_default(),
                req.cols.unwrap_or(80),
                req.rows.unwrap_or(24),
            ) {
                Ok(pane) => respond(serde_json::json!({"ok":true,"pane":pane})),
                Err(e) => Action::Respond(err(id, e)),
            }
        }
        "pty_write" => {
            let (Some(pane), Some(data)) = (req.pane, req.data.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"data\""));
            };
            match core.manager.write(pane, &data) {
                Ok(()) => respond(serde_json::json!({"ok": true})),
                Err(e) => Action::Respond(err(id, e)),
            }
        }
        "pty_resize" => match (req.pane, req.cols, req.rows) {
            (Some(pane), Some(cols), Some(rows)) => match core.manager.resize(pane, cols, rows) {
                Ok(()) => respond(serde_json::json!({"ok": true})),
                Err(e) => Action::Respond(err(id, e)),
            },
            _ => Action::Respond(err(id, "missing \"pane\", \"cols\" or \"rows\"")),
        },
        "pty_kill" => match need_pane(req)
            .and_then(|pane| core.manager.kill(pane).map_err(|e| e.to_string()))
        {
            Ok(()) => respond(serde_json::json!({"ok": true})),
            Err(e) => Action::Respond(err(id, e)),
        },
        "pty_read" => match need_pane(req) {
            Ok(pane) => match core.manager.screen_text(pane) {
                Some(screen) => respond(serde_json::json!({"ok": true, "screen": screen})),
                None => Action::Respond(err(id, format!("no such pane: {pane}"))),
            },
            Err(e) => Action::Respond(err(id, e)),
        },
        "panes" => respond(serde_json::json!({"ok": true, "panes": core.manager.pane_roots()})),
        "agent_states" => {
            let snapshot = core.statuses.snapshot();
            let states = snapshot.states;
            respond(
                serde_json::json!({"ok": true, "states": states, "revision": snapshot.revision}),
            )
        }
        "rules_reload" => {
            core.statuses.reload_rules();
            respond(serde_json::json!({"ok": true}))
        }
        "snapshot" => {
            let snapshot = core.statuses.snapshot();
            let states = snapshot.states;
            respond(serde_json::json!({
                "ok": true,
                "version": env!("CARGO_PKG_VERSION"),
                "protocol": PROTOCOL_VERSION,
                "panes": core.manager.pane_roots(),
                "states": states,
                "revision": snapshot.revision,
            }))
        }
        "pty_send_text" => {
            let (Some(pane), Some(data)) = (req.pane, req.data.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"data\""));
            };
            // Bracketed paste: the app treats this as one pasted input.
            let pasted = format!("\u{1b}[200~{data}\u{1b}[201~");
            match core.manager.write(pane, &pasted) {
                Ok(()) => respond(serde_json::json!({"ok": true})),
                Err(e) => Action::Respond(err(id, e)),
            }
        }
        "pty_send_keys" => {
            let (Some(pane), Some(keys)) = (req.pane, req.keys.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"keys\""));
            };
            let mut bytes = String::new();
            for key in &keys {
                match key_to_bytes(key) {
                    Ok(seq) => bytes.push_str(&seq),
                    Err(e) => return Action::Respond(err(id, e)),
                }
            }
            match core.manager.write(pane, &bytes) {
                Ok(()) => respond(serde_json::json!({"ok": true})),
                Err(e) => Action::Respond(err(id, e)),
            }
        }
        "agent_prompt" => {
            let (Some(pane), Some(data)) = (req.pane, req.data.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"data\""));
            };
            let submitted = format!("\u{1b}[200~{data}\u{1b}[201~\r");
            match core.manager.write(pane, &submitted) {
                Ok(()) => respond(serde_json::json!({"ok": true})),
                Err(e) => Action::Respond(err(id, e)),
            }
        }
        "wait_state" => {
            let (Some(pane), Some(want)) = (req.pane, req.want.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"want\""));
            };
            if want.is_empty() {
                return Action::Respond(err(id, "\"want\" must not be empty"));
            }
            if !pane_alive(core, pane) {
                return Action::Respond(err(id, format!("no such pane: {pane}")));
            }
            let deadline = std::time::Instant::now() + wait_timeout(req);
            loop {
                if !client_connected(socket) {
                    return Action::Respond(err(id, "client disconnected"));
                }
                if !pane_alive(core, pane) {
                    return Action::Respond(err(id, format!("pane exited: {pane}")));
                }
                let snapshot = core.statuses.snapshot();
                let states = snapshot.states;
                if let Some(status) = states.get(&pane) {
                    if want.iter().any(|w| w == status.state.state_key()) {
                        let identity = status.state.identity();
                        return respond(serde_json::json!({
                            "ok": true, "pane": pane, "state": status.state.state_key(),
                            "agent": identity.map(|i| i.0), "cli": identity.map(|i| i.1),
                            "agentInstanceId": status.agent_instance_id, "reason": status.reason,
                        }));
                    }
                }
                if std::time::Instant::now() >= deadline {
                    return Action::Respond(err(id, "timed out waiting for state"));
                }
                core.statuses.wait_for_revision(
                    snapshot.revision,
                    deadline
                        .saturating_duration_since(std::time::Instant::now())
                        .min(Duration::from_millis(200)),
                );
            }
        }
        "wait_output" => {
            let (Some(pane), Some(needle)) = (req.pane, req.contains.clone()) else {
                return Action::Respond(err(id, "missing \"pane\" or \"contains\""));
            };
            let deadline = std::time::Instant::now() + wait_timeout(req);
            loop {
                if !client_connected(socket) {
                    return Action::Respond(err(id, "client disconnected"));
                }
                match core.manager.screen_text(pane) {
                    Some(screen) if screen.contains(&needle) => {
                        return respond(serde_json::json!({"ok": true, "pane": pane}));
                    }
                    Some(_) => {}
                    None => return Action::Respond(err(id, format!("no such pane: {pane}"))),
                }
                if std::time::Instant::now() >= deadline {
                    return Action::Respond(err(id, "timed out waiting for output"));
                }
                thread::sleep(Duration::from_millis(200));
            }
        }
        "shutdown" => Action::Shutdown(ok(id, serde_json::json!({"ok":true,"shutdown":true}))),
        op => Action::Respond(err(id, format!("unknown op: {op}"))),
    }
}

/// At most MAX_CLIENTS live connections, including unauthenticated handshakes.
pub fn serve(core: Arc<DaemonCore>, listener: TcpListener, auth_token: String) {
    let auth_token = Arc::<str>::from(auth_token);
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if core
                    .connections
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                        (count < MAX_CLIENTS).then_some(count + 1)
                    })
                    .is_err()
                {
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    continue;
                }
                let connection_core = core.clone();
                let auth_token = auth_token.clone();
                let id = core.next_peer.fetch_add(1, Ordering::Relaxed);
                if thread::Builder::new()
                    .name(format!("ubra-daemon-conn-{id}"))
                    .spawn(move || {
                        let _lease = ConnectionLease(connection_core.clone());
                        handle_conn(connection_core, stream, id, auth_token);
                    })
                    .is_err()
                {
                    core.connections.fetch_sub(1, Ordering::AcqRel);
                }
            }
            Err(e) => eprintln!("ubra-daemon: accept failed: {e}"),
        }
    }
}

struct ConnectionLease(Arc<DaemonCore>);
impl Drop for ConnectionLease {
    fn drop(&mut self) {
        self.0.connections.fetch_sub(1, Ordering::AcqRel);
    }
}

fn authenticate_peer(
    writer: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
    expected: &str,
) -> io::Result<()> {
    let deadline = Instant::now() + IO_TIMEOUT;
    let hello: Request = serde_json::from_str(&read_frame(reader, deadline)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let Some(client_nonce) = hello.nonce.as_deref().filter(|n| valid_nonce(n)) else {
        write_frame(writer, &err(hello.id, "unauthorized"), deadline)?;
        return Err(permission_error("unauthorized"));
    };
    if hello.op != "hello" {
        write_frame(writer, &err(hello.id, "unauthorized"), deadline)?;
        return Err(permission_error("unauthorized"));
    }
    let server_nonce = new_auth_token()?;
    write_frame(
        writer,
        &respond(
            hello.id,
            serde_json::json!({
                "protocol":PROTOCOL_VERSION,"nonce":server_nonce,
                "proof":auth_proof(expected,"server",client_nonce,&server_nonce)
            }),
        ),
        deadline,
    )?;
    let auth: Request = serde_json::from_str(&read_frame(reader, deadline)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if auth.op != "auth"
        || auth.id != hello.id
        || !auth.proof.as_deref().is_some_and(|proof| {
            token_matches(
                &auth_proof(expected, "client", client_nonce, &server_nonce),
                proof,
            )
        })
    {
        write_frame(writer, &err(auth.id, "unauthorized"), deadline)?;
        return Err(permission_error("unauthorized"));
    }
    write_frame(
        writer,
        &ok(
            auth.id,
            serde_json::json!({
                "ok":true,"protocol":PROTOCOL_VERSION,"version":env!("CARGO_PKG_VERSION")
            }),
        ),
        deadline,
    )?;
    Ok(())
}

fn handle_conn(core: Arc<DaemonCore>, stream: TcpStream, id: u64, auth_token: Arc<str>) {
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let socket = match stream.try_clone() {
        Ok(s) => Arc::new(s),
        Err(_) => return,
    };
    let mut writer = stream;
    let mut reader = BufReader::new(reader_stream);
    if authenticate_peer(&mut writer, &mut reader, &auth_token).is_err() {
        return;
    }
    let (tx, rx) = mpsc::sync_channel(MAX_QUEUE_MESSAGES);
    let peer = Peer {
        tx,
        socket: socket.clone(),
        bytes: Arc::new(AtomicUsize::new(0)),
    };
    core.peers.lock().insert(id, peer.clone());
    let snapshot = core.statuses.snapshot();
    peer.send(
        serde_json::json!({"event":"agent-states","states":snapshot.states})
            .to_string()
            .into(),
        None,
    );
    peer.send(
        serde_json::json!({"event":"agent-state-update","revision":snapshot.revision,
        "states":snapshot.states,"transitions":[]})
        .to_string()
        .into(),
        None,
    );
    let bytes = peer.bytes.clone();
    let writer_thread = thread::spawn(move || {
        for queued in rx {
            let result = write_frame(&mut writer, &queued.message, Instant::now() + IO_TIMEOUT);
            bytes.fetch_sub(queued.message.len() + 1, Ordering::AcqRel);
            if result.is_err() {
                break;
            }
            if let Some(flushed) = queued.flushed {
                let _ = flushed.try_send(());
            }
        }
        let _ = writer.shutdown(std::net::Shutdown::Both);
    });
    loop {
        // An idle subscription is not a partial frame. Once bytes arrive,
        // the complete request must arrive within the smaller frame deadline.
        if reader
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(300)))
            .is_err()
            || reader.fill_buf().is_err()
        {
            break;
        }
        let line = match read_frame(&mut reader, Instant::now() + FRAME_TIMEOUT) {
            Ok(line) => line,
            Err(_) => break,
        };
        let req: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                if !peer.send(err(None, format!("invalid request: {e}")).into(), None) {
                    break;
                }
                continue;
            }
        };
        match dispatch_connected(&core, &req, Some(&socket)) {
            Action::Respond(message) => {
                if !peer.send(message.into(), None) {
                    break;
                }
            }
            Action::Shutdown(message) => {
                let (flushed, received) = mpsc::sync_channel(1);
                if peer.send(message.into(), Some(flushed)) {
                    let _ = received.recv_timeout(IO_TIMEOUT);
                }
                shutdown_now(&core);
            }
        }
    }
    core.peers.lock().remove(&id);
    let _ = socket.shutdown(std::net::Shutdown::Both);
    drop(peer);
    let _ = writer_thread.join();
}

fn shutdown_now(core: &DaemonCore) -> ! {
    if let Err(error) = core.manager.shutdown() {
        eprintln!("ubra-daemon: pane cleanup failed: {error}");
        remove_runtime_files(&core.state_dir);
        std::process::exit(1);
    }
    remove_runtime_files(&core.state_dir);
    std::process::exit(0);
}

/// Daemon CLI arguments (thin bin parses into this; kept here for tests).
#[derive(Debug, PartialEq, Eq)]
pub struct DaemonArgs {
    pub state_dir: Option<PathBuf>,
    pub rules_dir: Option<PathBuf>,
    pub port: u16,
}

pub fn parse_daemon_args(args: &[String]) -> Result<DaemonArgs, String> {
    let mut out = DaemonArgs {
        state_dir: None,
        rules_dir: None,
        port: 0,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--state-dir" => {
                i += 1;
                out.state_dir = Some(PathBuf::from(
                    args.get(i).ok_or("missing value for --state-dir")?,
                ));
            }
            "--rules-dir" => {
                i += 1;
                out.rules_dir = Some(PathBuf::from(
                    args.get(i).ok_or("missing value for --rules-dir")?,
                ));
            }
            "--port" => {
                i += 1;
                out.port = args
                    .get(i)
                    .ok_or("missing value for --port")?
                    .parse()
                    .map_err(|_| "invalid --port (want 0-65535)")?;
            }
            "--help" | "-h" => return Err("help".to_string()),
            flag => return Err(format!("unknown flag: {flag}")),
        }
        i += 1;
    }
    Ok(out)
}

pub const DAEMON_USAGE: &str =
    "usage: ubra-daemon [--state-dir DIR] [--rules-dir DIR] [--port PORT]";

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn scratch_dir() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("ubra-daemon-test-{}-{id}", std::process::id()))
    }

    fn core() -> Arc<DaemonCore> {
        DaemonCore::new(None, scratch_dir())
    }

    fn req(op: &str) -> Request {
        serde_json::from_str(&format!("{{\"op\":\"{op}\"}}")).unwrap()
    }

    fn responded(action: Action) -> serde_json::Value {
        match action {
            Action::Respond(line) => serde_json::from_str(&line).unwrap(),
            Action::Shutdown(_) => panic!("unexpected shutdown"),
        }
    }

    #[test]
    fn ping_echoes_id() {
        let req: Request = serde_json::from_str("{\"op\":\"ping\",\"id\":7}").unwrap();
        let v = responded(dispatch(&core(), &req));
        assert_eq!(v["ok"], true);
        assert_eq!(v["id"], 7);
        assert_eq!(v["protocol"], PROTOCOL_VERSION);
    }

    #[test]
    fn unknown_op_errors() {
        let v = responded(dispatch(&core(), &req("frobnicate")));
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("frobnicate"));
    }

    #[test]
    fn missing_args_error() {
        let v = responded(dispatch(&core(), &req("pty_write")));
        assert_eq!(v["ok"], false);
        let v = responded(dispatch(&core(), &req("pty_read")));
        assert_eq!(v["ok"], false);
        let v = responded(dispatch(&core(), &req("pty_kill")));
        assert_eq!(v["ok"], false);
    }

    #[test]
    fn spawn_write_read_kill_roundtrip() {
        let core = core();
        let spawn = responded(dispatch(&core, &req("pty_spawn")));
        assert_eq!(spawn["ok"], true);
        let pane = spawn["pane"].as_u64().unwrap() as PaneId;

        let mut write = req("pty_write");
        write.pane = Some(pane);
        write.data = Some("echo marker-abc123\n".to_string());
        assert_eq!(responded(dispatch(&core, &write))["ok"], true);

        let mut read = req("pty_read");
        read.pane = Some(pane);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let screen = responded(dispatch(&core, &read))["screen"]
                .as_str()
                .unwrap()
                .to_string();
            if screen.contains("marker-abc123") {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "screen never showed output: {screen:?}"
            );
            thread::sleep(Duration::from_millis(100));
        }

        let mut kill = req("pty_kill");
        kill.pane = Some(pane);
        assert_eq!(responded(dispatch(&core, &kill))["ok"], true);
        assert_eq!(responded(dispatch(&core, &read))["ok"], false);
    }

    #[test]
    fn panes_and_snapshot_list_spawned_panes() {
        let core = core();
        let spawn = responded(dispatch(&core, &req("pty_spawn")));
        let pane = spawn["pane"].as_u64().unwrap();
        let panes = responded(dispatch(&core, &req("panes")));
        assert!(panes["panes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == pane));
        let snap = responded(dispatch(&core, &req("snapshot")));
        assert_eq!(snap["ok"], true);
        assert!(snap["panes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == pane));
        assert!(snap.get("states").is_some());
    }

    #[test]
    fn shutdown_action_carries_ok() {
        match dispatch(&core(), &req("shutdown")) {
            Action::Shutdown(line) => {
                let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_eq!(v["ok"], true);
            }
            Action::Respond(_) => panic!("shutdown must not merely respond"),
        }
    }

    #[test]
    fn port_file_roundtrip() {
        let dir = scratch_dir();
        assert_eq!(read_port_file(&dir), None);
        write_port_file(&dir, 4567).unwrap();
        assert_eq!(
            read_port_file(&dir),
            Some(PortFile {
                port: 4567,
                pid: std::process::id(),
            })
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn app_data_dir_uses_identifier() {
        let home = Path::new("/home/test");
        let dir = app_data_dir_with(home, None, None);
        assert!(dir.ends_with(APP_IDENTIFIER), "got: {}", dir.display());
        assert!(dir.starts_with(home), "got: {}", dir.display());
        let xdg = app_data_dir_with(home, Some("/xdg/data".to_string()), None);
        assert!(xdg.ends_with(APP_IDENTIFIER), "got: {}", xdg.display());
    }

    #[test]
    fn parse_daemon_args_flags() {
        let args: Vec<String> = [
            "--state-dir",
            "/tmp/s",
            "--rules-dir",
            "/tmp/r",
            "--port",
            "1234",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            parse_daemon_args(&args).unwrap(),
            DaemonArgs {
                state_dir: Some(PathBuf::from("/tmp/s")),
                rules_dir: Some(PathBuf::from("/tmp/r")),
                port: 1234,
            }
        );
        assert_eq!(
            parse_daemon_args(&[]).unwrap(),
            DaemonArgs {
                state_dir: None,
                rules_dir: None,
                port: 0,
            }
        );
        assert!(parse_daemon_args(&["--port".to_string()]).is_err());
        assert!(parse_daemon_args(&["--port".to_string(), "x".to_string()]).is_err());
        assert!(parse_daemon_args(&["--bogus".to_string()]).is_err());
    }

    #[test]
    fn key_encoding_covers_names_ctrl_and_literals() {
        assert_eq!(key_to_bytes("enter").unwrap(), "\r");
        assert_eq!(key_to_bytes("Enter").unwrap(), "\r");
        assert_eq!(key_to_bytes("esc").unwrap(), "\u{1b}");
        assert_eq!(key_to_bytes("tab").unwrap(), "\t");
        assert_eq!(key_to_bytes("up").unwrap(), "\u{1b}[A");
        assert_eq!(key_to_bytes("f5").unwrap(), "\u{1b}[15~");
        assert_eq!(key_to_bytes("ctrl-c").unwrap(), "\u{3}");
        assert_eq!(key_to_bytes("ctrl-Z").unwrap(), "\u{1a}");
        assert_eq!(key_to_bytes("a").unwrap(), "a");
        assert_eq!(key_to_bytes("?").unwrap(), "?");
        assert!(key_to_bytes("f13").is_err());
        assert!(key_to_bytes("ctrl-").is_err());
        assert!(key_to_bytes("ctrl-ab").is_err());
        assert!(key_to_bytes("splat").is_err());
    }

    /// Bracketed-paste markers need a terminal that implements them.
    #[cfg(unix)]
    #[test]
    fn prompt_submits_and_output_arrives() {
        let core = core();
        let spawn = responded(dispatch(&core, &req("pty_spawn")));
        let pane = spawn["pane"].as_u64().unwrap() as PaneId;

        let mut prompt: Request =
            serde_json::from_str("{\"op\":\"agent_prompt\",\"data\":\"echo marker-p6a\"}").unwrap();
        prompt.pane = Some(pane);
        assert_eq!(responded(dispatch(&core, &prompt))["ok"], true);

        let mut wait: Request =
            serde_json::from_str("{\"op\":\"wait_output\",\"contains\":\"marker-p6a\"}").unwrap();
        wait.pane = Some(pane);
        wait.timeout_secs = Some(10);
        let got = responded(dispatch(&core, &wait));
        assert_eq!(got["ok"], true);
        assert_eq!(got["pane"], pane);
    }

    #[test]
    fn send_keys_submits_pending_line() {
        let core = core();
        let spawn = responded(dispatch(&core, &req("pty_spawn")));
        let pane = spawn["pane"].as_u64().unwrap() as PaneId;

        let mut write: Request =
            serde_json::from_str("{\"op\":\"pty_write\",\"data\":\"echo marker-p6b\"}").unwrap();
        write.pane = Some(pane);
        assert_eq!(responded(dispatch(&core, &write))["ok"], true);

        let mut keys: Request =
            serde_json::from_str("{\"op\":\"pty_send_keys\",\"keys\":[\"enter\"]}").unwrap();
        keys.pane = Some(pane);
        assert_eq!(responded(dispatch(&core, &keys))["ok"], true);

        let mut wait: Request =
            serde_json::from_str("{\"op\":\"wait_output\",\"contains\":\"marker-p6b\"}").unwrap();
        wait.pane = Some(pane);
        wait.timeout_secs = Some(10);
        assert_eq!(responded(dispatch(&core, &wait))["ok"], true);
    }

    #[test]
    fn send_keys_rejects_unknown_keys() {
        let core = core();
        let mut keys: Request =
            serde_json::from_str("{\"op\":\"pty_send_keys\",\"keys\":[\"splat\"]}").unwrap();
        keys.pane = Some(1);
        let v = responded(dispatch(&core, &keys));
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("splat"));
    }

    #[test]
    fn wait_state_matches_idle_and_times_out() {
        let core = core();
        let spawn = responded(dispatch(&core, &req("pty_spawn")));
        let pane = spawn["pane"].as_u64().unwrap() as PaneId;

        let mut wait: Request =
            serde_json::from_str("{\"op\":\"wait_state\",\"want\":[\"idle\"]}").unwrap();
        wait.pane = Some(pane);
        wait.timeout_secs = Some(10);
        let got = responded(dispatch(&core, &wait));
        assert_eq!(got["ok"], true);
        assert_eq!(got["state"], "idle");

        let mut miss: Request =
            serde_json::from_str("{\"op\":\"wait_state\",\"want\":[\"working\"]}").unwrap();
        miss.pane = Some(pane);
        miss.timeout_secs = Some(1);
        let v = responded(dispatch(&core, &miss));
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("timed out"));
    }

    #[test]
    fn waits_reject_dead_panes() {
        let core = core();
        let mut wait: Request =
            serde_json::from_str("{\"op\":\"wait_state\",\"want\":[\"idle\"]}").unwrap();
        wait.pane = Some(424242);
        assert_eq!(responded(dispatch(&core, &wait))["ok"], false);
        let mut out: Request =
            serde_json::from_str("{\"op\":\"wait_output\",\"contains\":\"x\"}").unwrap();
        out.pane = Some(424242);
        assert_eq!(responded(dispatch(&core, &out))["ok"], false);
    }
}
