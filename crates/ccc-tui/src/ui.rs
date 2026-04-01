use crate::app::{App, Focus};
use ratatui::{prelude::*, widgets::*};

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(0),    // Conversation
            Constraint::Length(3), // Input
        ])
        .split(f.size());

    // Header
    let header = Paragraph::new("Claude Code CLI (Rust) — Phase 7 TUI").style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(header, chunks[0]);

    // Conversation
    // TUI render is sync, but we use tokio Mutex.
    // We should use try_lock or block_in_place, but here try_lock is safer for UI.
    let messages_guard = app.messages.try_lock();
    if let Ok(messages_lock) = messages_guard {
        let messages: Vec<ListItem> = messages_lock
            .iter()
            .map(|m| {
                let role_str = match m.role {
                    ccc_core::types::Role::User => "[User]",
                    ccc_core::types::Role::Assistant => "[Claude]",
                };
                let content = m
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ccc_core::types::ContentBlock::Text { text } => Some(text.as_str()),
                        ccc_core::types::ContentBlock::Thinking { thinking, .. } => {
                            Some(thinking.as_str())
                        }
                        ccc_core::types::ContentBlock::ToolUse { name, .. } => Some(name.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                ListItem::new(format!("{}: {}", role_str, content))
            })
            .collect();

        let conversation = List::new(messages).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Conversation "),
        );
        f.render_widget(conversation, chunks[1]);
    } else {
        let conversation = List::new(vec![ListItem::new("Loading...")]).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Conversation "),
        );
        f.render_widget(conversation, chunks[1]);
    }

    // Input
    let (input_title, input_style) = match (&app.vim, app.focus) {
        (ccc_vim::types::VimState::Insert { .. }, Focus::Input) => {
            (" Input (INSERT) ", Style::default().fg(Color::Yellow))
        }
        (ccc_vim::types::VimState::Normal { .. }, Focus::Input) => {
            (" Input (NORMAL) ", Style::default().fg(Color::Cyan))
        }
        (_, _) => (" Input ", Style::default()),
    };

    let input = Paragraph::new(app.input.as_str()).block(
        Block::default()
            .borders(Borders::ALL)
            .title(input_title)
            .style(input_style),
    );
    f.render_widget(input, chunks[2]);

    if app.focus == Focus::Input {
        f.set_cursor(chunks[2].x + app.cursor_pos as u16 + 1, chunks[2].y + 1);
    }
}
