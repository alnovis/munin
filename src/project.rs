use std::path::Path;

use crate::error::{Error, Result};
use crate::model::{MAX_NAME_CHARS, is_name_char, normalize_name};

/// Picks the current project: an explicit name, then `from_env`
/// (`MUNIN_PROJECT`), then the directory. Explicit names must already be
/// valid; directory names are sanitized.
pub fn resolve(explicit: Option<&str>, from_env: Option<&str>, dir: &Path) -> Result<String> {
    if let Some(name) = explicit {
        return normalize_name("project", name);
    }
    if let Some(name) = from_env.filter(|s| !s.trim().is_empty()) {
        return normalize_name("project", name);
    }
    from_dir(dir)
}

/// Names a project after the git root that contains `dir`, or after `dir`
/// itself outside git.
pub fn from_dir(dir: &Path) -> Result<String> {
    let root = dir
        .ancestors()
        .find(|d| d.join(".git").exists())
        .unwrap_or(dir);
    root.file_name()
        .and_then(|name| name.to_str())
        .and_then(sanitize)
        .ok_or_else(|| {
            Error::invalid(
                "project",
                format!(
                    "cannot derive a project name from {}; pass the project explicitly",
                    root.display()
                ),
            )
        })
}

/// Lower-cases a directory name and turns every run of characters that are
/// not allowed in names into a single '-'.
pub fn sanitize(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    for c in raw.trim().chars().flat_map(char::to_lowercase) {
        let c = if is_name_char(c) { c } else { '-' };
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
    }
    let out: String = out.trim_matches('-').chars().take(MAX_NAME_CHARS).collect();
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_directory_names() {
        assert_eq!(sanitize("scry").as_deref(), Some("scry"));
        assert_eq!(
            sanitize("TrustTunnelClient").as_deref(),
            Some("trusttunnelclient")
        );
        assert_eq!(sanitize("my.repo (copy)").as_deref(), Some("my-repo-copy"));
        assert_eq!(sanitize("..."), None);
    }

    #[test]
    fn explicit_beats_env_beats_dir() {
        let dir = Path::new("/nowhere/from-dir");
        assert_eq!(resolve(Some("Arg"), Some("env"), dir).unwrap(), "arg");
        assert_eq!(resolve(None, Some("env"), dir).unwrap(), "env");
        assert_eq!(resolve(None, Some("  "), dir).unwrap(), "from-dir");
        assert!(matches!(
            resolve(Some("bad name"), None, dir),
            Err(Error::Invalid {
                field: "project",
                ..
            })
        ));
    }

    #[test]
    fn git_root_names_the_project() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("My.Repo");
        let nested = repo.join("src").join("deep");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir(repo.join(".git")).unwrap();

        assert_eq!(from_dir(&nested).unwrap(), "my-repo");
    }

    #[test]
    fn root_directory_has_no_name() {
        assert!(from_dir(Path::new("/")).is_err());
    }
}
