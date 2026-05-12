//! Command-line argument parsing for the watchdog. The shape is:
//!
//!     codexbar4windows-claude-watchdog --parent-pid <PID> -- <CHILD> [ARGS...]
//!
//! `--parent-pid` is required so the watchdog can poll the parent and
//! exit when it goes away. Everything after the `--` separator is the
//! child command line; we forward it verbatim to `CreateProcess`.

use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Args {
    pub parent_pid: u32,
    pub child_exe: String,
    pub child_args: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArgsError {
    #[error("missing required --parent-pid")]
    MissingParentPid,
    #[error("invalid --parent-pid value '{0}'")]
    InvalidParentPid(String),
    #[error("missing -- separator before child command")]
    MissingSeparator,
    #[error("missing child executable after --")]
    MissingChildExe,
}

pub fn parse<I, S>(input: I) -> Result<Args, ArgsError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let raw: Vec<String> = input.into_iter().map(Into::into).collect();
    let mut iter = raw.into_iter();
    let mut parent_pid: Option<u32> = None;
    let mut found_separator = false;
    let mut child: Vec<String> = Vec::new();
    while let Some(token) = iter.next() {
        if token == "--" {
            found_separator = true;
            child.extend(iter.by_ref());
            break;
        }
        if let Some(value) = token.strip_prefix("--parent-pid=") {
            parent_pid = Some(
                value
                    .parse()
                    .map_err(|_| ArgsError::InvalidParentPid(value.into()))?,
            );
            continue;
        }
        if token == "--parent-pid" {
            let value = iter
                .next()
                .ok_or_else(|| ArgsError::InvalidParentPid(String::new()))?;
            parent_pid = Some(
                value
                    .parse()
                    .map_err(|_| ArgsError::InvalidParentPid(value.clone()))?,
            );
            continue;
        }
    }
    let parent_pid = parent_pid.ok_or(ArgsError::MissingParentPid)?;
    if !found_separator {
        return Err(ArgsError::MissingSeparator);
    }
    let mut child_iter = child.into_iter();
    let child_exe = child_iter.next().ok_or(ArgsError::MissingChildExe)?;
    let child_args: Vec<String> = child_iter.collect();
    Ok(Args {
        parent_pid,
        child_exe,
        child_args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_separated_command() {
        let parsed = parse(["--parent-pid", "1234", "--", "cmd", "/c", "timeout", "100"]).unwrap();
        assert_eq!(
            parsed,
            Args {
                parent_pid: 1234,
                child_exe: "cmd".into(),
                child_args: vec!["/c".into(), "timeout".into(), "100".into()],
            }
        );
    }

    #[test]
    fn parses_equals_form() {
        let parsed = parse(["--parent-pid=42", "--", "claude"]).unwrap();
        assert_eq!(parsed.parent_pid, 42);
        assert_eq!(parsed.child_exe, "claude");
    }

    #[test]
    fn missing_parent_pid_errors() {
        assert_eq!(parse(["--", "claude"]), Err(ArgsError::MissingParentPid));
    }

    #[test]
    fn invalid_parent_pid_errors() {
        assert!(matches!(
            parse(["--parent-pid", "not-a-pid", "--", "x"]),
            Err(ArgsError::InvalidParentPid(_))
        ));
    }

    #[test]
    fn missing_separator_errors() {
        assert_eq!(
            parse(["--parent-pid", "1"]),
            Err(ArgsError::MissingSeparator)
        );
    }

    #[test]
    fn missing_child_exe_errors() {
        assert_eq!(
            parse(["--parent-pid", "1", "--"]),
            Err(ArgsError::MissingChildExe)
        );
    }
}
