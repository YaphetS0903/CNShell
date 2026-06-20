# CNshell Development Plan

CNshell is a desktop SSH and server operations client inspired by FinalShell's integrated SSH/SFTP/monitoring workflow and Xshell's professional terminal productivity features. This project will implement comparable workflows from scratch and will not copy proprietary code, branding, icons, UI assets, or unique text from closed-source products.

## Source Research Summary

- FinalShell public feature surface: SSH client, Windows remote desktop entry points, multi-tab sessions, SFTP side-by-side with terminal directory sync, command suggestions/history, dynamic parameters, quick commands, built-in editor, local/remote port forwarding, proxy support, compression, ZMODEM, no-agent server monitoring, cloud sync, acceleration, and intranet tunneling.
- FinalShell public technical hints: update notes mention Java 17, JediTerm, and JSch usage. These are useful as directional clues, not implementation constraints.
- Xshell public feature surface: Session Manager, Tab Manager, Compose Pane, synchronized input, Quick Commands, Highlight Sets, master password, key mappings, X11 forwarding, scripting/recording/triggers, Jump Host Proxy, Instant Tunneling, remote file manager, SCP, OpenSSH CA support, local CMD/PowerShell/WSL sessions, Telnet, Serial, RDP, and log viewer.
- CNshell technical direction: start with Electron + React + TypeScript for fast desktop iteration, use xterm.js for terminal rendering, Node/Electron main process for SSH/SFTP/PTY integrations, and keep IPC boundaries explicit for security.

## Product Principles

- Operational density first: show hosts, terminal, file transfer, and monitoring without marketing-style layout.
- Safe by default: verify host keys, isolate renderer privileges, protect credentials, and warn on dangerous bulk commands.
- Familiar terminal behavior: keyboard-first, predictable focus, stable tab management, searchable output, configurable shortcuts.
- Extensible architecture: SSH, SFTP, RDP, tunneling, monitoring, and sync should remain separate service modules.

## MVP Milestones

- [x] ~~Create a trackable project plan document.~~
- [x] ~~Scaffold Electron + React + TypeScript desktop app.~~
- [x] ~~Create core workspace layout: connection sidebar, tab strip, terminal area, SFTP panel, monitoring panel.~~
- [x] ~~Add typed domain models for connections, sessions, quick commands, transfers, and metrics.~~
- [x] ~~Add secure Electron IPC boundary through preload APIs.~~
- [x] ~~Render a first terminal surface with xterm.js.~~
- [x] ~~Add local connection storage abstraction.~~
- [x] ~~Add first SSH connection service boundary.~~
- [x] ~~Implement connect/disconnect lifecycle in the UI.~~
- [x] ~~Implement SFTP remote file listing.~~
- [x] ~~Implement basic upload/download transfer queue.~~
- [x] ~~Implement no-agent server metrics collection.~~
- [x] ~~Implement Quick Commands and command palette.~~
- [x] ~~Implement session tabs and reconnect states.~~
- [x] ~~Implement logging and terminal search.~~

## Xshell-Inspired Workflow Milestones

- [x] ~~Compose Pane for multi-line command drafting.~~
- [x] ~~Synchronized input across selected sessions.~~
- [x] ~~Highlight Sets for terminal output rules.~~
- [x] ~~Trigger rules for output matching and automatic actions.~~
- [x] ~~Instant local/remote/dynamic SSH tunneling controls.~~
- [x] ~~Jump Host Proxy and chained gateway configuration.~~
- [x] ~~Key mapping profiles.~~
- [x] ~~Safe Paste review for multiline or risky commands.~~
- [x] ~~Script recording and replay.~~
- [x] ~~Log viewer with filtering.~~

## FinalShell-Inspired Workflow Milestones

- [x] ~~Terminal and SFTP same-directory synchronization.~~
- [x] ~~Remote file editor flow.~~
- [x] ~~Process manager.~~
- [x] ~~Disk/network/process monitoring charts.~~
- [x] ~~ZMODEM upload/download.~~
- [x] ~~Windows RDP connection entries.~~
- [x] ~~Cloud sync for encrypted settings.~~
- [x] ~~CN Relay service for acceleration and intranet tunneling.~~

## Security Milestones

- [x] ~~Host key verification and known_hosts management.~~
- [x] ~~System-protected encrypted credential storage for secrets.~~
- [x] ~~Master password option.~~
- [x] ~~Private key import with passphrase support.~~
- [x] ~~IPC input validation.~~
- [x] ~~Audit logging with secret redaction.~~
- [x] ~~Bulk command confirmation.~~

## Engineering Milestones

- [x] ~~Unit tests for connection models and storage.~~
- [x] ~~IPC contract tests.~~
- [x] ~~Renderer component tests for core workflow states.~~
- [x] ~~Windows installer packaging.~~
- [x] ~~macOS packaging.~~
- [x] ~~Linux packaging.~~
- [x] ~~Auto-update channel.~~
- [x] ~~Crash/error reporting boundary.~~

## Phase 2 Productization Milestones

- [x] ~~Add connection search filtering by host, group, name, username, and tags.~~
- [x] ~~Add connection create/edit/delete workflow with persisted workspace updates.~~
- [x] ~~Wire topbar shortcuts to focus the tunneling manager and credential vault panels.~~
- [x] ~~Add quick command create/edit/delete workflow.~~
- [x] ~~Add terminal split-pane workspace behavior.~~
- [x] ~~Add terminal actions menu for logs, ZMODEM, reconnect, and clear guidance.~~
- [x] ~~Add collapsible connection groups with persisted in-memory state.~~
- [x] ~~Add inline validation and recovery messages for connection and command forms.~~
- [x] ~~Expand renderer tests for connection management, quick command management, and terminal toolbar actions.~~
- [x] ~~Re-run full typecheck, lint, test, and packaged app smoke verification.~~

## Phase 3 Professional Workflow Milestones

- [x] ~~Implement real split terminal sessions backed by independent xterm instances.~~
- [x] ~~Add session tab close workflow with active-tab fallback and terminal cleanup.~~
- [x] ~~Add SFTP create directory operation.~~
- [x] ~~Add SFTP rename operation.~~
- [x] ~~Add SFTP delete operation with confirmation and refresh.~~
- [x] ~~Add application icon assets and wire them into Electron windows and packaged builds.~~
- [x] ~~Add package author metadata and reduce packaging warnings.~~
- [x] ~~Expand IPC validation and renderer component tests for the new workflows.~~
- [x] ~~Re-run full typecheck, lint, test, package, and GitHub push.~~

## Phase 4 FinalShell-Style File Workspace Sprint

- [x] ~~Re-check FinalShell's visible SSH + SFTP workflow and map it to CNshell without copying proprietary code or assets.~~
- [x] ~~Audit the current SFTP backend and renderer state flow.~~
- [x] ~~Move SFTP out of the narrow right operations panel and into a bottom workspace under the terminal.~~
- [x] ~~Add a directory tree, path bar, refresh/history-style actions, upload/download controls, and a dense file table.~~
- [x] ~~Keep direct file operations visible: open/edit, rename, delete, create directory, and transfer status.~~
- [x] ~~Embed the remote text editor into the bottom file workspace so a selected file can be edited immediately.~~
- [x] ~~Automatically refresh the file panel after SSH connects and when the synced current directory changes.~~
- [x] ~~Update renderer tests for the FinalShell-style file table/tree workflow.~~
- [x] ~~Re-run typecheck, lint, tests, package, commit, and push.~~

## Phase 5 FinalShell-Style Server Status Sprint

- [x] ~~Harden the last-session close path so CNshell never reopens to a black workspace.~~
- [x] ~~Add a FinalShell-style server status rail with IP, uptime, load, CPU, memory, swap, processes, network, latency, and filesystems.~~
- [x] ~~Add a clickable System Information workspace tab for OS, kernel, CPU, memory, swap, network, filesystems, and processes.~~
- [x] ~~Auto-refresh status data after SSH connects and keep the visible rail in sync.~~
- [x] ~~Update tests for workspace recovery and the new status/system-info components.~~
- [x] ~~Re-run typecheck, lint, tests, package, commit, and push.~~

## Theme And Connection Stabilization Sprint

- [x] ~~Add light/dark theme switching in Preferences, defaulting to a brighter UI.~~
- [x] ~~Re-tokenize major workspace, sidebar, panel, dialog, table, and form surfaces for both themes.~~
- [x] ~~Improve SSH first-connect host key guidance so the user sees what to do instead of a vague connection failure.~~
- [x] ~~Harden SSH failure cleanup so failed attempts do not leave stale connecting sessions.~~
- [x] ~~Re-run typecheck, lint, tests, package, commit, and push.~~

## Connection Endpoint And Credential Panel Fix Sprint

- [x] ~~Normalize host fields that include a port, such as `124.223.19.107:22`, before saving and connecting.~~
- [x] ~~Fix duplicated endpoint display like `host:22:22` in the sidebar, topbar, and terminal startup path.~~
- [x] ~~Make the light-theme connection editor inputs readable.~~
- [x] ~~Reduce the right SSH panel to connection status, connect action, host-key trust, and a collapsed advanced credential area.~~
- [x] ~~Add regression tests for host/port normalization and the editor split behavior.~~

## SSH Connectivity Diagnostics And Operations Drawer Sprint

- [x] ~~Add a TCP reachability preflight before starting SSH authentication.~~
- [x] ~~Replace vague SSH timeouts with Chinese guidance for security group, firewall, sshd, and port checks.~~
- [x] ~~Remove the permanent right-side SSH login panel from the main workspace.~~
- [x] ~~Move operations panels into an on-demand drawer opened from toolbar actions.~~
- [x] ~~Keep SSH credentials managed from the left connection editor and show host-key trust in a modal dialog.~~
- [x] ~~Verify the reported server endpoint with a local TCP test.~~
- [x] ~~Re-run typecheck, lint, tests, package, commit, and push.~~

## FinalShell Parity SSH Compatibility Sprint

- [x] ~~Treat FinalShell's successful connection as proof that local TCP probes can be false negatives.~~
- [x] ~~Remove the blocking TCP reachability preflight from CNshell's SSH path.~~
- [x] ~~Let ssh2 perform the real SSH handshake and report the actual authentication or host-key result.~~
- [x] ~~Increase SSH ready timeout to 60 seconds and align the renderer startup timeout.~~
- [x] ~~Enable keyboard-interactive password fallback for servers that behave like FinalShell/JSch targets.~~
- [x] ~~Update stale terminal guidance that referenced the removed right-side credential panel.~~
- [x] ~~Re-run typecheck, lint, tests, package, commit, and push.~~

## Real SSH Credential Verification Sprint

- [x] ~~Read the user's SSH credential file locally without exposing secrets in responses.~~
- [x] ~~Verify the server with raw ssh2 and a remote `whoami/hostname` command.~~
- [x] ~~Verify CNshell's `connectSshClient` backend path with temporary known_hosts storage.~~
- [x] ~~Verify CNshell's `TerminalSessionManager` can open an interactive SSH shell and read remote output.~~
- [x] ~~Auto-trust first-seen host keys while still blocking changed host keys.~~
- [x] ~~Re-run typecheck, lint, tests, package, commit, and push.~~

## Desktop Preload Bridge Fix Sprint

- [x] ~~Trace the UI timeout to a missing renderer-to-main `window.cnshell` bridge instead of SSH itself.~~
- [x] ~~Build Electron preload as CommonJS `preload.cjs` so packaged Electron loads it reliably.~~
- [x] ~~Point the main BrowserWindow at the CommonJS preload file.~~
- [x] ~~Show an explicit desktop bridge error instead of a misleading SSH timeout when preload is unavailable.~~
- [x] ~~Migrate legacy localStorage workspace data into the main-process workspace store after the bridge is restored.~~
- [x] ~~Open the packaged app and verify `window.cnshell` plus terminal SSH startup through the UI bridge.~~
- [x] ~~Re-run typecheck, lint, tests, package, commit, and push.~~

## Current Sprint

- [x] ~~Create `PLAN.md` and initial milestone checklist.~~
- [x] ~~Create desktop application scaffold.~~
- [x] ~~Build first static CNshell workspace.~~
- [x] ~~Add typed seed data and UI state.~~
- [x] ~~Verify TypeScript and production build.~~
- [x] ~~Mark completed sprint items in this document.~~
- [x] ~~Add terminal session IPC contracts.~~
- [x] ~~Add `node-pty` and `ssh2` dependencies.~~
- [x] ~~Wire local shell session output into xterm.js.~~
- [x] ~~Smoke test Electron desktop startup.~~
- [x] ~~Add SSH connection service implementation.~~
- [x] ~~Wire SSH session output into xterm.js.~~
- [x] ~~Add host key verification for SSH sessions.~~
- [x] ~~Add secure credential storage instead of session-only credentials.~~
- [x] ~~Persist workspace connections and sessions through Electron userData storage.~~
- [x] ~~Implement SFTP remote directory listing through ssh2.~~
- [x] ~~Implement basic SFTP upload/download transfer queue.~~
- [x] ~~Implement no-agent SSH metrics collection.~~
- [x] ~~Implement Quick Commands execution and command palette.~~
- [x] ~~Implement session tab creation and reconnect action.~~
- [x] ~~Implement terminal output logging and search.~~
- [x] ~~Upgrade Compose Pane to multiline command drafting.~~
- [x] ~~Implement synchronized input toggle and multi-session command dispatch.~~
- [x] ~~Implement terminal output highlight rules and toggle.~~
- [x] ~~Implement output trigger detection and recent trigger panel.~~
- [x] ~~Implement local, remote, and dynamic SSH tunneling controls.~~
- [x] ~~Implement jump host proxy and chained gateway configuration.~~
- [x] ~~Implement key mapping profiles and terminal shortcut dispatch.~~
- [x] ~~Implement Safe Paste review for multiline or risky commands.~~
- [x] ~~Implement script recording and replay.~~
- [x] ~~Implement log viewer with filtering.~~
- [x] ~~Implement terminal and SFTP same-directory synchronization.~~
- [x] ~~Implement remote file editor flow.~~
- [x] ~~Implement process manager.~~
- [x] ~~Implement disk/network/process monitoring charts.~~
- [x] ~~Implement ZMODEM upload/download controls.~~
- [x] ~~Implement Windows RDP connection entries.~~
- [x] ~~Implement cloud sync import/export for encrypted settings.~~
- [x] ~~Implement CN Relay service for acceleration and intranet tunneling.~~
- [x] ~~Add optional master password vault mode.~~
- [x] ~~Add private key import with passphrase support.~~
- [x] ~~Add IPC input validation.~~
- [x] ~~Add audit logging with secret redaction.~~
- [x] ~~Add bulk command confirmation.~~
- [x] ~~Add unit tests for connection models and storage.~~
- [x] ~~Add IPC contract tests.~~
- [x] ~~Add renderer component tests for core workflow states.~~
- [x] ~~Add Windows installer packaging.~~
- [x] ~~Add macOS packaging.~~
- [x] ~~Add Linux packaging.~~
- [x] ~~Add auto-update channel.~~
- [x] ~~Add crash/error reporting boundary.~~
