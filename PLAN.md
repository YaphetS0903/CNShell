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
- [ ] Auto-update channel.
- [ ] Crash/error reporting boundary.

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
- [ ] Add auto-update channel.
