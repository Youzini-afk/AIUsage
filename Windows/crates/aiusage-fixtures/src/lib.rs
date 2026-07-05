use std::{fs, path::PathBuf};

use aiusage_core::DesktopSnapshot;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FixtureError {
    #[error("fixture file could not be read: {0}")]
    Io(#[from] std::io::Error),
    #[error("fixture JSON did not match the schema: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
}

pub fn load_desktop_snapshot_fixture(name: &str) -> Result<DesktopSnapshot, FixtureError> {
    let data = fs::read(fixture_root().join(name))?;
    Ok(serde_json::from_slice(&data)?)
}
