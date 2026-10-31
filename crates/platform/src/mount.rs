//! Verify the expected NFS export against mountinfo and the held root descriptor.
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::path::Path;

fn bounded_file(path: &Path) -> io::Result<String> {
    let mut text = String::new();
    File::open(path)?.take(1_048_577).read_to_string(&mut text)?;
    if text.len() > 1_048_576 {
        return Err(io::Error::other("Mount information exceeds limit"));
    }
    Ok(text)
}
fn unescape(text: &str) -> String {
    text.replace("\\040", " ").replace("\\011", "\t").replace("\\012", "\n").replace("\\134", "\\")
}
fn expected_id(info: &str, root: &Path, remote: &str) -> io::Result<u64> {
    for line in info.lines() {
        let Some((left, right)) = line.split_once(" - ") else { continue };
        let fields: Vec<_> = left.split_whitespace().collect();
        let mut right = right.split_whitespace();
        let kind = right.next();
        let source = right.next();
        if fields.get(4).is_some_and(|p| Path::new(&unescape(p)) == root)
            && matches!(kind, Some("nfs" | "nfs4"))
            && source.is_some_and(|s| unescape(s) == remote)
            && fields.get(3) == Some(&"/")
        {
            return fields.first().and_then(|id| id.parse().ok()).ok_or_else(|| io::Error::other("Invalid mount ID"));
        }
    }
    Err(io::Error::other("Expected NFS mount is offline or differs from settings; mount it on this PC before browsing"))
}

/// Refuse a bare local mount directory, another export, a bind subtree, or a replaced mount.
pub fn verify(root: &Path, remote: &str, descriptor: &File) -> io::Result<()> {
    let info = bounded_file(Path::new("/proc/self/mountinfo"))?;
    let id = expected_id(&info, root, remote)?;
    let fdinfo = bounded_file(Path::new(&format!("/proc/self/fdinfo/{}", descriptor.as_raw_fd())))?;
    let held =
        fdinfo.lines().find_map(|line| line.strip_prefix("mnt_id:").and_then(|id| id.trim().parse::<u64>().ok()));
    if held != Some(id) {
        return Err(io::Error::other("NFS mount changed during browsing; retry after checking the mount"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_wrong_or_bind_mounts_fail_closed() {
        let root = Path::new("/mnt/library");
        let good = "42 1 0:4 / /mnt/library ro - nfs4 server:/media ro\n";
        assert_eq!(expected_id(good, root, "server:/media").unwrap(), 42);
        for bad in [
            "",
            "42 1 0:4 / /mnt/library rw - ext4 /dev/fixture rw",
            "42 1 0:4 / /mnt/library ro - nfs4 other:/media ro",
            "42 1 0:4 /sub /mnt/library ro - nfs4 server:/media ro",
        ] {
            assert!(expected_id(bad, root, "server:/media").is_err());
        }
        assert!(expected_id(good, Path::new("/mnt/library/sub"), "server:/media").is_err());
    }
}
