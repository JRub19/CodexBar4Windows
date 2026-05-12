---
summary: "Step by step GitHub UI walk through to enable branch protection on main."
read_when:
  - Setting up the repository for the first time
  - Confirming CI gates are enforced on main
---

# Branch protection setup

Audience: the repository owner. CodexBar4Windows uses GitHub branch protection to require CI green on `main`. This document walks the UI clicks. We do not enforce protection via the GitHub API at this time.

## Prerequisites

- The repository owner has admin access to `JRub19/CodexBar4Windows`.
- The CI workflow `.github/workflows/ci.yml` has run at least once and shows the job name `ci / windows build` in the Actions tab.

## Steps

1. Open https://github.com/JRub19/CodexBar4Windows/settings/branches in a browser.
2. Click `Add branch protection rule` (or `Add classic branch protection rule` if your account uses the new ruleset UI; the classic rule path is sufficient here).
3. In `Branch name pattern`, type `main`.
4. Tick `Require status checks to pass before merging`.
5. Tick `Require branches to be up to date before merging`.
6. In the status check search box, type `windows build` and select the `ci / windows build` check.
7. Leave `Require a pull request before merging` UNCHECKED for now. The project is a solo maintainer with the `work on main` policy from `CLAUDE.md`. If the team grows beyond one contributor, revisit this toggle.
8. Tick `Do not allow bypassing the above settings` (optional, recommended). This blocks force pushes that skip the rule.
9. Click `Create`.

## Verification

After saving:

- Run a small docs only commit on `main` to confirm the rule fires the CI check.
- Open the Actions tab: the run should show `ci / windows build` as a required check.
- Try a force push from a clone: it should be rejected if `Do not allow bypassing` is ticked. If unticked, this step is optional.

## When to revisit

- When a second contributor lands: turn on `Require a pull request before merging` and require at least one review.
- When the project grows additional CI jobs (Tier 2 nightly, Tier 3 release): add each required job name to the rule.
- When release tags ship: consider a tag protection rule for `v*` to prevent accidental tag deletion.

## Alternative: GitHub rulesets

GitHub also offers the newer `Rulesets` interface at https://github.com/JRub19/CodexBar4Windows/settings/rules. A ruleset can express the same intent with finer scoping (per actor, per tag, per branch pattern). The classic branch protection rule is simpler and sufficient at this phase. Migrate to rulesets later if needed.
