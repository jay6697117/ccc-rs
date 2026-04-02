use crate::app::{App, Focus};
use ccc_core::McpConnectionStatus;
use ratatui::{prelude::*, widgets::*};

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(0),    // Conversation
            Constraint::Length(5), // MCP
            Constraint::Length(3), // Input
        ])
        .split(f.size());

    // Header
    let header = Paragraph::new(format!(
        "Claude Code CLI (Rust) | {}",
        mcp_status_summary(app)
    ))
    .style(
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

    let mcp_items = match app.mcp_connections.try_lock() {
        Ok(snapshots) if !snapshots.is_empty() => snapshots
            .iter()
            .map(|snapshot| {
                let detail = snapshot.error.as_deref().unwrap_or("");
                let suffix = if detail.is_empty() {
                    String::new()
                } else {
                    format!(" — {detail}")
                };
                ListItem::new(format!(
                    "{} [{} {:?}]{}",
                    snapshot.name,
                    render_mcp_status(snapshot.status),
                    snapshot.source_scope,
                    suffix
                ))
            })
            .collect::<Vec<_>>(),
        _ => vec![ListItem::new("No MCP servers configured")],
    };
    let mcp_panel = List::new(mcp_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" MCP Status "),
    );
    f.render_widget(mcp_panel, chunks[2]);

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
    f.render_widget(input, chunks[3]);

    if app.focus == Focus::Input {
        f.set_cursor(chunks[3].x + app.cursor_pos as u16 + 1, chunks[3].y + 1);
    }
}

fn mcp_status_summary(app: &App) -> String {
    let Ok(snapshots) = app.mcp_connections.try_lock() else {
        return "MCP loading".into();
    };
    if snapshots.is_empty() {
        return "MCP none".into();
    }

    let mut connected = 0;
    let mut pending = 0;
    let mut failed = 0;
    let mut needs_auth = 0;
    let mut disabled = 0;

    for snapshot in snapshots.iter() {
        match snapshot.status {
            McpConnectionStatus::Connected => connected += 1,
            McpConnectionStatus::Pending => pending += 1,
            McpConnectionStatus::Failed => failed += 1,
            McpConnectionStatus::NeedsAuth => needs_auth += 1,
            McpConnectionStatus::Disabled => disabled += 1,
        }
    }

    format!(
        "MCP connected:{connected} pending:{pending} needs-auth:{needs_auth} failed:{failed} disabled:{disabled}"
    )
}

fn render_mcp_status(status: McpConnectionStatus) -> &'static str {
    match status {
        McpConnectionStatus::Pending => "pending",
        McpConnectionStatus::Connected => "connected",
        McpConnectionStatus::Failed => "failed",
        McpConnectionStatus::NeedsAuth => "needs-auth",
        McpConnectionStatus::Disabled => "disabled",
    }
}
