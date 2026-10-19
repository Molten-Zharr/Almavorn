use crate::remote::Request;
use anyhow::{Context, Result, ensure};
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
        let mut request = None;
        let mut arguments = std::env::args_os().skip(1);
        while let Some(argument) = arguments.next() {
            if argument == "--help" || argument == "-h" {
                println!(
                    "Almavorn {}\nUsage: almavorn [--data-dir DIRECTORY] [MUSIC FILES...]\nИспользование: almavorn [--data-dir КАТАЛОГ] [МУЗЫКАЛЬНЫЕ ФАЙЛЫ...]\nУправление работающим плеером:\n  --play | --pause | --play-pause | --stop | --next | --previous\n  --volume-up | --volume-down | --volume 0..100\n  --command ИМЯ     выполнить действие (--commands: список)\n  --status          состояние плеера\n  --niri-bindings   вывести глобальные сочетания для niri\n--version: version / версия. F1: help / помощь. Q: quit / выход.",
                    env!("CARGO_PKG_VERSION")
                );
                return Ok(None);
            } else if argument == "--version" || argument == "-V" {
                println!("Almavorn {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            } else if argument == "--data-dir" {
                directory = PathBuf::from(
                    arguments
                        .next()
                        .context("--data-dir requires a directory")?,
                );
            } else if argument == "--commands" {
                println!("play\npause\nstatus\nniri-bindings");
                for (action, name) in crate::remote::COMMANDS {
                    println!(
                        "{name:<20} {}",
                        action.name(crate::model::Language::Russian)
                    );
                }
                return Ok(None);
            } else if argument == "--command" {
                ensure!(
                    request.is_none(),
                    "Use one command per invocation / Укажите одну команду за вызов"
                );
                let name = arguments
                    .next()
                    .context("--command requires a command name")?;
                request = Some(Request::parse(
                    name.to_str().context("Command name must be UTF-8")?,
                )?);
            } else if argument == "--volume" {
                ensure!(request.is_none(), "Укажите одну команду за вызов");
                let value: u8 = arguments
                    .next()
                    .context("--volume requires 0..100")?
                    .to_str()
                    .context("Invalid volume")?
                    .parse()
                    .context("Volume must be 0..100")?;
                ensure!(
                    value <= 100,
                    "Volume must be 0..100 / Громкость: от 0 до 100"
                );
                request = Some(Request::Volume(value));
            } else if matches!(
                argument.to_str(),
                Some(
                    "--play"
                        | "--pause"
                        | "--play-pause"
                        | "--stop"
                        | "--next"
                        | "--previous"
                        | "--volume-up"
                        | "--volume-down"
                        | "--seek-forward"
                        | "--seek-backward"
                        | "--status"
                        | "--niri-bindings"
                )
            ) {
                ensure!(request.is_none(), "Укажите одну команду за вызов");
                request = Some(Request::parse(
                    argument
                        .to_str()
                        .expect("matched UTF-8")
                        .trim_start_matches("--"),
                )?);
            } else if argument.to_string_lossy().starts_with('-') {
                anyhow::bail!("Unknown option: {}", argument.to_string_lossy());
            } else {
                files.push(PathBuf::from(argument));
            }
        }
        if let Some(request) = request {
            ensure!(
                files.is_empty(),
                "Команду управления нельзя совмещать с добавлением файлов."
            );
            #[cfg(unix)]
            {
                let output = crate::remote::send(&directory, &request)?;
                if !output.is_empty() {
                    println!("{output}");
                }
            }
            #[cfg(not(unix))]
            anyhow::bail!(
                "Global command control is supported on Unix / Командное управление поддерживается в Unix"
            );
            return Ok(None);
        }
        Ok(Some(Self { directory, files }))
    }
}
