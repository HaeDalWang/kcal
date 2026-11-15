# Security

## Reporting a vulnerability

Please report it privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability** (private vulnerability reporting). Don't open a public issue for it.

Include what you did, what you expected, and what happened. I'll reply as soon as I can; kcal is a one-person side project, so there is no guaranteed response time.

## In scope

- Reads and writes of local files outside what the [privacy table](README.md#privacy-and-what-it-reads) lists.
- Handling of the `Claude Code-credentials` Keychain item and the Codex token (storing, logging or sending them anywhere but the usage endpoints).
- Weakening of the webview's CSP or of the Tauri capabilities (`src-tauri/tauri.conf.json`, `src-tauri/capabilities/`).
- The `install.sh` download and checksum steps.

## Out of scope

- kcal is ad-hoc signed and not notarized. That is documented in the README and is not a vulnerability.
- Anything that needs an attacker who can already run code as your user or write to your home folder.

## Bounty

There is no bounty. I'm glad to credit you in the fix if you want.
