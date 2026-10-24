use anyhow::{Context, Result, bail, ensure};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub(crate) struct Output {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub success: bool,
}

pub(crate) fn run(
    mut command: Command,
    input: Option<Vec<u8>>,
    cancel: &AtomicBool,
    timeout: Duration,
    limit: usize,
) -> Result<Output> {
    ensure!(!cancel.load(Ordering::Relaxed), "Операция отменена.");
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .context("Не удалось запустить компонент. Установите компоненты в этом разделе.")?;
    let input = child.stdin.take().map(|mut stdin| {
        std::thread::spawn(move || -> std::io::Result<()> {
            if let Some(input) = input {
                stdin.write_all(&input)?;
            }
            Ok(())
        })
    });
    let overflow = Arc::new(AtomicBool::new(false));
    fn reader(
        mut stream: impl Read + Send + 'static,
        limit: usize,
        overflow: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
        std::thread::spawn(move || {
            let mut data = Vec::new();
            (&mut stream)
                .take(limit as u64 + 1)
                .read_to_end(&mut data)?;
            if data.len() > limit {
                overflow.store(true, Ordering::Relaxed);
            }
            Ok(data)
        })
    }
    let stdout = reader(
        child.stdout.take().expect("stdout"),
        limit,
        overflow.clone(),
    );
    let stderr = reader(
        child.stderr.take().expect("stderr"),
        256 * 1024,
        overflow.clone(),
    );
    let started = Instant::now();
    let mut interrupted = None;
    let mut status = None;
    loop {
        if status.is_none() {
            match child.try_wait() {
                Ok(Some(value)) => status = Some(value),
                Ok(None) => {}
                Err(error) => {
                    interrupted = Some(error.to_string());
                    break;
                }
            }
        }
        if status.is_some()
            && stdout.is_finished()
            && stderr.is_finished()
            && input.as_ref().is_none_or(|writer| writer.is_finished())
        {
            break;
        }
        if cancel.load(Ordering::Relaxed) {
            interrupted = Some("Операция отменена.".into());
            break;
        }
        if overflow.load(Ordering::Relaxed) {
            interrupted = Some("Компонент вернул слишком большой ответ.".into());
            break;
        }
        if started.elapsed() > timeout {
            interrupted = Some("Компонент не завершил запрос вовремя. Повторите действие.".into());
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if interrupted.is_some() {
        #[cfg(unix)]
        {
            unsafe extern "C" {
                fn kill(pid: i32, signal: i32) -> i32;
            }
            // Kill the owned group, including descendants still holding output pipes.
            unsafe {
                kill(-(child.id() as i32), 9);
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    let drain_deadline = Instant::now() + Duration::from_secs(1);
    while (!stdout.is_finished()
        || !stderr.is_finished()
        || input.as_ref().is_some_and(|writer| !writer.is_finished()))
        && Instant::now() < drain_deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    ensure!(
        stdout.is_finished()
            && stderr.is_finished()
            && input.as_ref().is_none_or(|writer| writer.is_finished()),
        "Компонент не закрыл соединение. Запрос отменен."
    );
    if let Some(input) = input {
        let _ = input.join();
    }
    let out = stdout
        .join()
        .map_err(|_| anyhow::anyhow!("Чтение ответа компонента прервано."))??;
    let err = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("Чтение ответа компонента прервано."))??;
    if let Some(reason) = interrupted {
        bail!(reason);
    }
    ensure!(
        !overflow.load(Ordering::Relaxed),
        "Компонент вернул слишком большой ответ."
    );
    Ok(Output {
        stdout: out,
        stderr: err,
        success: status.is_some_and(|s| s.success()),
    })
}
