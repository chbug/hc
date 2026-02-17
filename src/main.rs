use anyhow::Context;
use clap::Parser;

mod hc;
mod help;
mod input;
mod stack;
mod state;

#[derive(Parser)]
#[command(version, about, long_about=None)]
struct Cli {
    #[arg(help = "Operations to perform at startup")]
    extra: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initial loading and pre-UI calculations.
    // We haven't taken over the screen yet, so it's fine to
    // just return an error.
    let state = state::load().unwrap_or_default();
    let mut app = hc::App::new(state)?;
    app.add_extra(cli.extra.join(" "))?;

    // From here on, we need to restore prior to failing.
    let mut term = ratatui::init();
    let result = app.run(&mut term);
    ratatui::restore();
    // Don't attempt to save the state if something went wrong,
    // to avoid corrupting it.
    result.context("UI failure")?;
    let state = app.state();
    state::save(&state)?;
    // Provide the top of the stack in the output for convenience.
    if !state.stack.is_empty() {
        println!("{}", state.stack[0]);
    }
    Ok(())
}
