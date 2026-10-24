use almavorn::{
    app::{App, Options},
    input::{Action, Input, Key, KeyPress},
    ui,
};
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Position};
use std::{
    io::{Write, stdout},
    time::{Duration, Instant},
};

struct TerminalSession;
impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}

fn copy_to_terminal(text: &str) -> std::io::Result<()> {
    // OSC 52 works without desktop clipboard utilities, including over SSH.
    // The terminal decides whether clipboard writes are permitted.
    const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut sequence = Vec::with_capacity(8 + text.len().div_ceil(3) * 4);
    sequence.extend_from_slice(b"\x1b]52;c;");
    for chunk in text.as_bytes().chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        sequence.push(BASE64[(first >> 2) as usize]);
        sequence.push(BASE64[(((first & 3) << 4) | (second >> 4)) as usize]);
        sequence.push(if chunk.len() > 1 {
            BASE64[(((second & 15) << 2) | (third >> 6)) as usize]
        } else {
            b'='
        });
        sequence.push(if chunk.len() > 2 {
            BASE64[(third & 63) as usize]
        } else {
            b'='
        });
    }
    sequence.push(b'\x07');
    let mut output = stdout().lock();
    output.write_all(&sequence)?;
    output.flush()
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Almavorn: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let Some(options) = Options::parse()? else {
        return Ok(());
    };
    let mut app = App::new_with_commands(&options.directory)?;
    if !options.files.is_empty() {
        app.start_import(options.files)?;
    }
    enable_raw_mode()?;
    let _session = TerminalSession;
    execute!(
        stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        old_hook(info);
    }));
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut previous_click: Option<(Position, Instant)> = None;
    let mut pending_event = None;
    while !app.view.quit {
        app.tick();
        if let Some(text) = app.view.clipboard_text.take() {
            app.clipboard_result(
                copy_to_terminal(&text).map_err(|error| error.to_string()),
                true,
            );
        }
        terminal.draw(|frame| ui::render(&mut app, frame))?;
        let mut input = if let Some(input) = pending_event.take() {
            input
        } else {
            if !event::poll(Duration::from_millis(50))? {
                continue;
            }
            event::read()?
        };
        let mut hover = None;
        let started = Instant::now();
        // A motion burst needs one redraw, not one full frame per queued position.
        // Stop at every other event so clicks keep the geometry of the last frame.
        for _ in 0..128 {
            let Event::Mouse(mouse) = &input else {
                break;
            };
            if mouse.kind != MouseEventKind::Moved
                || started.elapsed() >= Duration::from_millis(8)
                || !event::poll(Duration::ZERO)?
            {
                break;
            }
            hover = Some(Position::new(mouse.column, mouse.row));
            input = event::read()?;
        }
        if !matches!(input, Event::Mouse(mouse) if mouse.kind == MouseEventKind::Moved)
            && let Some(point) = hover
        {
            app.handle(Input::Move {
                x: point.x,
                y: point.y,
            });
            pending_event = Some(input);
            continue;
        }
        match input {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    if app.copy_color_hex(false) || app.copy_help(None) {
                        continue;
                    }
                    if !app.writing_tags() {
                        app.action(Action::Quit)?;
                    }
                    continue;
                }
                if app.accepts_text()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && let KeyCode::Char(c) = key.code
                {
                    app.handle(Input::Text(c.to_string()));
                    continue;
                }
                if let Some(code) = convert_key(key.code) {
                    app.handle(Input::Key(KeyPress {
                        key: code,
                        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
                        alt: key.modifiers.contains(KeyModifiers::ALT),
                        shift: key.modifiers.contains(KeyModifiers::SHIFT),
                    }));
                }
            }
            Event::Paste(text) => app.handle(Input::Text(text)),
            Event::Mouse(mouse) => {
                let (x, y) = (mouse.column, mouse.row);
                match mouse.kind {
                    MouseEventKind::Moved => app.handle(Input::Move { x, y }),
                    MouseEventKind::Down(event::MouseButton::Left) => {
                        let point = Position::new(x, y);
                        let double = previous_click.as_ref().is_some_and(|(previous, time)| {
                            *previous == point && time.elapsed() < Duration::from_millis(400)
                        });
                        previous_click = Some((point, Instant::now()));
                        app.handle(Input::Click { x, y, double });
                    }
                    MouseEventKind::Drag(event::MouseButton::Left) => {
                        app.handle(Input::Drag { x, y });
                    }
                    MouseEventKind::Up(event::MouseButton::Left) => {
                        app.handle(Input::Release { x, y })
                    }
                    MouseEventKind::Down(event::MouseButton::Right) => {
                        app.handle(Input::SecondaryClick { x, y })
                    }
                    MouseEventKind::ScrollUp => app.handle(Input::Scroll { x, y, delta: -1 }),
                    MouseEventKind::ScrollDown => app.handle(Input::Scroll { x, y, delta: 1 }),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn convert_key(key: KeyCode) -> Option<Key> {
    Some(match key {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Escape,
        KeyCode::Tab | KeyCode::BackTab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::F(n) => Key::F(n),
        _ => return None,
    })
}
