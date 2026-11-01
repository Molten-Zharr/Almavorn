use super::{MAX_COVER, NoCover, wait};
use crate::model::Language;
use anyhow::{Result, ensure};
use reqwest::{blocking::Client, header::RETRY_AFTER};
use std::{
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Clone, Copy)]
pub(crate) struct Progress {
    pub attempt: u8,
    pub wait_seconds: u64,
}

#[derive(Debug)]
struct HttpFailure {
    status: u16,
    delay: Option<u64>,
}

impl std::fmt::Display for HttpFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Cover Art Archive: HTTP {}", self.status)
    }
}
impl std::error::Error for HttpFailure {}

fn attempt(client: &Client, url: &str, number: u8) -> Result<Vec<u8>> {
    let response = client.get(url).send()?;
    let status = response.status().as_u16();
    if status == 404 {
        return Err(NoCover.into());
    }
    if !response.status().is_success() {
        let delay = if matches!(status, 429 | 500 | 502 | 503 | 504) {
            match response.headers().get(RETRY_AFTER) {
                Some(value) => value
                    .to_str()
                    .ok()
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .filter(|delay| *delay <= 30),
                None => Some(2_u64.pow(u32::from(number))),
            }
        } else {
            None
        };
        return Err(HttpFailure { status, delay }.into());
    }
    ensure!(
        response
            .content_length()
            .is_none_or(|size| size <= MAX_COVER),
        "Обложка больше 8 МБ."
    );
    let mut bytes = Vec::new();
    response.take(MAX_COVER + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_COVER, "Обложка больше 8 МБ.");
    Ok(bytes)
}

fn retry_delay(error: &anyhow::Error, attempt: u8) -> Option<u64> {
    if let Some(error) = error.downcast_ref::<HttpFailure>() {
        return error.delay;
    }
    let transient = error.chain().any(|cause| {
        if let Some(error) = cause.downcast_ref::<reqwest::Error>() {
            error.is_timeout() || error.is_connect() || error.is_body()
        } else {
            cause.downcast_ref::<std::io::Error>().is_some_and(|error| {
                matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::BrokenPipe
                )
            })
        }
    });
    transient.then(|| 2_u64.pow(u32::from(attempt)))
}

fn message(error: &anyhow::Error, language: Language, attempts: u8) -> String {
    let reason = if let Some(error) = error.downcast_ref::<HttpFailure>() {
        format!(
            "{} (HTTP {}).",
            language.text(
                "Cover Art Archive could not return the image",
                "Cover Art Archive не вернул изображение"
            ),
            error.status
        )
    } else if let Some(error) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<reqwest::Error>())
    {
        language
            .text(
                if error.is_timeout() {
                    "Cover Art Archive did not respond in time."
                } else {
                    "Could not connect to Cover Art Archive."
                },
                if error.is_timeout() {
                    "Cover Art Archive не ответил вовремя."
                } else {
                    "Не удалось подключиться к Cover Art Archive."
                },
            )
            .into()
    } else if error.downcast_ref::<std::io::Error>().is_some() {
        language
            .text(
                "The artwork download was interrupted.",
                "Загрузка обложки прервалась.",
            )
            .into()
    } else {
        return error.to_string();
    };
    format!(
        "{reason} {}: {attempts}/3. {}",
        language.text("Attempts", "Попыток"),
        language.text(
            "Try again later. Technical details: Details.",
            "Повторите позже. Техническая причина: Подробнее."
        )
    )
}

pub(super) fn download(
    client: &Client,
    url: &str,
    cancel: &AtomicBool,
    language: Language,
    progress: &mut impl FnMut(Progress),
) -> Result<Vec<u8>> {
    for number in 1..=3 {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Загрузка обложки отменена."
        );
        progress(Progress {
            attempt: number,
            wait_seconds: 0,
        });
        match attempt(client, url, number) {
            Ok(bytes) => {
                ensure!(
                    !cancel.load(Ordering::Relaxed),
                    "Загрузка обложки отменена."
                );
                return Ok(bytes);
            }
            Err(error) if error.is::<NoCover>() => return Err(error),
            Err(error) => {
                ensure!(
                    !cancel.load(Ordering::Relaxed),
                    "Загрузка обложки отменена."
                );
                if number < 3
                    && let Some(delay) = retry_delay(&error, number)
                {
                    progress(Progress {
                        attempt: number + 1,
                        wait_seconds: delay,
                    });
                    wait(cancel, Duration::from_secs(delay))?;
                } else {
                    let message = message(&error, language, number);
                    return Err(error.context(message));
                }
            }
        }
    }
    unreachable!("download returns after its last attempt")
}
