# Development tools

## Toolchain (verified in the development container on 2026-10-12)

| Tool | Version | Used for |
|------|---------|----------|
| Rust (cargo, clippy, rustfmt) | 1.97.0 | backend crates; MSRV is 1.82 |
| rust-analyzer | installed (`/root/.cargo/bin`) | LSP for the rust-analyzer-lsp plugin |
| Node.js / npm | 22.22.0 / 10.9.4 | phone PWA |
| Chromium for Playwright | `/opt/pw-browsers` (build 1194) | browser tests; never run `playwright install` in this container |
| g++ | system | one-off layout check of Cemu's packed DSU structs |

CI also builds and tests on `windows-latest`.

## Claude Code plugins

Every plugin requested in the project brief exists in the official marketplace (`claude-plugins-official`,
checked against its `marketplace.json`). Project settings (`.claude/settings.json`) register that marketplace and
enable the plugins below for local development, where Claude Code asks each developer to trust them.

| Plugin | Status | Responsibility |
|--------|--------|----------------|
| `rust-analyzer-lsp` | enabled in project settings | Rust navigation and diagnostics |
| `typescript-lsp` | enabled in project settings | TS/React navigation and diagnostics (needs `typescript-language-server` on PATH) |
| `commit-commands` | enabled in project settings | scoped commits / PR helpers. Never stage unrelated files. |
| `security-guidance` | enabled in project settings | flags risky input handling and injection patterns while editing |
| `playwright` | enabled in project settings | interactive browser checks of phone layouts (runs `npx @playwright/mcp`) |
| `frontend-design` | enabled in project settings | controller UI polish; authentic Wii look takes priority over its defaults |
| `context7` | enabled in project settings | current library docs (Tauri, axum, WebRTC). Needs network access to `mcp.context7.com`. |
| `github` | **not enabled** | needs a `GITHUB_PERSONAL_ACCESS_TOKEN` and access to `api.githubcopilot.com`. Cloud sessions use the built-in GitHub MCP tools instead. |
| `figma` | later | for controller design work (Milestone 4+). The Figma MCP was unreachable from the cloud container. |
| `superpowers`, `code-review`, `code-simplifier`, `claude-md-management` | not enabled yet | to be re-evaluated when PR volume grows. Until then, reviews use the built-in `/code-review` and `/security-review`. |

### Cloud session limits (observed 2026-10-12)

- The egress proxy blocks `api.githubcopilot.com`, `mcp.context7.com` and `mcp.figma.com`, so the `github`,
  `context7` and Figma MCP servers cannot connect from the cloud container. The built-in GitHub MCP integration
  works and handled repository and PR operations.
- Plugin installation changes Claude Code's own configuration, and a new install only loads in the next session.
  Project-level enablement (above) is the reviewable way to share the setup.

## Specialised agents used

| Agent | Scope | Isolation |
|-------|-------|-----------|
| Lead (this session) | Rust backend, emulator research, integration, docs, CI | `feat/virtual-wiimote` |
| Mobile interface agent | `apps/phone` Wii Remote PWA + tests | its own git worktree and branch, merged by the lead after review |
