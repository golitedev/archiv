//! Bounded, read-only SFTP v3 over the system OpenSSH client.
//! Only INIT, LSTAT, OPENDIR, READDIR and CLOSE are sent; no file-data or mutation requests.
use crate::{BATCH_SIZE, MAX_ENTRIES, ReadRequest, ReadSummary};
use archiv_core::{Entry, EntryId, EntryKind};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::{
    ffi::{OsStrExt, OsStringExt},
    net::UnixStream,
};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

const MAX_PACKET: usize = 262_144;
const MAX_STRING: usize = 16_384;
const TIMEOUT: Duration = Duration::from_secs(30);
fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}

/// Structured OpenSSH invocation; remote filenames are sent only as binary SFTP strings.
fn command(connection: &archiv_core::sftp::SftpConnection) -> io::Result<Command> {
    connection.validate().map_err(|e| invalid(&e))?;
    let mut command = Command::new("ssh");
    command.args([
        "-T",
        "-a",
        "-x",
        "-oBatchMode=yes",
        "-oStrictHostKeyChecking=yes",
        "-oConnectTimeout=10",
        "-oServerAliveInterval=5",
        "-oServerAliveCountMax=2",
        "-oClearAllForwardings=yes",
        "-oControlMaster=no",
        "-oControlPath=none",
        "-oPermitLocalCommand=no",
        "-oRemoteCommand=none",
        "-oRequestTTY=no",
        "-oForkAfterAuthentication=no",
        "-oStdinNull=no",
    ]);
    if !connection.user.is_empty() {
        command.arg("-l").arg(&connection.user);
    }
    if let Some(port) = connection.port {
        command.arg("-p").arg(port.to_string());
    }
    command.args(["-s", "--", &connection.host, "sftp"]);
    Ok(command)
}
struct Session<'a> {
    stream: UnixStream,
    child: Child,
    cancel: &'a AtomicBool,
    deadline: Instant,
    next_id: u32,
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        // Every exit (including parsing failure/cancellation) terminates and reaps this client.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl<'a> Session<'a> {
    fn start(mut command: Command, cancel: &'a AtomicBool) -> io::Result<Self> {
        let (stream, child_stream) = UnixStream::pair()?;
        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
        stream.set_write_timeout(Some(Duration::from_millis(100)))?;
        let input: OwnedFd = child_stream.try_clone()?.into();
        let output: OwnedFd = child_stream.into();
        let child = command
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output))
            .stderr(Stdio::null())
            .env("SSH_ASKPASS_REQUIRE", "never")
            .spawn()
            .map_err(|e| invalid(&format!("Cannot start OpenSSH (install the ssh client): {e}")))?;
        Ok(Self { stream, child, cancel, deadline: Instant::now() + TIMEOUT, next_id: 0 })
    }
    fn check(&self) -> io::Result<()> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "SFTP browsing cancelled"));
        }
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "SFTP directory request exceeded 30 seconds"));
        }
        Ok(())
    }
    fn read_exact(&mut self, mut bytes: &mut [u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            self.check()?;
            match self.stream.read(bytes) {
                Ok(0) => {
                    return Err(invalid("SFTP connection closed"));
                }
                Ok(n) => {
                    bytes = bytes.get_mut(n..).ok_or_else(|| invalid("Invalid read length"))?;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        let size = u32::try_from(bytes.len()).map_err(|_| invalid("SFTP request too large"))?;
        if bytes.len() > MAX_PACKET {
            return Err(invalid("SFTP request too large"));
        }
        let packet = [size.to_be_bytes().as_slice(), bytes].concat();
        let mut rest = packet.as_slice();
        while !rest.is_empty() {
            self.check()?;
            match self.stream.write(rest) {
                Ok(0) => return Err(invalid("SFTP connection closed while sending")),
                Ok(n) => rest = rest.get(n..).ok_or_else(|| invalid("Invalid write length"))?,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn receive(&mut self) -> io::Result<Vec<u8>> {
        let mut length = [0; 4];
        self.read_exact(&mut length)?;
        let length = u32::from_be_bytes(length) as usize;
        if !(1..=MAX_PACKET).contains(&length) {
            return Err(invalid("SFTP response exceeds packet limit or is empty"));
        }
        let mut packet = vec![0; length];
        self.read_exact(&mut packet)?;
        Ok(packet)
    }
    fn initialize(&mut self) -> io::Result<()> {
        self.send(&[1, 0, 0, 0, 3])?;
        let packet = self.receive()?;
        let mut data = Decoder(&packet);
        if data.byte()? != 2 || data.u32()? != 3 {
            return Err(invalid("Server does not support SFTP version 3"));
        }
        while !data.0.is_empty() {
            data.string()?;
            data.string()?;
        }
        Ok(())
    }
    fn request(&mut self, kind: u8, argument: &[u8]) -> io::Result<Vec<u8>> {
        self.next_id = self.next_id.checked_add(1).ok_or_else(|| invalid("SFTP request limit"))?;
        let mut packet = vec![kind];
        packet.extend(self.next_id.to_be_bytes());
        string(&mut packet, argument)?;
        self.send(&packet)?;
        let reply = self.receive()?;
        let mut data = Decoder(&reply);
        data.byte()?;
        if data.u32()? != self.next_id {
            return Err(invalid("SFTP response identity mismatch"));
        }
        Ok(reply)
    }
    fn directory(&mut self, path: &Path) -> io::Result<()> {
        let packet = self.request(7, path.as_os_str().as_bytes())?;
        let mut data = response(&packet, 105)?;
        let attrs = attributes(&mut data)?;
        data.end()?;
        if attrs.mode & 0o170000 != 0o040000 {
            return Err(invalid("SFTP navigation refuses symbolic links and non-directories"));
        }
        Ok(())
    }
}
fn string(target: &mut Vec<u8>, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() > MAX_STRING {
        return Err(invalid("SFTP string exceeds limit"));
    }
    let len = u32::try_from(bytes.len()).map_err(|_| invalid("SFTP string exceeds limit"))?;
    target.extend(len.to_be_bytes());
    target.extend(bytes);
    Ok(())
}
struct Decoder<'a>(&'a [u8]);
impl<'a> Decoder<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let (value, rest) = self.0.split_at_checked(n).ok_or_else(|| invalid("Truncated SFTP response"))?;
        self.0 = rest;
        Ok(value)
    }
    fn byte(&mut self) -> io::Result<u8> {
        self.take(1)?.first().copied().ok_or_else(|| invalid("Missing SFTP type"))
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().map_err(|_| invalid("Invalid integer"))?))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().map_err(|_| invalid("Invalid integer"))?))
    }
    fn string(&mut self) -> io::Result<&'a [u8]> {
        let length = self.u32()? as usize;
        if length > MAX_STRING {
            return Err(invalid("SFTP string exceeds limit"));
        }
        self.take(length)
    }
    fn end(&self) -> io::Result<()> {
        if self.0.is_empty() { Ok(()) } else { Err(invalid("Unexpected SFTP response data")) }
    }
}
fn status(data: &mut Decoder<'_>) -> io::Result<u32> {
    let code = data.u32()?;
    data.string()?;
    data.string()?;
    data.end()?;
    Ok(code)
}
fn response(packet: &[u8], expected: u8) -> io::Result<Decoder<'_>> {
    let mut data = Decoder(packet);
    let kind = data.byte()?;
    data.u32()?;
    if kind == 101 {
        let code = status(&mut data)?;
        return Err(invalid(&format!(
            "SFTP server refused request (status {code}); check remote path and permissions"
        )));
    }
    if kind != expected {
        return Err(invalid("Unexpected SFTP response type"));
    }
    Ok(data)
}
#[derive(Default)]
struct Attributes {
    size: Option<u64>,
    mode: u32,
    modified: Option<std::time::SystemTime>,
}
fn attributes(data: &mut Decoder<'_>) -> io::Result<Attributes> {
    let flags = data.u32()?;
    if flags & !0x8000000f != 0 {
        return Err(invalid("Unsupported SFTP attributes"));
    }
    let mut attrs = Attributes::default();
    if flags & 1 != 0 {
        attrs.size = Some(data.u64()?);
    }
    if flags & 2 != 0 {
        data.u32()?;
        data.u32()?;
    }
    if flags & 4 != 0 {
        attrs.mode = data.u32()?;
    }
    if flags & 8 != 0 {
        data.u32()?;
        attrs.modified = UNIX_EPOCH.checked_add(Duration::from_secs(u64::from(data.u32()?)));
    }
    if flags & 0x80000000 != 0 {
        let count = data.u32()?;
        if count > 128 {
            return Err(invalid("Too many SFTP attribute extensions"));
        }
        for _ in 0..count {
            data.string()?;
            data.string()?;
        }
    }
    Ok(attrs)
}
fn browse(
    session: &mut Session<'_>,
    request: &ReadRequest,
    emit: &mut dyn FnMut(Vec<Entry>) -> bool,
) -> io::Result<ReadSummary> {
    session.initialize()?;
    let physical = request.source.root.join(request.path.as_path());
    if !physical.is_absolute() || physical.components().count() > 64 || physical.as_os_str().as_bytes().contains(&0) {
        return Err(invalid("SFTP requires an absolute remote folder with depth at most 64"));
    }
    // Observe every ancestor without following its final component. SFTP v3 has no openat2:
    // this is a read-only navigation guard, not a server-side confinement guarantee.
    let mut ancestor = PathBuf::from("/");
    for part in physical.components() {
        match part {
            std::path::Component::RootDir => {}
            std::path::Component::Normal(name) => {
                ancestor.push(name);
                session.directory(&ancestor)?;
            }
            _ => return Err(invalid("SFTP folder contains traversal")),
        }
    }
    let packet = session.request(11, physical.as_os_str().as_bytes())?;
    let mut data = response(&packet, 102)?;
    let handle = data.string()?.to_vec();
    data.end()?;
    let mut summary = ReadSummary::default();
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    let mut seen = std::collections::HashSet::new();
    let mut rounds = 0usize;
    loop {
        session.check()?;
        rounds = rounds.saturating_add(1);
        if rounds > MAX_ENTRIES {
            return Err(invalid("SFTP directory response limit exceeded"));
        }
        let packet = session.request(12, &handle)?;
        let mut data = Decoder(&packet);
        let kind = data.byte()?;
        data.u32()?;
        if kind == 101 {
            if status(&mut data)? == 1 {
                break;
            }
            return Err(invalid("SFTP directory read failed; results are incomplete"));
        }
        if kind != 104 {
            return Err(invalid("Expected SFTP directory entries"));
        }
        let count = data.u32()?;
        if count == 0 || count > 4096 {
            return Err(invalid("Invalid SFTP directory batch count"));
        }
        for _ in 0..count {
            let bytes = data.string()?;
            let name = OsString::from_vec(bytes.to_vec());
            data.string()?;
            let attrs = attributes(&mut data)?;
            if bytes == b"." || bytes == b".." {
                continue;
            }
            if bytes.is_empty() || bytes.len() > 255 || bytes.contains(&0) || bytes.contains(&b'/') {
                return Err(invalid("Invalid remote directory filename"));
            }
            let path = request.path.child(&name).map_err(|e| invalid(&e.to_string()))?;
            if !seen.insert(name.clone()) {
                return Err(invalid("Duplicate SFTP directory entry; refresh this folder"));
            }
            let kind = match attrs.mode & 0o170000 {
                0o040000 => EntryKind::Directory,
                0o100000 => EntryKind::File,
                0o120000 => EntryKind::Symlink,
                _ => EntryKind::Other,
            };
            batch.push(Entry {
                id: EntryId { source: request.source.id, path },
                name,
                kind,
                bytes: if kind == EntryKind::File { attrs.size } else { None },
                modified: attrs.modified,
                mode: attrs.mode,
                identity: Default::default(),
            });
            summary.entries = summary.entries.saturating_add(1);
            if batch.len() == BATCH_SIZE && !emit(std::mem::replace(&mut batch, Vec::with_capacity(BATCH_SIZE))) {
                summary.cancelled = true;
                return Ok(summary);
            }
            if summary.entries >= MAX_ENTRIES {
                summary.truncated = true;
                break;
            }
        }
        if summary.truncated {
            break;
        }
        data.end()?;
    }
    if !batch.is_empty() && !emit(batch) {
        summary.cancelled = true;
    }
    let packet = session.request(4, &handle)?;
    let mut data = Decoder(&packet);
    if data.byte()? != 101 {
        return Err(invalid("Expected SFTP close status"));
    }
    data.u32()?;
    if status(&mut data)? != 0 {
        return Err(invalid("SFTP directory close failed"));
    }
    Ok(summary)
}
pub(crate) fn read(
    request: &ReadRequest,
    cancelled: &AtomicBool,
    emit: &mut dyn FnMut(Vec<Entry>) -> bool,
) -> io::Result<ReadSummary> {
    if cancelled.load(Ordering::Acquire) {
        return Ok(ReadSummary { cancelled: true, ..Default::default() });
    }
    let connection = request.source.sftp.as_ref().ok_or_else(|| invalid("Missing SFTP connection settings"))?;
    let mut session = Session::start(command(connection)?, cancelled)?;
    let result = browse(&mut session, request, emit);
    if cancelled.load(Ordering::Acquire) { Ok(ReadSummary { cancelled: true, ..Default::default() }) } else { result }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archiv_core::{RelativePath, Source, SourceId, sftp::SftpConnection};
    fn connection() -> SftpConnection {
        SftpConnection { host: "synthetic-host".into(), user: "".into(), port: None }
    }
    fn request(root: &Path) -> ReadRequest {
        ReadRequest {
            source: Source {
                id: SourceId(17),
                root: root.to_owned(),
                label: "Synthetic SFTP".into(),
                nfs_remote: None,
                sftp: Some(connection()),
            },
            path: RelativePath::default(),
        }
    }
    fn server(cancel: &AtomicBool) -> Session<'_> {
        // OpenSSH's actual read-only SFTP subsystem, over inherited sockets; no network listener,
        // SSH credentials, user configuration or personal files are involved.
        let program = ["/usr/lib/openssh/sftp-server", "/usr/lib/ssh/sftp-server", "/usr/libexec/sftp-server"]
            .into_iter()
            .find(|path| Path::new(path).is_file())
            .expect("Install the OpenSSH SFTP server for protocol integration tests");
        let mut command = Command::new(program);
        command.arg("-R");
        Session::start(command, cancel).unwrap()
    }
    #[test]
    fn real_sftp_server_lists_metadata_bytes_folders_and_refuses_symlink_navigation() {
        let fixture = tempfile::tempdir().unwrap();
        let nested = fixture.path().join("nested space");
        std::fs::create_dir(&nested).unwrap();
        let odd = OsString::from_vec(vec![b'x', 0xff]);
        std::fs::write(nested.join(&odd), b"safe").unwrap();
        std::fs::write(fixture.path().join("literal;$(name).txt"), b"unchanged").unwrap();
        std::os::unix::fs::symlink(&nested, fixture.path().join("link")).unwrap();
        let cancel = AtomicBool::new(false);
        let mut entries = Vec::new();
        let summary = browse(&mut server(&cancel), &request(fixture.path()), &mut |batch| {
            entries.extend(batch);
            true
        })
        .unwrap();
        assert_eq!(summary.entries, 3);
        assert!(entries.iter().all(|e| e.id.source == SourceId(17)));
        assert_eq!(entries.iter().find(|e| e.name == "link").unwrap().kind, EntryKind::Symlink);
        let file = entries.iter().find(|e| e.name == "literal;$(name).txt").unwrap();
        assert_eq!(file.bytes, Some(9));
        assert!(file.modified.is_some());
        let mut req = request(fixture.path());
        req.path = RelativePath::new("nested space").unwrap();
        entries.clear();
        browse(&mut server(&cancel), &req, &mut |batch| {
            entries.extend(batch);
            true
        })
        .unwrap();
        assert_eq!(entries[0].name, odd);
        req.path = RelativePath::new("link").unwrap();
        assert!(browse(&mut server(&cancel), &req, &mut |_| panic!("link emitted entries")).is_err());
        assert_eq!(std::fs::read(file_path(fixture.path())).unwrap(), b"unchanged");
    }
    fn file_path(root: &Path) -> PathBuf {
        root.join("literal;$(name).txt")
    }
    #[test]
    fn real_server_batches_and_callback_cancellation_are_bounded() {
        let fixture = tempfile::tempdir().unwrap();
        for i in 0..300 {
            std::fs::write(fixture.path().join(format!("file-{i}")), b"safe").unwrap();
        }
        let cancel = AtomicBool::new(false);
        let mut batches = 0;
        let summary = browse(&mut server(&cancel), &request(fixture.path()), &mut |batch| {
            batches += 1;
            assert_eq!(batch.len(), BATCH_SIZE);
            false
        })
        .unwrap();
        assert!(summary.cancelled);
        assert_eq!(batches, 1);
        let summary =
            read(&request(fixture.path()), &AtomicBool::new(true), &mut |_| panic!("cancelled request emitted"))
                .unwrap();
        assert!(summary.cancelled);
    }
    #[test]
    fn ssh_arguments_require_verified_hosts_and_never_include_remote_paths() {
        let mut connection = connection();
        connection.user = "reader".into();
        connection.port = Some(2222);
        let command = command(&connection).unwrap();
        let args: Vec<_> = command.get_args().map(|v| v.to_string_lossy().into_owned()).collect();
        assert!(args.contains(&"-oStrictHostKeyChecking=yes".into()));
        assert!(args.contains(&"-oBatchMode=yes".into()));
        assert!(args.contains(&"-oClearAllForwardings=yes".into()));
        assert_eq!(&args[args.len() - 4..], ["-s", "--", "synthetic-host", "sftp"]);
    }
    #[test]
    fn cancellation_interrupts_a_waiting_protocol_read() {
        let cancel = AtomicBool::new(false);
        let mut session = server(&cancel);
        let started = Instant::now();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(20));
                cancel.store(true, Ordering::Release);
            });
            assert_eq!(session.receive().unwrap_err().kind(), io::ErrorKind::Interrupted);
        });
        assert!(started.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn malformed_packets_and_attribute_lengths_fail_closed() {
        for bytes in [vec![], vec![0], vec![0, 0, 0], vec![0xff; 4], vec![0, 0, 0, 5, b'a']] {
            assert!(Decoder(&bytes).string().is_err());
        }
        assert!(attributes(&mut Decoder(&[0, 0, 0, 16])).is_err());
        assert!(attributes(&mut Decoder(&[0, 0, 0, 1, 0])).is_err());
        assert!(response(&[104, 0, 0, 0, 1], 105).is_err());
    }
    #[test]
    fn oversized_frames_deadlines_and_cancel_reap_the_child() {
        let cancel = AtomicBool::new(false);
        let mut session = server(&cancel);
        session.deadline = Instant::now();
        assert!(session.receive().is_err());
        let pid = session.child.id();
        drop(session);
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        let mut session = server(&cancel);
        let fake = session.stream.try_clone().unwrap();
        // A dedicated socket pair lets the real framing reader see an oversized server header.
        let (client, mut peer) = UnixStream::pair().unwrap();
        session.stream = client;
        peer.write_all(&u32::MAX.to_be_bytes()).unwrap();
        assert!(session.receive().unwrap_err().to_string().contains("packet limit"));
        drop(fake);
        cancel.store(true, Ordering::Release);
        assert!(session.check().is_err());
    }
}
