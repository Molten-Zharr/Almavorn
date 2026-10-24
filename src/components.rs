use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

/// Executables are shared between libraries; an imported data directory cannot supply code.
pub(crate) fn directory() -> Result<PathBuf> {
    let root = dirs::data_local_dir()
        .context("Не найдена папка компонентов приложения.")?
        .join("almavorn/components");
    validate(&root)?;
    Ok(root)
}
pub(crate) fn validate(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                ensure!(
                    !metadata.file_type().is_symlink(),
                    "Папка компонентов не должна быть символической ссылкой."
                );
                #[cfg(unix)]
                if ancestor == path {
                    use std::os::unix::fs::MetadataExt;
                    unsafe extern "C" {
                        fn geteuid() -> u32;
                    }
                    ensure!(
                        metadata.uid() == unsafe { geteuid() } && metadata.mode() & 0o022 == 0,
                        "Папка компонентов должна принадлежать вам и быть закрыта для записи другими пользователями."
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
pub(crate) fn tool(name: &str) -> Result<PathBuf> {
    let path = directory()?.join(name);
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Файл компонента поврежден. Повторите установку."
        );
        return Ok(path);
    }
    Ok(PathBuf::from(name))
}
