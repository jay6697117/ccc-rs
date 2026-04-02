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
    let mcp_bootstrap = config.mcp_bootstrap.clone();
    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard;

    let mut app = App::new(config)?;
    app.bootstrap_mcp_plan(&mcp_bootstrap).await?;

    loop {
        terminal.draw(|f| render(f, &app))?;

        handle_events(&mut app).await?;

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
