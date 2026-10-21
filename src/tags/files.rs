#[cfg(not(target_os = "linux"))]
use anyhow::bail;
use anyhow::{Context, Result};
use std::{fs::File, path::Path};

pub(super) fn sync_directory(path: &Path) -> Result<()> {
    File::open(path.parent().context("У файла нет родительской папки.")?)?.sync_all()?;
    Ok(())
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{
        ffi::CString,
        os::unix::{ffi::OsStrExt, fs::MetadataExt},
    };
    unsafe extern "C" {
        fn renameat2(
            oldfd: i32,
            old: *const std::ffi::c_char,
            newfd: i32,
            new: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
        fn listxattr(
            path: *const std::ffi::c_char,
            list: *mut std::ffi::c_char,
            size: usize,
        ) -> isize;
        fn getxattr(
            path: *const std::ffi::c_char,
            name: *const std::ffi::c_char,
            value: *mut std::ffi::c_void,
            size: usize,
        ) -> isize;
        fn setxattr(
            path: *const std::ffi::c_char,
            name: *const std::ffi::c_char,
            value: *const std::ffi::c_void,
            size: usize,
            flags: i32,
        ) -> i32;
        fn removexattr(path: *const std::ffi::c_char, name: *const std::ffi::c_char) -> i32;
    }
    fn path_string(path: &Path) -> Result<CString> {
        Ok(CString::new(path.as_os_str().as_bytes())?)
    }
    fn names(path: &CString) -> Result<Vec<CString>> {
        // SAFETY: valid NUL-terminated paths, first call only requests the buffer size.
        let length = unsafe { listxattr(path.as_ptr(), std::ptr::null_mut(), 0) };
        if length < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut bytes = vec![0u8; length as usize];
        let length = unsafe { listxattr(path.as_ptr(), bytes.as_mut_ptr().cast(), bytes.len()) };
        if length < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        bytes.truncate(length as usize);
        bytes
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .map(|name| CString::new(name).map_err(Into::into))
            .collect()
    }
    pub(super) fn copy_attributes(source: &Path, destination: &Path) -> Result<()> {
        let metadata = std::fs::metadata(source)?;
        let target = std::fs::metadata(destination)?;
        if (metadata.uid(), metadata.gid()) != (target.uid(), target.gid()) {
            std::os::unix::fs::chown(destination, Some(metadata.uid()), Some(metadata.gid()))?;
        }
        std::fs::set_permissions(destination, metadata.permissions())?;
        let source = path_string(source)?;
        let destination = path_string(destination)?;
        let original_names = names(&source)?;
        for name in names(&destination)? {
            if !original_names.contains(&name)
                && unsafe { removexattr(destination.as_ptr(), name.as_ptr()) } < 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        for name in original_names {
            let length =
                unsafe { getxattr(source.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
            if length < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let mut value = vec![0u8; length as usize];
            let length = unsafe {
                getxattr(
                    source.as_ptr(),
                    name.as_ptr(),
                    value.as_mut_ptr().cast(),
                    value.len(),
                )
            };
            if length < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            value.truncate(length as usize);
            let mut existing = vec![0u8; value.len()];
            let length = unsafe {
                getxattr(
                    destination.as_ptr(),
                    name.as_ptr(),
                    existing.as_mut_ptr().cast(),
                    existing.len(),
                )
            };
            if length == value.len() as isize && existing == value {
                continue;
            }
            if unsafe {
                setxattr(
                    destination.as_ptr(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                )
            } < 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(())
    }
    pub(super) fn exchange(first: &Path, second: &Path) -> Result<()> {
        let first = path_string(first)?;
        let second = path_string(second)?;
        // RENAME_EXCHANGE preserves the displaced inode until it has been verified.
        if unsafe { renameat2(-100, first.as_ptr(), -100, second.as_ptr(), 2) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}

pub(super) fn copy_attributes(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::copy_attributes(source, destination)
            .context("Не удалось сохранить права или расширенные атрибуты аудиофайла.")
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (source, destination);
        bail!("Безопасная запись тегов на этой платформе пока не поддерживается.")
    }
}

pub(super) fn exchange(first: &Path, second: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::exchange(first, second)
            .context("Файловая система не поддерживает безопасный обмен файлов.")
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (first, second);
        bail!("Безопасная запись тегов на этой платформе пока не поддерживается.")
    }
}
