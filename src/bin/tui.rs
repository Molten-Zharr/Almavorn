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
    io::stdout,
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
    let mut app = App::new(&options.directory)?;
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
    while !app.quit {
        app.tick();
        terminal.draw(|frame| ui::render(&mut app, frame))?;
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    app.action(Action::Quit)?;
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
                    MouseEventKind::ScrollUp => app.handle(Input::Scroll { x, y, delta: -3 }),
                    MouseEventKind::ScrollDown => app.handle(Input::Scroll { x, y, delta: 3 }),
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
