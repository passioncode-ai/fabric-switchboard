//! Windows native boundaries. No plaintext vault fallback or machine-wide DPAPI scope.
use std::{
    ffi::c_void,
    fs::{self, File},
    io,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, Cryptography::*, *},
    Storage::FileSystem::*,
    System::Threading::*,
};
fn error(_: impl std::fmt::Display) -> String {
    "Windows private storage unavailable".into()
}
fn wide(p: &Path) -> Result<Vec<u16>, String> {
    let mut v: Vec<_> = p.as_os_str().encode_wide().collect();
    if v.contains(&0) {
        return Err("Invalid private storage path".into());
    }
    v.push(0);
    Ok(v)
}
struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn user_sid() -> Result<String, String> {
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(error(io::Error::last_os_error()));
        }
        let token = Handle(token);
        let mut needed = 0;
        GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed);
        let mut data = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
        if GetTokenInformation(
            token.0,
            TokenUser,
            data.as_mut_ptr().cast(),
            needed,
            &mut needed,
        ) == 0
        {
            return Err(error("token"));
        }
        let user = &*(data.as_ptr().cast::<TOKEN_USER>());
        let mut text = null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return Err(error("sid"));
        }
        let _text = Local(text.cast());
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(text, len)).map_err(error)
    }
}
fn descriptor() -> Result<Local, String> {
    let s = format!("O:{}D:P(A;OICI;FA;;;{})", user_sid()?, user_sid()?);
    let wide: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
    let mut sd = null_mut();
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            1,
            &mut sd,
            null_mut(),
        ) == 0
        {
            return Err(error("descriptor"));
        }
    }
    Ok(Local(sd))
}
/// Reject reparse points in every existing path component (including junctions).
pub fn check_path(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    if !path.is_absolute() {
        return Err("Account storage path must be absolute".into());
    }
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(m) if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 => {
                return Err("Unsafe Windows reparse point".into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(error(e)),
        }
    }
    Ok(())
}
fn protect(path: &Path) -> Result<(), String> {
    unsafe {
        let path = wide(path)?;
        let mut existing = null_mut();
        let mut owner = null_mut();
        if GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut existing,
        ) != 0
        {
            return Err(error("owner"));
        }
        let _existing = Local(existing);
        let sd = descriptor()?;
        let mut expected = null_mut();
        let mut defaulted = 0;
        if GetSecurityDescriptorOwner(sd.0, &mut expected, &mut defaulted) == 0
            || EqualSid(owner, expected) == 0
        {
            return Err("Private storage belongs to another Windows user".into());
        }
        let mut dacl = null_mut();
        let mut present = 0;
        if GetSecurityDescriptorDacl(sd.0, &mut present, &mut dacl, &mut defaulted) == 0
            || present == 0
            || dacl.is_null()
        {
            return Err(error("dacl"));
        }
        if SetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            dacl,
            null(),
        ) != 0
        {
            return Err(error("protect"));
        }
        Ok(())
    }
}
pub fn private_dir(path: &Path) -> Result<(), String> {
    check_path(path)?;
    if !path.exists() {
        let parent = path.parent().ok_or("Private directory has no parent")?;
        if !parent.exists() {
            private_dir(parent)?;
        }
        let sd = descriptor()?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: 0,
        };
        if unsafe { CreateDirectoryW(wide(path)?.as_ptr(), &sa) } == 0 && !path.is_dir() {
            return Err(error(io::Error::last_os_error()));
        }
    }
    check_path(path)?;
    if !path.is_dir() {
        return Err("Unsafe private storage directory".into());
    }
    protect(path)
}
fn file(path: &Path, create: bool, write: bool) -> Result<File, String> {
    check_path(path)?;
    let sd = descriptor()?;
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    let handle = unsafe {
        CreateFileW(
            wide(path)?.as_ptr(),
            GENERIC_READ | if write { GENERIC_WRITE } else { 0 },
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            &sa,
            if create { CREATE_NEW } else { OPEN_EXISTING },
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(error(io::Error::last_os_error()));
    }
    let f = unsafe { File::from_raw_handle(handle) };
    let mut info = unsafe { zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    if unsafe { GetFileInformationByHandle(f.as_raw_handle(), &mut info) } == 0
        || info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || info.nNumberOfLinks != 1
    {
        return Err("Unsafe Windows private file".into());
    }
    protect(path)?;
    Ok(f)
}
pub fn create_new(path: &Path) -> Result<File, String> {
    file(path, true, true)
}
pub fn open_private(path: &Path, write: bool) -> Result<File, String> {
    file(path, false, write)
}
pub fn replace(source: &Path, destination: &Path) -> Result<(), String> {
    check_path(source)?;
    check_path(destination)?;
    if source.parent() != destination.parent() {
        return Err("Private replacement must stay in one directory".into());
    }
    // No delete-before-rename gap. Sharing violations leave the old destination intact.
    if unsafe {
        MoveFileExW(
            wide(source)?.as_ptr(),
            wide(destination)?.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(error(io::Error::last_os_error()));
    }
    Ok(())
}
pub(crate) fn vault_path(id: &str) -> Result<PathBuf, String> {
    if !crate::uuid_valid(id) {
        return Err("Invalid credential identifier".into());
    }
    let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA unavailable")?)
        .join("ai.passioncode.fabric-switchboard")
        .join("vault");
    private_dir(&root)?;
    Ok(root.join(format!("{id}.dpapi")))
}
pub(crate) fn crypt(data: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    if data.len() > 128 * 1024 {
        return Err("Stored credential is too large".into());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = unsafe { zeroed::<CRYPT_INTEGER_BLOB>() };
    let okay = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                null(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if okay == 0 {
        return Err("Native credential storage unavailable".into());
    }
    let _allocation = Local(output.pbData.cast());
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    // Clear native decrypted allocation before LocalFree; Rust credential buffers retain normal lifetime.
    if !encrypt {
        for i in 0..output.cbData as usize {
            unsafe {
                std::ptr::write_volatile(output.pbData.add(i), 0);
            }
        }
    }
    Ok(result)
}
/// True only for the same living process incarnation; timestamp is Windows UTC FILETIME.
pub fn process_matches(pid: u32, created: u64) -> Result<bool, String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return if GetLastError() == ERROR_INVALID_PARAMETER {
                Ok(false)
            } else {
                Err("Session process cannot be inspected".into())
            };
        }
        let h = Handle(h);
        let (mut start, mut exit, mut kernel, mut user) = (zeroed(), zeroed(), zeroed(), zeroed());
        if GetProcessTimes(h.0, &mut start, &mut exit, &mut kernel, &mut user) == 0 {
            return Err("Session process cannot be inspected".into());
        }
        let mut code = 0;
        if GetExitCodeProcess(h.0, &mut code) == 0 {
            return Err("Session process cannot be inspected".into());
        }
        Ok(code == 259
            && ((start.dwHighDateTime as u64) << 32 | start.dwLowDateTime as u64) == created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{private_fs, AuthKind, Credential, NativeVault, Provider, Vault};
    #[test]
    fn windows_dpapi_roundtrip_large_oauth_and_tamper_refusal() {
        let id = uuid::Uuid::new_v4().to_string();
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = NativeVault::new().delete(&self.0);
            }
        }
        let _cleanup = Cleanup(id.clone());
        let vault = NativeVault::new();
        let material=serde_json::json!({"tokens":{"access_token":"a".repeat(16384),"refresh_token":"r".repeat(16384),"id_token":"i".repeat(16384),"account_id":"synthetic"}}).to_string();
        let credential = Credential::parse(Provider::Codex, AuthKind::OAuth, &material).unwrap();
        vault.put(&id, &credential).unwrap();
        assert_eq!(
            vault.get(&id).unwrap().access_token,
            credential.access_token
        );
        let path = vault_path(&id).unwrap();
        let encrypted = fs::read(&path).unwrap();
        assert!(!encrypted
            .windows(128)
            .any(|w| w == &credential.access_token.as_bytes()[..128]));
        private_fs::private_write(&path, b"corrupt synthetic ciphertext").unwrap();
        assert!(vault.get(&id).is_err());
        vault.delete(&id).unwrap();
        vault.delete(&id).unwrap();
    }
    #[test]
    fn windows_private_acl_has_only_current_user_and_no_inheritance() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("private");
        private_dir(&root).unwrap();
        let path = root.join("file");
        private_fs::private_write(&path, b"synthetic").unwrap();
        unsafe {
            let mut sd = null_mut();
            let mut dacl = null_mut();
            let mut owner = null_mut();
            assert_eq!(
                GetNamedSecurityInfoW(
                    wide(&path).unwrap().as_ptr(),
                    SE_FILE_OBJECT,
                    OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                    &mut owner,
                    null_mut(),
                    &mut dacl,
                    null_mut(),
                    &mut sd
                ),
                0
            );
            let _sd = Local(sd);
            let mut control = 0;
            let mut revision = 0;
            assert_ne!(
                GetSecurityDescriptorControl(sd, &mut control, &mut revision),
                0
            );
            assert_ne!(control & SE_DACL_PROTECTED, 0);
            assert_eq!((*dacl).AceCount, 1);
            let mut ace = null_mut();
            assert_ne!(GetAce(dacl, 0, &mut ace), 0);
            let ace = &*(ace as *const ACCESS_ALLOWED_ACE);
            assert_eq!(ace.Header.AceType, 0 /* ACCESS_ALLOWED_ACE_TYPE */);
            assert_eq!(ace.Header.AceFlags & INHERITED_ACE as u8, 0);
            assert_ne!(
                EqualSid(owner, (&ace.SidStart as *const u32) as *mut c_void),
                0
            );
        }
        let hardlink = root.join("hardlink");
        fs::hard_link(&path, &hardlink).unwrap();
        assert!(private_fs::read_private(&path, 64).is_err());
    }
    #[test]
    fn windows_failed_replacement_preserves_old_bytes() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("private");
        private_dir(&root).unwrap();
        let path = root.join("file");
        private_fs::private_write(&path, b"old").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&path)
            .unwrap();
        assert!(private_fs::private_write(&path, b"new").is_err());
        drop(held);
        assert_eq!(private_fs::read_private(&path, 64).unwrap(), b"old");
        private_fs::private_write(&path, b"new").unwrap();
        assert_eq!(private_fs::read_private(&path, 64).unwrap(), b"new");
    }
    #[test]
    fn windows_process_identity_rejects_reused_pid_timestamp() {
        unsafe {
            let (mut created, mut exit, mut kernel, mut user) =
                (zeroed::<FILETIME>(), zeroed(), zeroed(), zeroed());
            assert_ne!(
                GetProcessTimes(
                    GetCurrentProcess(),
                    &mut created,
                    &mut exit,
                    &mut kernel,
                    &mut user
                ),
                0
            );
            let timestamp = (created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64;
            assert!(process_matches(std::process::id(), timestamp).unwrap());
            assert!(!process_matches(std::process::id(), timestamp + 1).unwrap());
        }
    }
}
