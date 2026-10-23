use super::{Recovery, Stamp, files};
use anyhow::{Result, bail, ensure};
use std::path::{Path, PathBuf};

fn exists(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn copy_matches(path: &Path, expected: &Stamp) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
        && Stamp::read(path)
            .is_ok_and(|s| (s.length, s.digest) == (expected.length, expected.digest))
}

fn clean_copy(path: &Path, parent: &Path, expected: &Stamp) -> Result<()> {
    if !exists(path)? {
        return Ok(());
    }
    ensure!(
        path.parent() == parent.parent()
            && path != parent
            && path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(".almavorn-tags-"))
            && copy_matches(path, expected),
        "Временная копия изменена. Файл сохранен: {}",
        path.display()
    );
    std::fs::remove_file(path)?;
    files::sync_directory(path)
}

// Move the exact inode out of the public path before removing it. A concurrent
// replacement is put back without overwriting anything, or retained for recovery.
fn remove_expected(path: &Path, holding: &Path, stamp: &Stamp) -> Result<Option<PathBuf>> {
    files::rename_noreplace(path, holding)?;
    files::sync_directory(path)?;
    if Stamp::read(holding).ok().as_ref() != Some(stamp) {
        return Ok(Some(if files::rename_noreplace(holding, path).is_ok() {
            files::sync_directory(path)?;
            path.to_owned()
        } else {
            holding.to_owned()
        }));
    }
    std::fs::remove_file(holding)?;
    files::sync_directory(holding)?;
    Ok(None)
}

pub(super) fn finish(record: &Recovery, committed: bool) -> Result<Option<PathBuf>> {
    let destination = record
        .destination
        .as_ref()
        .expect("relocation has a destination");
    ensure!(
        destination != &record.path
            && record.backup.parent() == record.path.parent()
            && record.staging.parent() == destination.parent()
            && [&record.backup, &record.staging].iter().all(|p| p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(".almavorn-tags-"))),
        "Некорректные пути восстановления перемещения."
    );
    if committed {
        ensure!(
            Stamp::read(destination).ok().as_ref() == Some(&record.after),
            "Перенесенный файл изменен или недоступен. Исходная копия сохранена: {}",
            record.backup.display()
        );
        if exists(&record.path)? {
            if Stamp::read(&record.path).ok().as_ref() != Some(&record.before) {
                clean_copy(&record.backup, &record.path, &record.before)?;
                clean_copy(&record.staging, destination, &record.after)?;
                return Ok(Some(record.path.clone()));
            }
            clean_copy(&record.backup, &record.path, &record.before)?;
            if let Some(preserved) = remove_expected(&record.path, &record.backup, &record.before)?
            {
                return Ok(Some(preserved));
            }
        } else {
            clean_copy(&record.backup, &record.path, &record.before)?;
        }
        clean_copy(&record.staging, destination, &record.after)?;
    } else {
        // The database still uses the source; never remove its last usable copy.
        if exists(&record.path)? && !copy_matches(&record.path, &record.before) {
            bail!(
                "Исходный файл изменен другой программой. Исходная копия сохранена: {}. Подготовленный файл: {}",
                record.backup.display(),
                destination.display()
            );
        }
        if !exists(&record.path)? {
            ensure!(
                copy_matches(&record.backup, &record.before),
                "Не найдена исходная копия: {}",
                record.backup.display()
            );
            files::rename_noreplace(&record.backup, &record.path)?;
            files::sync_directory(&record.path)?;
        }
        if exists(destination)? && Stamp::read(destination).ok().as_ref() == Some(&record.after) {
            clean_copy(&record.staging, destination, &record.after)?;
            if let Some(preserved) = remove_expected(destination, &record.staging, &record.after)? {
                bail!(
                    "Файл изменен другой программой и сохранен: {}",
                    preserved.display()
                );
            }
        }
        clean_copy(&record.backup, &record.path, &record.before)?;
        clean_copy(&record.staging, destination, &record.after)?;
    }
    Ok(None)
}
