# CNshell Team Relay

`cnshell-team-relay` is the server-side authorization and encrypted routing service for CNshell
team workspaces. It does not receive room keys, connection credentials, terminal plaintext, or
passwords other than account passwords submitted over the authenticated TLS account endpoint.

## Local development

```bash
CNSHELL_RELAY_DATABASE_URL='sqlite://relay.sqlite?mode=rwc' \
  cargo run --manifest-path services/team-relay/Cargo.toml
```

The default listener is `127.0.0.1:8787`. A non-loopback bind is rejected unless
`CNSHELL_RELAY_BEHIND_TLS_PROXY=1` is set.

```bash
npm run check:relay
```

The integration test starts two accounts and two devices, performs workspace invitation and
device challenge authentication, routes signed encrypted terminal frames over WebSocket, replays
a missed frame after reconnect, transfers a bounded control lease, rejects duplicate input, and
verifies that member removal advances the epoch and invalidates the device session.

## Production boundary

The binary intentionally does not terminate TLS. Deploy it behind a reverse proxy or managed load
balancer that provides TLS 1.2+, redirects HTTP to HTTPS, preserves WebSocket upgrades, limits
request rates and body sizes, and does not log `Authorization` headers or request bodies. Bind the
container port to loopback or a private service network; never expose port 8787 directly.

Required runtime settings:

| Variable | Purpose |
| --- | --- |
| `CNSHELL_RELAY_DATABASE_URL` | SQLite URL on an encrypted, backed-up persistent volume |
| `CNSHELL_RELAY_BIND` | Listener, normally `0.0.0.0:8787` inside a private container network |
| `CNSHELL_RELAY_BEHIND_TLS_PROXY=1` | Explicit acknowledgement required for a non-loopback bind |
| `RUST_LOG` | Metadata-only service logs; request bodies and tokens are never intentionally logged |

The example Compose file publishes the relay only on host loopback so a host reverse proxy can
reach it. Public DNS, certificates, email delivery, backup retention, monitoring and incident
response remain deployment responsibilities.

## Stored data

- Argon2id account password hashes and short-lived opaque session-token hashes.
- Workspace/member/device metadata, public keys, roles, revocation state and key epoch.
- One-time workspace invitation and device-challenge hashes.
- Room routing metadata, control leases and signed end-to-end encrypted envelopes.
- At most five minutes, 512 frames and 4 MiB of encrypted output replay data per room.
- Metadata-only audit events capped at 4,096 per workspace.

The schema has no columns for terminal plaintext, room content keys, connection credentials, host
addresses, usernames or local paths.

