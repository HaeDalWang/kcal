# Contributing to kcal

Tauri 2 + React 19 + TypeScript desktop app. Releases are macOS only; the Windows code is built in CI but not released yet. User-facing docs are in [README.md](README.md). Design decisions are recorded in [`docs/decisions/`](docs/decisions/), and the next tasks are in [`docs/backlog.md`](docs/backlog.md). Agent rules for this repo are in `AGENTS.md`. UI rules are in [`docs/design.md`](docs/design.md).

> Windows: on GitHub Actions (`windows-latest`), `pnpm check` (types, lint, format, Vitest, clippy, Rust tests) and `tauri build` (msi, nsis) pass. The app has only been run, with screens and IPC checked, on macOS (arm64).

## Requirements

| Item    | Version / notes                                                                                                    |
| ------- | ------------------------------------------------------------------------------------------------------------------ |
| Node.js | 24.x (same as CI)                                                                                                  |
| pnpm    | 12.15.0 (pinned by the `packageManager` field, Corepack works)                                                     |
| Rust    | stable 1.90 or newer (verified with 1.99.1), `rustup component add clippy rustfmt`                                 |
| macOS   | Xcode Command Line Tools (`xcode-select --install`)                                                                |
| Windows | Microsoft C++ Build Tools (MSVC, "Desktop development with C++"), WebView2 Runtime (preinstalled on Windows 10/11) |

Don't install the Tauri CLI globally. Use the devDependency `@tauri-apps/cli` through `pnpm tauri ...`.

Regenerate icons with `pnpm tauri icon docs/assets/icon.svg` (this rewrites `src-tauri/icons/`; delete the android and ios output). The macOS menu bar template icon is described in `docs/assets/tray.svg`. Agent icons in the popover tabs are inline SVG in `src/components/agent-icon.tsx`.

## Setup

```sh
pnpm install
```

Windows: clone with `git clone -c core.symlinks=true …` (needs Developer Mode or an admin shell). Without it, `.claude/skills` and `.kiro/skills` check out as small text files and the project skills don't load. Run `pnpm check` from Git Bash (its PATH has `sh`): the `installer:test` step calls `sh`, which a plain cmd or PowerShell PATH doesn't have.

## Commands

| Command                             | What it does                                                                                   |
| ----------------------------------- | ---------------------------------------------------------------------------------------------- |
| `pnpm tauri:dev`                    | Run the desktop app in dev mode (Vite dev server + Rust debug build)                           |
| `pnpm tauri:dev:mcp`                | Same, with the dev-only MCP bridge enabled (see MCP below)                                     |
| `pnpm tauri:build`                  | Release build and bundles                                                                      |
| `pnpm dev` / `pnpm build`           | Run / build the frontend only (`dist/`)                                                        |
| `pnpm typecheck`                    | `tsc --noEmit`                                                                                 |
| `pnpm lint`                         | ESLint (flat config, typescript-eslint, react-hooks)                                           |
| `pnpm format` / `pnpm format:check` | Apply / check Prettier                                                                         |
| `pnpm test`                         | Vitest (jsdom, Testing Library), single run                                                    |
| `pnpm rust:check`                   | In `src-tauri`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` |
| `pnpm check`                        | typecheck, lint, format:check, test, rust:check, all of them                                   |

Add a shadcn component: `pnpm dlx shadcn@4.23.0 add <component>` (config in `components.json`, alias `@/`).

## Layout

```
index.html              # loads theme-init.js first, then React
public/theme-init.js    # applies the theme class before first render (no flash)
src/
  App.tsx               # popover shell
  components/           # popover, dashboard, checks/coach views, shadcn components in ui/
  lib/
    ipc.ts              # typed wrappers for Rust commands and events; invoke/listen are used only here
    i18n-en.ts          # English UI strings (Korean is the other language)
  test/                 # test setup and fixtures
src-tauri/
  src/lib.rs            # Builder, command registration, MCP bridge (conditional)
  src/usage/            # limit lookup: parsers, per-provider I/O, paths, timers
  src/context/          # startup context measurement
  src/rate/, metrics/   # rate fits and per-request metrics from session logs
  src/coach/            # checks, switches, Claude handoff, backups and undo
  src/health/           # periodic checkup
  src/tray.rs, tray/    # tray icon, menu, popover placement
  src/settings.rs       # settings.json load, validation, save
  src/i18n.rs           # UI language for Rust-made strings
  src/error.rs          # AppError (serialized as { kind, message })
  capabilities/default.json
  tauri.conf.json       # app config, CSP
  tauri.mcp.conf.json   # dev-only overlay (MCP)
docs/decisions/         # decision records
docs/assets/            # banner and icons (svg sources + png)
.agents/skills/         # agent skills (.claude/skills is a symlink); sources in .agents/skills/SOURCES.md
.mcp.json, .codex/      # MCP server config for Claude Code / Codex
```

## IPC

All Rust commands validate input and return `Result<T, E>` with a serializable error. The frontend calls only the typed wrappers in `src/lib/ipc.ts`. When you change a contract, change the Rust type, the TS type, `ipc.ts` and the tests in the same change. Tokens never leave Rust. Values are refreshed by Rust timers (Claude and Codex every 5 minutes, Kiro every 10, with backoff on 429) and pushed to the UI as events; the frontend does not poll.

## MCP (development only)

Uses [hypothesi/mcp-server-tauri](https://github.com/hypothesi/mcp-server-tauri) (rationale: `docs/decisions/0001-initial-stack.md`).

1. Start the app with `pnpm tauri:dev:mcp`. The log shows `WebSocket server listening on: 127.0.0.1:9224`.
2. Use the `tauri` server from Claude Code (`.mcp.json`) or Codex (`.codex/config.toml`). The server is pinned to `@hypothesi/tauri-mcp-server@0.13.2`.
   - Codex reads `.codex/config.toml` only if this repo's own path is trusted. Trust on a parent directory (for example `~`) is not inherited. Approve trust the first time you open `codex` interactively in the repo, or add `[projects."<absolute repo path>"] trust_level = "trusted"` to `~/.codex/config.toml`. Check: `codex mcp list` shows `tauri`.
3. Ask the agent to run `driver_session` start, then `webview_screenshot`, `webview_interact`, `ipc_execute_command`, and so on.

From a terminal (the CLI is pinned too):

```sh
npx -y -p @hypothesi/tauri-mcp-cli@0.13.2 tauri-mcp driver-session start --raw '{"action":"start","port":9224}'   # without a port it attaches to 9223 (another app)
npx -y -p @hypothesi/tauri-mcp-cli@0.13.2 tauri-mcp ipc-execute-command --raw '{"command":"get_usage"}'
npx -y -p @hypothesi/tauri-mcp-cli@0.13.2 tauri-mcp driver-session stop
npx -y -p @hypothesi/tauri-mcp-cli@0.13.2 tauri-mcp daemon stop
```

What the `tauri.mcp.conf.json` overlay does:

- `app.withGlobalTauri: true` (the bridge uses `window.__TAURI__`).
- `app.security.capabilities` is set to `["default", { inline capability "mcp-dev": "mcp-bridge:default" }]`. Once this array is set, only the listed capabilities are active, so `"default"` is listed too. The default `capabilities/` has no bridge permission, so a build without the feature doesn't break.

Limit: the bridge injects a helper script into the page, so it doesn't work in bundled builds with CSP on. macOS `tauri dev` has no CSP, so use it only with `tauri:dev:mcp`.

### Why MCP is not in production builds

- `tauri-plugin-mcp-bridge` is an optional dependency enabled only by the `mcp` feature. `pnpm tauri:build` doesn't pass the feature.
- Registration is inside `#[cfg(all(debug_assertions, feature = "mcp"))]`. In release builds (`debug_assertions` off) it isn't registered even if the feature is on.
- The bind address is fixed to `127.0.0.1` (the plugin default is `0.0.0.0`).

To verify:

```sh
pnpm tauri:build --bundles app
strings src-tauri/target/release/bundle/macos/kcal.app/Contents/MacOS/kcal | grep -ci mcp_bridge   # 0
(cd src-tauri && cargo tree -e normal | grep -c mcp-bridge)                                                           # 0
lsof -nP -iTCP:9224 -sTCP:LISTEN   # while dev:mcp runs: 127.0.0.1:9224
```

## Security

- Capabilities: `capabilities/default.json` gives the popover and dashboard windows `core:default` only. App commands need no separate permission, and no Tauri plugins are used.
- CSP: `app.security.csp` in `tauri.conf.json` allows `'self'` only, plus `connect-src ipc: http://ipc.localhost` for IPC. Tauri adds inline script hashes and style nonces at build time. Never set `"csp": null`.
- The theme-flash script is a file (`public/theme-init.js`), not inline, so `script-src 'self'` allows it without relaxing CSP.
- macOS `tauri dev` opens the Vite server directly, so CSP doesn't apply there. Check CSP behavior against the `tauri:build` output. `devCsp` is a relaxed policy for platforms that proxy the dev server (untested on Windows).
- `withGlobalTauri` is false by default (true only in the MCP overlay).
- Don't put secrets in code or config. See [docs/privacy.md](docs/privacy.md) for what the app reads and sends.

## Build output

- macOS: `src-tauri/target/release/bundle/macos/kcal.app` (`--bundles app`; `--bundles dmg` for the dmg)
- Windows (CI build verified): `src-tauri/target/release/bundle/msi/`, `nsis/`
- Release profile: `lto = true`, `codegen-units = 1`, `opt-level = "s"`, `strip = true`, `panic = "abort"`
- Releases are built by `.github/workflows/release.yml` on a `v*` tag or manual run, ad-hoc signed only ([decision 0020](docs/decisions/0020-unsigned-macos-release.md)).

## CI (`.github/workflows/ci.yml`)

| Trigger                          | OS              | Why                                             |
| -------------------------------- | --------------- | ----------------------------------------------- |
| `main` push                      | Windows         | Only the platform that can't be checked locally |
| PR, manual (`Run workflow`)      | macOS + Windows | Check both platforms before merge               |
| Only `*.md` or `docs/**` changed | skipped         |                                                 |

Each job runs `pnpm check`, then `pnpm tauri:build`, then uploads the bundle (kept 7 days). GitHub-hosted runners bill Windows minutes at 2x and macOS at 10x of Linux. Run macOS manually just before a release.

## Platform differences

- macOS uses the default title bar (no custom title bar).
- Font stack: `system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`.
