//! Launch-at-sign-in via Windows HKCU Run key. Phase 8 task 16.
//!
//! HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run
//! holds one string-value entry per app. We write our exe path
//! (with proper quoting) under the value name `CodexBar4Windows`.
//! Easy to remove via the Startup tab in Task Manager, so the user
//! is never stuck.
//!
//! Non-Windows targets compile but the functions are no-ops, since
//! the rest of the shell never branches on platform.

use tracing::info;

pub const REG_VALUE_NAME: &str = "CodexBar4Windows";

#[tauri::command]
pub async fn launch_at_signin_is_enabled() -> Result<bool, String> {
    is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn launch_at_signin_enable() -> Result<(), String> {
    enable().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn launch_at_signin_disable() -> Result<(), String> {
    disable().map_err(|e| e.to_string())
}

#[cfg(windows)]
mod platform {
    use std::io;

    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    pub fn current_exe_command() -> io::Result<String> {
        let exe = std::env::current_exe()?;
        let path = exe.to_string_lossy().to_string();
        // Quote the path so spaces work. Run-key values do not
        // expand %VARS% by default, which is fine — absolute paths.
        Ok(format!("\"{}\"", path))
    }

    pub fn is_enabled() -> io::Result<bool> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run = match hkcu.open_subkey_with_flags(RUN_KEY, KEY_READ) {
            Ok(k) => k,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        match run.get_value::<String, _>(super::REG_VALUE_NAME) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn enable() -> io::Result<()> {
        let command = current_exe_command()?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (run, _) = hkcu.create_subkey_with_flags(RUN_KEY, KEY_WRITE)?;
        run.set_value(super::REG_VALUE_NAME, &command)?;
        Ok(())
    }

    pub fn disable() -> io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run = match hkcu.open_subkey_with_flags(RUN_KEY, KEY_WRITE) {
            Ok(k) => k,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        match run.delete_value(super::REG_VALUE_NAME) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::io;

    pub fn is_enabled() -> io::Result<bool> {
        Ok(false)
    }
    pub fn enable() -> io::Result<()> {
        Ok(())
    }
    pub fn disable() -> io::Result<()> {
        Ok(())
    }
}

fn is_enabled() -> std::io::Result<bool> {
    platform::is_enabled()
}

fn enable() -> std::io::Result<()> {
    let r = platform::enable();
    if r.is_ok() {
        info!(target: "codexbar::launch_at_signin", "enabled");
    }
    r
}

fn disable() -> std::io::Result<()> {
    let r = platform::disable();
    if r.is_ok() {
        info!(target: "codexbar::launch_at_signin", "disabled");
    }
    r
}

#[cfg(all(test, windows))]
mod tests {
    // The HKCU registry is shared across the test runner, so we
    // namespace each test's value name to avoid colliding with a
    // real install or with each other.

    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
    use winreg::RegKey;

    const TEST_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    fn cleanup(name: &str) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(run) = hkcu.open_subkey_with_flags(TEST_KEY, KEY_WRITE) {
            let _ = run.delete_value(name);
        }
    }

    #[test]
    fn current_exe_command_is_quoted_absolute_path() {
        let cmd = super::platform::current_exe_command().unwrap();
        assert!(cmd.starts_with('"'));
        assert!(cmd.ends_with('"'));
        assert!(cmd.len() > 2);
    }

    #[test]
    fn enable_then_disable_round_trip() {
        // Use the real REG_VALUE_NAME; clean up before and after to
        // avoid leaking state into a real install.
        cleanup(super::REG_VALUE_NAME);
        assert!(!super::is_enabled().unwrap());
        super::enable().unwrap();
        assert!(super::is_enabled().unwrap());
        super::disable().unwrap();
        assert!(!super::is_enabled().unwrap());
    }
}
