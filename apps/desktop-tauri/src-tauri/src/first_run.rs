//! Phase 3 D12 + Phase 8 onboarding: persist the "first run hint
//! shown" flag (tray-pin balloon) plus the multi-step onboarding
//! wizard state (which step the user reached, whether the flow
//! finished). State lives next to `settings.json` in
//! `%APPDATA%\CodexBar4Windows\state.json`.
//!
//! Onboarding steps mirror `docs/windows/plan/phase-8` Task 21:
//!
//! - `Welcome` — initial hello.
//! - `Providers` — provider picker.
//! - `SignIn` — per-provider sign-in.
//! - `Done` — terminal step, sets `onboarding_completed = true`.
//!
//! When `onboarding_completed` is true, the popup hides the wizard
//! shell. The About-pane "Run onboarding again" button calls
//! `onboarding_reset` which sets `onboarding_completed = false` and
//! rewinds `onboarding_step` to `Welcome`. Provider settings are
//! preserved across resets.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStep {
    #[default]
    Welcome,
    Providers,
    SignIn,
    Done,
}

impl OnboardingStep {
    pub fn advance(self) -> Self {
        match self {
            OnboardingStep::Welcome => OnboardingStep::Providers,
            OnboardingStep::Providers => OnboardingStep::SignIn,
            OnboardingStep::SignIn => OnboardingStep::Done,
            OnboardingStep::Done => OnboardingStep::Done,
        }
    }

    pub fn rewind(self) -> Self {
        match self {
            OnboardingStep::Welcome => OnboardingStep::Welcome,
            OnboardingStep::Providers => OnboardingStep::Welcome,
            OnboardingStep::SignIn => OnboardingStep::Providers,
            OnboardingStep::Done => OnboardingStep::SignIn,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FirstRunState {
    #[serde(default)]
    pub tray_pinned_hint_shown: bool,
    #[serde(default)]
    pub onboarding_completed: bool,
    #[serde(default)]
    pub onboarding_step: OnboardingStep,
}

#[derive(Clone)]
pub struct FirstRunStore {
    path: PathBuf,
}

impl FirstRunStore {
    pub fn new(state_dir: impl AsRef<Path>) -> Self {
        Self {
            path: state_dir.as_ref().join("state.json"),
        }
    }

    pub fn read(&self) -> FirstRunState {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => FirstRunState::default(),
        }
    }

    /// Atomic write: write to a temp file in the same directory, then
    /// rename over the target so partial writes never corrupt state.
    pub fn write(&self, state: &FirstRunState) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(tmp, &self.path)?;
        Ok(())
    }

    pub fn mark_tray_pinned_hint_shown(&self) -> std::io::Result<()> {
        let mut s = self.read();
        s.tray_pinned_hint_shown = true;
        self.write(&s)
    }

    pub fn advance_onboarding(&self) -> std::io::Result<FirstRunState> {
        let mut s = self.read();
        s.onboarding_step = s.onboarding_step.advance();
        if matches!(s.onboarding_step, OnboardingStep::Done) {
            s.onboarding_completed = true;
        }
        self.write(&s)?;
        Ok(s)
    }

    pub fn rewind_onboarding(&self) -> std::io::Result<FirstRunState> {
        let mut s = self.read();
        s.onboarding_step = s.onboarding_step.rewind();
        // Backing up out of Done un-completes the flow.
        s.onboarding_completed = false;
        self.write(&s)?;
        Ok(s)
    }

    pub fn complete_onboarding(&self) -> std::io::Result<FirstRunState> {
        let mut s = self.read();
        s.onboarding_completed = true;
        s.onboarding_step = OnboardingStep::Done;
        self.write(&s)?;
        Ok(s)
    }

    pub fn reset_onboarding(&self) -> std::io::Result<FirstRunState> {
        let mut s = self.read();
        s.onboarding_completed = false;
        s.onboarding_step = OnboardingStep::Welcome;
        self.write(&s)?;
        Ok(s)
    }

    pub fn clear(&self) -> std::io::Result<()> {
        self.write(&FirstRunState::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_state_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = FirstRunStore::new(dir.path());
        assert!(!store.read().tray_pinned_hint_shown);
        store.mark_tray_pinned_hint_shown().unwrap();
        assert!(store.read().tray_pinned_hint_shown);
        store.clear().unwrap();
        assert!(!store.read().tray_pinned_hint_shown);
    }

    #[test]
    fn onboarding_advances_through_all_four_steps() {
        let dir = tempfile::tempdir().unwrap();
        let store = FirstRunStore::new(dir.path());

        let s0 = store.read();
        assert_eq!(s0.onboarding_step, OnboardingStep::Welcome);
        assert!(!s0.onboarding_completed);

        let s1 = store.advance_onboarding().unwrap();
        assert_eq!(s1.onboarding_step, OnboardingStep::Providers);
        assert!(!s1.onboarding_completed);

        let s2 = store.advance_onboarding().unwrap();
        assert_eq!(s2.onboarding_step, OnboardingStep::SignIn);
        assert!(!s2.onboarding_completed);

        // Landing on Done flips onboarding_completed.
        let s3 = store.advance_onboarding().unwrap();
        assert_eq!(s3.onboarding_step, OnboardingStep::Done);
        assert!(s3.onboarding_completed);

        // Re-advancing past Done is a no-op.
        let s4 = store.advance_onboarding().unwrap();
        assert_eq!(s4.onboarding_step, OnboardingStep::Done);
        assert!(s4.onboarding_completed);
    }

    #[test]
    fn onboarding_rewind_un_completes_the_flow() {
        let dir = tempfile::tempdir().unwrap();
        let store = FirstRunStore::new(dir.path());

        store.complete_onboarding().unwrap();
        assert!(store.read().onboarding_completed);

        let s = store.rewind_onboarding().unwrap();
        assert_eq!(s.onboarding_step, OnboardingStep::SignIn);
        assert!(!s.onboarding_completed);

        // Rewinding off the front clamps at Welcome.
        let _ = store.rewind_onboarding().unwrap();
        let _ = store.rewind_onboarding().unwrap();
        let s = store.rewind_onboarding().unwrap();
        assert_eq!(s.onboarding_step, OnboardingStep::Welcome);
    }

    #[test]
    fn onboarding_reset_preserves_other_flags() {
        let dir = tempfile::tempdir().unwrap();
        let store = FirstRunStore::new(dir.path());
        store.mark_tray_pinned_hint_shown().unwrap();
        store.complete_onboarding().unwrap();
        let s = store.reset_onboarding().unwrap();
        assert!(s.tray_pinned_hint_shown); // preserved
        assert!(!s.onboarding_completed);
        assert_eq!(s.onboarding_step, OnboardingStep::Welcome);
    }
}
