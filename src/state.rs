use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

use crate::stack::Stack;

/// Permanent state of the app.
#[derive(Serialize, Deserialize, Default)]
pub struct State {
    pub stack: Vec<String>,
    pub precision: Option<u64>,
}

impl From<&Stack> for State {
    fn from(stack: &Stack) -> Self {
        State {
            stack: stack.snapshot().iter().map(|v| v.to_string()).collect(),
            precision: Some(stack.precision()),
        }
    }
}

pub fn load() -> anyhow::Result<State> {
    let json = fs::read_to_string(config_file()?)?;
    let state: State = serde_json::from_str(&json)?;
    Ok(state)
}

pub fn save(state: &State) -> anyhow::Result<()> {
    let path = config_file()?;
    let prefix = path.parent().context("incorrect path")?;
    std::fs::create_dir_all(prefix)?;
    let mut output = File::create(path)?;
    output
        .write_all(serde_json::to_string(state)?.as_bytes())
        .context("failed to write")
}

#[cfg(windows)]
fn config_file() -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(env::var("LOCALAPPDATA")?)
        .join("HelixCalc")
        .join("state.json"))
}

#[cfg(unix)]
fn config_file() -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(env::var("HOME")?)
        .join(".config")
        .join("helix-calc")
        .join("state.json"))
}
