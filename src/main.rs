use anyhow::Context;

mod hc;
mod stack;
mod state;

fn main() -> anyhow::Result<()> {
    let state = state::load().unwrap_or_default();
    let mut term = ratatui::init();
    let mut app = hc::App::new(state)?;
    let result = app.run(&mut term);
    // Try to always restore the screen to avoid weird display.
    ratatui::restore();
    // Don't attempt to save the state if something went wrong,
    // to avoid corrupting it.
    result.context("UI failure")?;
    state::save(&app.state())?;
    Ok(())
}
