# Security Policy

## Reporting a vulnerability

Open a private GitHub security advisory at https://github.com/JRub19/CodexBar4Windows/security/advisories/new. Do not file public issues for vulnerabilities until a fix has shipped.

Please include:

- A short description of the issue.
- Steps to reproduce.
- The affected version (commit hash or release tag).
- Impact assessment if you have one.

We aim to acknowledge within 72 hours and to ship a fix within 14 days when feasible. The project is currently maintained by one person, so response times are best effort.

## Supported versions

Until v0.1.0 ships, only the current `main` branch is supported. Once releases begin, this section lists the supported version range.

## Secrets and credentials

CodexBar4Windows is a usage monitor that talks to AI coding providers on your behalf. To do that it stores OAuth refresh tokens, API keys, and browser session cookies for the providers you enable.

Storage policy:

- Refresh tokens and API keys are encrypted with Windows DPAPI in `%APPDATA%\CodexBar4Windows\secrets\` and the Windows Credential Manager. Decryption requires the same user account that wrote them.
- Browser cookies are read with the user's permission and never persisted to disk in plaintext.
- Tokens, cookies, and identifying account fields are never logged. The Rust core uses a `SensitiveString` newtype that fails the audit if a redaction is missed.

If you find a path that violates the storage or logging policy, report it through the advisory link above. We treat that as a vulnerability.

## Browser cookie import

Reading browser cookies on Windows uses CryptUnprotectData to decrypt the per profile cookie store. This is a normal, documented Windows API path. Some endpoint security products flag this behavior. If your environment blocks the read, switch to the manual cookie paste path in Preferences, which never touches the browser data store.

## Telemetry

The app sends no telemetry by default. Opt-in error reports may land in a future beta. Usage analytics will not ship at any point.

## Code signing

Once v0.1.0 ships, installer and EXE artifacts are signed with Authenticode using an OV or EV certificate. SHA 256 checksums are published alongside each release. Until then, SmartScreen will warn on first run of unsigned dev builds. That is expected.
