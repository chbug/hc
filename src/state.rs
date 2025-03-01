use anyhow::Context;
use bigdecimal::{BigDecimal, ParseBigDecimalError};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    env,
    fs::{self, File},
    io::Write,
    path::PathBuf,
    str::FromStr,
};

/// Permanent state of the app.
#[derive(Serialize, Deserialize, Default)]
pub struct State {
    stack: Vec<String>,
}

impl TryFrom<&State> for VecDeque<BigDecimal> {
    type Error = ParseBigDecimalError;

    fn try_from(value: &State) -> Result<Self, Self::Error> {
        let mut result = VecDeque::new();
        for v in &value.stack {
            result.push_back(BigDecimal::from_str(v)?);
        }
        Ok(result)
    }
}

impl From<&VecDeque<BigDecimal>> for State {
    fn from(value: &VecDeque<BigDecimal>) -> Self {
        State {
            stack: value.iter().map(|v| v.to_string()).collect(),
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
