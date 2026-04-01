use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    ccc_tui::run_app(ccc_tui::AppConfig::default()).await
}
