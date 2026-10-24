use anyhow::{Context, Result};
use std::{
    hash::{Hash, Hasher},
    path::Path,
};

fn entry(data: &Path, id: &str) -> Result<keyring::Entry> {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hash);
    keyring::Entry::new("Almavorn", &format!("{:016x}:{id}", hash.finish()))
        .context("Не удалось открыть системное хранилище паролей.")
}

// Called only from blocking workers, never from the UI or Tokio executor.
pub(crate) fn get(data: &Path, id: &str) -> Result<Option<String>> {
    match entry(data, id)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => {
            Err(error).context("Откройте системное хранилище паролей и повторите действие.")
        }
    }
}
pub(crate) fn set(data: &Path, id: &str, value: &str) -> Result<()> {
    entry(data, id)?
        .set_password(value)
        .context("Не удалось сохранить данные входа в системное хранилище паролей.")
}
pub(crate) fn remove(data: &Path, id: &str) -> Result<()> {
    match entry(data, id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error).context("Не удалось удалить сохраненные данные входа."),
    }
}
