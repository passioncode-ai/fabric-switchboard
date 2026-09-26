use crate::{validate_snapshot, Snapshot};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

const MAX_FILE: u64 = 2 * 1024 * 1024;
const METADATA: &str = "accounts.json";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Disk {
    schema_version: u32,
    snapshot: Snapshot,
}

fn error(_: impl std::fmt::Display) -> String {
    "Private account storage unavailable".into()
}

#[cfg(unix)]
fn private_options() -> OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    options
}

#[cfg(unix)]
fn secure_file(file: &File) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = file.metadata().map_err(error)?;
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err("Unsafe account storage file".into());
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(error)
}

#[cfg(unix)]
fn secure_root(root: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    if !root.is_absolute() {
        return Err("Account storage path must be absolute".into());
    }
    // macOS's /var and /tmp are system symlinks; canonicalize the existing parent.
    // The supplied final directory itself must never be a symlink.
    match fs::symlink_metadata(root) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
        Ok(_) => return Err("Unsafe account storage directory".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(root)
                .map_err(error)?;
        }
        Err(e) => return Err(error(e)),
    }
    let m = fs::symlink_metadata(root).map_err(error)?;
    if !m.is_dir() || m.file_type().is_symlink() || m.uid() != unsafe { libc::geteuid() } {
        return Err("Unsafe account storage directory".into());
    }
    fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(error)
}

#[cfg(unix)]
pub(crate) fn open(root: &Path) -> Result<(File, Snapshot), String> {
    secure_root(root)?;
    let lock = private_options()
        .read(true)
        .write(true)
        .create(true)
        .open(root.join("instance.lock"))
        .map_err(error)?;
    secure_file(&lock)?;
    lock.try_lock_exclusive()
        .map_err(|_| "Another Switchboard instance owns this account storage")?;
    let path = root.join(METADATA);
    let snapshot = match private_options().read(true).open(&path) {
        Ok(file) => {
            secure_file(&file)?;
            if file.metadata().map_err(error)?.len() > MAX_FILE {
                return Err("Account metadata exceeds size limit".into());
            }
            let mut bytes = Vec::new();
            file.take(MAX_FILE + 1)
                .read_to_end(&mut bytes)
                .map_err(error)?;
            if bytes.len() as u64 > MAX_FILE {
                return Err("Account metadata exceeds size limit".into());
            }
            let disk: Disk = serde_json::from_slice(&bytes)
                .map_err(|_| "Account metadata is corrupt; restore a known-good backup")?;
            if disk.schema_version != 1 {
                return Err("Unsupported account metadata version".into());
            }
            validate_snapshot(&disk.snapshot)?;
            disk.snapshot
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Snapshot::default(),
        Err(e) => return Err(error(e)),
    };
    Ok((lock, snapshot))
}

#[cfg(unix)]
pub(crate) fn write(root: &Path, snapshot: &Snapshot) -> Result<(), String> {
    secure_root(root)?;
    let destination = root.join(METADATA);
    match fs::symlink_metadata(&destination) {
        Ok(m) if !m.is_file() || m.file_type().is_symlink() => {
            return Err("Unsafe account metadata file".into())
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(error(e)),
    }
    let data = serde_json::to_vec(&Disk {
        schema_version: 1,
        snapshot: snapshot.clone(),
    })
    .map_err(error)?;
    if data.len() as u64 > MAX_FILE {
        return Err("Account metadata exceeds size limit".into());
    }
    let temporary = root.join(format!(".accounts-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = private_options()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(error)?;
        secure_file(&file)?;
        file.write_all(&data).map_err(error)?;
        file.sync_all().map_err(error)?;
        fs::rename(&temporary, &destination).map_err(error)?;
        // Rename is the publication boundary. Directory sync is best-effort: returning
        // failure after rename would make memory disagree with the visible disk state.
        let _ = File::open(root).and_then(|directory| directory.sync_all());
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(unix))]
pub(crate) fn open(_: &Path) -> Result<(File, Snapshot), String> {
    Err("Secure metadata storage is not implemented on this platform".into())
}
#[cfg(not(unix))]
pub(crate) fn write(_: &Path, _: &Snapshot) -> Result<(), String> {
    Err("Secure metadata storage is not implemented on this platform".into())
}
