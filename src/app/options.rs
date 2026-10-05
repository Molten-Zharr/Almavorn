use anyhow::{Context, Result};
use std::path::PathBuf;

pub struct Options {
    pub directory: PathBuf,
    pub files: Vec<PathBuf>,
}

impl Options {
    pub fn parse() -> Result<Option<Self>> {
        let mut directory = dirs::data_local_dir()
            .context("Cannot locate local data directory")?
            .join("almavorn");
        let mut files = Vec::new();
        let mut arguments = std::env::args_os().skip(1);
        while let Some(argument) = arguments.next() {
            if argument == "--help" || argument == "-h" {
                println!(
                    "Almavorn\nUsage: almavorn [--data-dir DIRECTORY] [MUSIC FILES...]\nИспользование: almavorn [--data-dir КАТАЛОГ] [МУЗЫКАЛЬНЫЕ ФАЙЛЫ...]\nF1: help / помощь. Q: quit / выход."
                );
                return Ok(None);
            } else if argument == "--data-dir" {
                directory = PathBuf::from(
                    arguments
                        .next()
                        .context("--data-dir requires a directory")?,
                );
            } else if argument.to_string_lossy().starts_with('-') {
                anyhow::bail!("Unknown option: {}", argument.to_string_lossy());
            } else {
                files.push(PathBuf::from(argument));
            }
        }
        Ok(Some(Self { directory, files }))
    }
}
