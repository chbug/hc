use anyhow::Context;
use bigdecimal::BigDecimal;
use clap::Parser;

mod hc;
mod stack;
mod state;

#[derive(Parser)]
#[command(version, about, long_about=None)]
struct Cli {
    #[arg(help = "Numbers to push to the stack at startup")]
    extra: Vec<BigDecimal>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let state = state::load().unwrap_or_default();
    let mut term = ratatui::init();
    let mut app = hc::App::new(state)?;
    app.add_extra(cli.extra)?;
    let result = app.run(&mut term);
    // Try to always restore the screen to avoid weird display.
    ratatui::restore();
    // Don't attempt to save the state if something went wrong,
    // to avoid corrupting it.
    result.context("UI failure")?;
    state::save(&app.state())?;
    Ok(())
}
