//! App-owned storage primitives. Callers must supply absolute paths within their private root.
//! Windows uses protected current-user DACLs, reparse refusal and native replacement;
//! Unix uses owner checks, no-follow opens and 0700/0600 permissions.
use std::fs::File;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
fn err(_: impl std::fmt::Display) -> String {
    "Private account storage unavailable".into()
}

#[cfg(windows)]
use crate::windows::open_private_strict;
#[cfg(windows)]
pub use crate::windows::{check_path, create_new, open_private, private_dir, replace};

#[cfg(unix)]
pub fn check_path(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let m = fs::symlink_metadata(path).map_err(err)?;
    if m.file_type().is_symlink()
        || m.uid() != unsafe { libc::geteuid() }
        || (m.is_file() && m.nlink() != 1)
    {
        return Err("Unsafe private storage path".into());
    }
    Ok(())
}
#[cfg(unix)]
pub fn private_dir(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    if !path.is_absolute() {
        return Err("Account storage path must be absolute".into());
    }
    if !path.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(err)?;
    }
    check_path(path)?;
    if !path.is_dir() {
        return Err("Unsafe private storage directory".into());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(err)
}
#[cfg(unix)]
fn options() -> OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    let mut o = OpenOptions::new();
    o.mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    o
}
#[cfg(unix)]
pub fn create_new(path: &Path) -> Result<File, String> {
    options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(err)
}
/// How many times an open starts again when the file it reached was replaced in between.
#[cfg(unix)]
const REPLACED_RETRIES: usize = 8;
/// Opens `path` and checks the file it reached: a regular file of this user with one link.
/// A file with no link left was replaced by `private_write`'s rename after this open reached it
/// (SB-320: a reader polling a launch record while its run ended read "unsafe" and got none), so
/// the open starts again on the file now there; a second link (a hard link) is still refused.
#[cfg(unix)]
fn open_checked(path: &Path, write: bool) -> Result<(File, fs::Metadata), String> {
    use std::os::unix::fs::MetadataExt;
    for _ in 0..REPLACED_RETRIES {
        let f = options().read(true).write(write).open(path).map_err(err)?;
        let m = f.metadata().map_err(err)?;
        if m.nlink() == 0 {
            continue;
        }
        if !m.is_file() || m.nlink() != 1 || m.uid() != unsafe { libc::geteuid() } {
            return Err("Unsafe private storage file".into());
        }
        return Ok((f, m));
    }
    Err("Unsafe private storage file".into())
}
#[cfg(unix)]
pub fn open_private(path: &Path, write: bool) -> Result<File, String> {
    use std::os::unix::fs::PermissionsExt;
    let (f, _) = open_checked(path, write)?;
    f.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(err)?;
    Ok(f)
}
#[cfg(unix)]
fn open_private_strict(path: &Path) -> Result<File, String> {
    use std::os::unix::fs::PermissionsExt;
    let (file, meta) = open_checked(path, false)?;
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(
            "Private file permissions are unsafe; rotate its capability before reuse".into(),
        );
    }
    Ok(file)
}
#[cfg(unix)]
pub fn replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(err)
}

pub fn read_private(path: &Path, max_bytes: u64) -> Result<Vec<u8>, String> {
    read_bounded(open_private(path, false)?, max_bytes)
}
/// Verify current private permissions on the same opened handle, without fixing them.
/// Use this for capability descriptors: narrowing an exposed ACL cannot revoke a leaked token.
pub fn read_private_strict(path: &Path, max_bytes: u64) -> Result<Vec<u8>, String> {
    read_bounded(open_private_strict(path)?, max_bytes)
}
fn read_bounded(f: File, max_bytes: u64) -> Result<Vec<u8>, String> {
    if f.metadata().map_err(err)?.len() > max_bytes {
        return Err("Private file exceeds size limit".into());
    }
    let mut bytes = Vec::new();
    f.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > max_bytes {
        return Err("Private file exceeds size limit".into());
    }
    Ok(bytes)
}
pub fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Private file has no parent")?;
    private_dir(parent)?;
    if fs::symlink_metadata(path).is_ok() {
        check_path(path)?;
        if !path.is_file() {
            return Err("Unsafe private storage file".into());
        }
    }
    let temporary = parent.join(format!(".private-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut f = create_new(&temporary)?;
        f.write_all(bytes).map_err(err)?;
        f.sync_all().map_err(err)?;
        drop(f);
        replace(&temporary, path)?;
        #[cfg(unix)]
        {
            let _ = File::open(parent).and_then(|f| f.sync_all());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn a_read_racing_a_replace_reads_one_whole_version() {
        // SB-320: a file replaced between a reader's open and its check had no link left and
        // read as unsafe. Every read here must succeed and see one whole version.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("record.json");
        private_write(&path, b"version-0000").unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer = {
            let (path, stop) = (path.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut n = 0u32;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    n = (n + 1) % 10_000;
                    private_write(&path, format!("version-{n:04}").as_bytes()).unwrap();
                }
            })
        };
        let (mut reads, mut strict) = (0, 0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            let bytes = read_private(&path, 64).unwrap();
            assert!(bytes.starts_with(b"version-") && bytes.len() == 12, "{bytes:?}");
            reads += 1;
            read_private_strict(&path, 64).unwrap();
            strict += 1;
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        writer.join().unwrap();
        assert!(reads > 100 && strict > 100);
    }
    #[test]
    fn a_hard_linked_file_is_still_refused() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("record.json");
        private_write(&path, b"x").unwrap();
        fs::hard_link(&path, temp.path().join("other")).unwrap();
        assert!(read_private(&path, 8).is_err());
        assert!(read_private_strict(&path, 8).is_err());
    }
    #[test]
    fn strict_read_refuses_exposed_capability_without_changing_file() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("control.json");
        private_write(&path, b"synthetic capability").unwrap();
        assert_eq!(
            read_private_strict(&path, 128).unwrap(),
            b"synthetic capability"
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_private_strict(&path, 128)
            .unwrap_err()
            .contains("permissions are unsafe"));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert_eq!(fs::read(&path).unwrap(), b"synthetic capability");
    }
}
