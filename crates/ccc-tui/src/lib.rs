pub mod app;
pub mod events;
pub mod terminal;
pub mod ui;

pub use app::AppConfig;

use anyhow::Result;
use app::App;
use events::handle_events;
use terminal::{setup_terminal, TerminalGuard};
use ui::render;

pub async fn run_app(config: AppConfig) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard;

    let mut app = App::new(config)?;

    loop {
        terminal.draw(|f| render(f, &app))?;

        handle_events(&mut app).await?;

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
