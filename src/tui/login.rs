use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use ahi_widgets::{Paragraph, Wrap};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui_core::layout::{Constraint, Layout};

use crate::directories::Directories;
use crate::error::{Error, Result};
use crate::harness::OpenAiLogin;

pub(crate) fn run(state: &Directories) -> Result<()> {
    let mut terminal = crate::term::try_init().map_err(|error| Error::new(error.to_string()))?;
    let result = crossterm::execute!(std::io::stdout(), EnableBracketedPaste)
        .map_err(|error| Error::new(error.to_string()))
        .and_then(|()| login_loop(&mut terminal, state));
    let cleanup = crossterm::execute!(std::io::stdout(), DisableBracketedPaste);
    crate::term::restore();
    cleanup.map_err(|error| Error::new(error.to_string()))?;
    result
}

fn login_loop(terminal: &mut crate::term::DefaultTerminal, state: &Directories) -> Result<()> {
    let mut login = OpenAiLogin::start(&state.data)?;
    let mut callback = String::new();
    let mut status =
        "Waiting for browser login. Paste the full callback URL if needed.".to_string();
    let mut scroll = 0_u16;
    let mut browser_opened = false;
    let mut copy_result: Option<mpsc::Receiver<bool>> = None;
    loop {
        if let Some(receiver) = &copy_result {
            match receiver.try_recv() {
                Ok(true) => {
                    status =
                        "URL sent to clipboard (terminal copy requests cannot confirm acceptance)."
                            .into();
                    copy_result = None;
                }
                Ok(false) | Err(TryRecvError::Disconnected) => {
                    status = "Clipboard unavailable. Ctrl+Y retries; the URL remains above.".into();
                    copy_result = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        terminal
            .draw(|frame| {
                let areas = Layout::vertical([Constraint::Length(2), Constraint::Min(1), Constraint::Length(7)])
                    .split(frame.area());
                frame.render_widget(Paragraph::new("OpenAI login").style(ratatui_core::style::Style::default().add_modifier(ratatui_core::style::Modifier::BOLD)), areas[0]);
                frame.render_widget(
                    Paragraph::new(format!(
                        "Open this URL to sign in with ChatGPT:\n\n{}\n\nThe browser redirects to http://127.0.0.1:1455/auth/callback.\nLogin does not refresh catalogs or enable OpenAI inference.",
                        login.authorization_url()
                    ))
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                    areas[1],
                );
                frame.render_widget(
                    Paragraph::new(format!(
                        "{status}\nCallback: {} characters (hidden)\nCtrl+Y copy URL  Enter submit  Ctrl+U clear\nUp/Down scroll URL  Esc/Ctrl+C cancel",
                        callback.chars().count()
                    ))
                    .wrap(Wrap { trim: false }),
                    areas[2],
                );
            })
            .map_err(|error| Error::new(error.to_string()))?;
        if !browser_opened {
            login.open_browser();
            browser_opened = true;
        }
        if let Some(result) = login.poll() {
            return result;
        }
        if !event::poll(Duration::from_millis(50)).map_err(|error| Error::new(error.to_string()))? {
            continue;
        }
        match event::read().map_err(|error| Error::new(error.to_string()))? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Esc => return Err(Error::new("OpenAI login cancelled")),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Err(Error::new("OpenAI login cancelled"));
                }
                KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if copy_result.is_none() {
                        let (sender, receiver) = mpsc::channel();
                        copy_result = Some(receiver);
                        status = "Copying login URL…".into();
                        // Use the raw URL, not rendered terminal cells. Helpers can
                        // take time; keep callback polling and cancellation responsive.
                        let url = login.authorization_url().to_string();
                        std::thread::spawn(move || {
                            let _ = sender.send(crate::clipboard::copy(&url));
                        });
                    }
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    callback.clear();
                }
                KeyCode::Enter if !callback.trim().is_empty() => {
                    match login.submit_callback(&callback) {
                        Ok(()) => status = "Completing OpenAI login…".to_string(),
                        Err(error) => status = error.to_string(),
                    }
                    callback.clear();
                }
                KeyCode::Backspace => {
                    callback.pop();
                }
                KeyCode::Up => scroll = scroll.saturating_sub(1),
                KeyCode::Down => scroll = scroll.saturating_add(1),
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && !character.is_control()
                        && callback.len() + character.len_utf8() <= 8192 =>
                {
                    callback.push(character);
                }
                _ => {}
            },
            Event::Paste(text) => {
                if callback.len() + text.len() <= 8192 {
                    callback.push_str(text.trim());
                } else {
                    status = "Callback URL exceeds 8192 bytes; clear and paste again".to_string();
                }
            }
            _ => {}
        }
    }
}
