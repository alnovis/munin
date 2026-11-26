use std::ffi::OsString;
use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::store::Options;

pub const ENV_DB: &str = "MUNIN_DB";
pub const ENV_PROJECT: &str = "MUNIN_PROJECT";
pub const ENV_HALF_LIFE_DAYS: &str = "MUNIN_HALF_LIFE_DAYS";

/// `MUNIN_DB` when set and non-empty, otherwise `~/.munin/munin.db`.
pub fn db_path(env_db: Option<OsString>, home: Option<OsString>) -> Result<PathBuf> {
    if let Some(path) = env_db.filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = home.filter(|h| !h.is_empty()).ok_or_else(|| {
        Error::invalid(
            "database",
            format!("HOME is not set; set {ENV_DB} to a file path"),
        )
    })?;
    Ok(PathBuf::from(home).join(".munin").join("munin.db"))
}

pub fn options(half_life_days: Option<String>) -> Result<Options> {
    let mut options = Options::default();
    if let Some(raw) = half_life_days.filter(|s| !s.trim().is_empty()) {
        options.half_life_days = raw.trim().parse().map_err(|_| {
            Error::invalid(
                "half_life_days",
                format!("{ENV_HALF_LIFE_DAYS}={raw:?} is not a number"),
            )
        })?;
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_path_prefers_env() {
        let path = db_path(Some("/x/m.db".into()), Some("/home/u".into())).unwrap();
        assert_eq!(path, PathBuf::from("/x/m.db"));
    }

    #[test]
    fn db_path_defaults_under_home() {
        let path = db_path(Some("".into()), Some("/home/u".into())).unwrap();
        assert_eq!(path, PathBuf::from("/home/u/.munin/munin.db"));
        assert!(db_path(None, None).is_err());
    }

    #[test]
    fn half_life_from_env() {
        assert_eq!(options(None).unwrap().half_life_days, 30.0);
        assert_eq!(options(Some(" 7 ".into())).unwrap().half_life_days, 7.0);
        assert!(options(Some("week".into())).is_err());
    }
}
