use crate::app::{App, Focus};
use anyhow::Result;
use ccc_vim::{transition, TransitionResult};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use std::sync::Arc;
use tracing::warn;

pub async fn handle_events(app: &mut App) -> Result<()> {
    if event::poll(std::time::Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            // Global hotkeys
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                app.should_quit = true;
                return Ok(());
            }

            match app.focus {
                Focus::Input => handle_input_events(app, key).await?,
                Focus::Conversation => handle_conversation_events(app, key).await?,
                Focus::TaskList => {}
            }
        }
    }
    Ok(())
}

async fn handle_input_events(app: &mut App, key: event::KeyEvent) -> Result<()> {
    match &mut app.vim {
        ccc_vim::types::VimState::Insert { .. } => {
            match key.code {
                KeyCode::Esc => {
                    app.vim = ccc_vim::types::VimState::Normal {
                        command: ccc_vim::types::CommandState::Idle,
                    };
                }
                KeyCode::Char(c) => {
                    app.input.insert(app.cursor_pos, c);
                    app.cursor_pos += 1;
                }
                KeyCode::Backspace => {
                    if app.cursor_pos > 0 {
                        app.cursor_pos -= 1;
                        app.input.remove(app.cursor_pos);
                    }
                }
                KeyCode::Delete => {
                    if app.cursor_pos < app.input.len() {
                        app.input.remove(app.cursor_pos);
                    }
                }
                KeyCode::Left => app.cursor_pos = app.cursor_pos.saturating_sub(1),
                KeyCode::Right => app.cursor_pos = (app.cursor_pos + 1).min(app.input.len()),
                KeyCode::Enter => {
                    if !app.input.trim().is_empty() {
                        let input = app.input.clone();
                        app.input.clear();
                        app.cursor_pos = 0;

                        let runner = Arc::clone(&app.runner);
                        let messages = Arc::clone(&app.messages);
                        let session_store = app.session_store.clone();

                        tokio::spawn(async move {
                            let (updated_messages, snapshot) = {
                                let mut runner = runner.lock().await;
                                match runner
                                    .run_with_events(input, |_event| {
                                        // No-op closure
                                    })
                                    .await
                                {
                                    Ok(_) => (runner.messages().clone(), Some(runner.snapshot())),
                                    Err(error) => {
                                        warn!(error = %error, "chat turn failed");
                                        (runner.messages().clone(), None)
                                    }
                                }
                            };

                            if let (Some(store), Some(snapshot)) = (session_store, snapshot) {
                                if let Err(error) = store.save(&snapshot).await {
                                    warn!(error = %error, "failed to persist session snapshot");
                                }
                            }

                            let mut msgs = messages.lock().await;
                            *msgs = updated_messages;
                        });
                    }
                }
                _ => {}
            }
        }
        ccc_vim::types::VimState::Normal { command } => {
            if let KeyCode::Char(c) = key.code {
                let (res, next_command) = transition(command, c, &app.vim_persistent);
                *command = next_command;

                match res {
                    TransitionResult::EnterInsert => {
                        app.vim = ccc_vim::types::VimState::Insert {
                            inserted_text: String::new(),
                        };
                    }
                    TransitionResult::ExecuteMotion { motion_key, count } => match motion_key {
                        'h' => app.cursor_pos = app.cursor_pos.saturating_sub(count as usize),
                        'l' => {
                            app.cursor_pos = (app.cursor_pos + count as usize).min(app.input.len())
                        }
                        '0' | '^' => app.cursor_pos = 0,
                        '$' => app.cursor_pos = app.input.len(),
                        _ => {}
                    },
                    TransitionResult::Reset => {
                        *command = ccc_vim::types::CommandState::Idle;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

async fn handle_conversation_events(app: &mut App, key: event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('i') => app.focus = Focus::Input,
        _ => {}
    }
    Ok(())
}
