use anyhow::Result;
mod app;
mod events;
mod terminal;
mod ui;

use crate::app::App;
use crate::events::handle_events;
use crate::terminal::{setup_terminal, TerminalGuard};
use crate::ui::render;

#[tokio::main]
async fn main() -> Result<()> {
    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard;

    let mut app = App::new();

    loop {
        terminal.draw(|f| render(f, &app))?;

        handle_events(&mut app).await?;

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
