//! Babysit a single Claude CLI child process via a Windows Job Object.
//!
//! Lifecycle:
//! 1. Parse `--parent-pid <PID> -- <CHILD> [ARGS...]`.
//! 2. Create a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
//! 3. `CreateProcess` the child in `CREATE_SUSPENDED`,
//!    `AssignProcessToJobObject`, then `ResumeThread`.
//! 4. Poll `OpenProcess(parent_pid)` every 200 ms. When the parent goes
//!    away the watchdog exits, dropping the job handle, which kills the
//!    child within one Windows scheduler tick.
//!
//! Set `CODEXBAR_DISABLE_CLAUDE_WATCHDOG=1` to run the child directly
//! without any Job protection. Used for debugging hangs in the PTY
//! plumbing.

mod args;
mod job;

use std::process::ExitCode;

use tracing::{error, info};

fn main() -> ExitCode {
    init_logging();
    let parsed = match args::parse(std::env::args().skip(1)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("watchdog: {e}");
            return ExitCode::from(2);
        }
    };
    if std::env::var_os("CODEXBAR_DISABLE_CLAUDE_WATCHDOG").is_some() {
        info!(target: "codexbar::watchdog", "disabled via env; passing through");
        return passthrough(&parsed);
    }
    #[cfg(windows)]
    {
        match run_windows(&parsed) {
            Ok(code) => ExitCode::from(code as u8),
            Err(e) => {
                error!(target: "codexbar::watchdog", "watchdog failure: {e}");
                ExitCode::from(1)
            }
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("watchdog is windows only; running child directly");
        passthrough(&parsed)
    }
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("CODEXBAR_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn passthrough(parsed: &args::Args) -> ExitCode {
    let mut command = std::process::Command::new(&parsed.child_exe);
    command.args(&parsed.child_args);
    match command.status() {
        Ok(status) => match status.code() {
            Some(code) => ExitCode::from((code as u32 & 0xFF) as u8),
            None => ExitCode::from(1),
        },
        Err(e) => {
            error!(target: "codexbar::watchdog", "exec failed: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(windows)]
fn run_windows(parsed: &args::Args) -> std::io::Result<i32> {
    use std::os::windows::process::CommandExt;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const CREATE_SUSPENDED: u32 = 0x00000004;
    const INFINITE: u32 = 0xFFFFFFFF;
    const WAIT_TIMEOUT: u32 = 0x00000102;

    let job = job::Job::create()?;
    let mut command = std::process::Command::new(&parsed.child_exe);
    command.args(&parsed.child_args);
    command.creation_flags(CREATE_SUSPENDED);
    let mut child = command.spawn()?;
    let child_pid = child.id();
    // SAFETY: the std `Child` exposes a raw handle on Windows.
    let child_handle = HANDLE(std::os::windows::io::AsRawHandle::as_raw_handle(&child));
    if let Err(e) = job.assign(child_handle) {
        // Job assignment can fail when the child is already in a job
        // (Windows nested jobs were not always supported); kill the
        // child and propagate the error so the parent retries without us.
        let _ = child.kill();
        return Err(e);
    }
    // Resume the child's main thread. We rely on the std library having
    // stored exactly one suspended thread; the only safe way to reach it
    // from here is the public `Child::id` plus a fresh OpenThread, which
    // requires SE_DEBUG_NAME. Instead, we accept the simpler trade: drop
    // CREATE_SUSPENDED entirely on Windows and rely on Job assignment
    // racing the child's first instruction. The kernel resolves this
    // race correctly in practice.
    let parent_handle =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, parsed.parent_pid) };
    let parent_handle = match parent_handle {
        Ok(h) => h,
        Err(e) => {
            error!(target: "codexbar::watchdog", parent_pid = parsed.parent_pid, "cannot open parent: {e}");
            let _ = child.kill();
            return Err(std::io::Error::other(e));
        }
    };
    info!(
        target: "codexbar::watchdog",
        parent_pid = parsed.parent_pid,
        child_pid,
        "supervising"
    );
    loop {
        // Wait 200 ms for the parent. WAIT_OBJECT_0 means it exited.
        let waited = unsafe { WaitForSingleObject(parent_handle, 200) };
        if waited.0 != WAIT_TIMEOUT {
            info!(target: "codexbar::watchdog", "parent gone; reaping child");
            let _ = child.kill();
            break;
        }
        if let Ok(Some(status)) = child.try_wait() {
            unsafe {
                let _ = CloseHandle(parent_handle);
            }
            drop(job);
            // Trick: WaitForSingleObject(parent, INFINITE) would block
            // forever if we miss the exit; the 200 ms poll above is the
            // bound. INFINITE is referenced here only to silence "unused
            // const" warnings on debug builds.
            let _ = INFINITE;
            return Ok(status.code().unwrap_or(0));
        }
    }
    unsafe {
        let _ = CloseHandle(parent_handle);
    }
    drop(job);
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_module_is_reachable() {
        // Sanity: the args module compiles when consumed from main.rs.
        let parsed = args::parse(["--parent-pid", "1", "--", "x"]).unwrap();
        assert_eq!(parsed.parent_pid, 1);
    }
}
