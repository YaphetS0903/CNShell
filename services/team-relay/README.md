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

Loopback development may create trusted accounts without email verification. A non-loopback
listener refuses to start unless TLS SMTP delivery is configured. The explicit
`CNSHELL_RELAY_ALLOW_UNVERIFIED_ACCOUNTS=1` escape hatch exists only for the loopback-published
container smoke test and must never be set in production.

```bash
npm run check:relay
```

The integration test starts two accounts and two devices, performs workspace invitation and
device challenge authentication, routes signed encrypted terminal frames over WebSocket, replays
a missed frame after reconnect, transfers a bounded control lease, rejects duplicate input, and
verifies that member removal advances the epoch and invalidates the device session. The operations
drill exercises backup/restore failure boundaries, liveness, database readiness and SIGTERM.

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
| `CNSHELL_RELAY_SMTP_HOST` | SMTP server used for account verification delivery |
| `CNSHELL_RELAY_SMTP_PORT` | Optional port; defaults to 465 for TLS or 587 for STARTTLS |
| `CNSHELL_RELAY_SMTP_SECURITY` | `tls` (default) or `starttls`; plaintext SMTP is rejected |
| `CNSHELL_RELAY_SMTP_FROM` | Valid sender mailbox, for example `CNshell <relay@example.com>` |
| `CNSHELL_RELAY_SMTP_USERNAME` | Optional SMTP username; requires exactly one password source |
| `CNSHELL_RELAY_SMTP_PASSWORD` | SMTP password supplied by the runtime secret manager |
| `CNSHELL_RELAY_SMTP_PASSWORD_FILE` | Alternative small regular-file secret; mutually exclusive with the password variable |
| `RUST_LOG` | Metadata-only service logs; request bodies and tokens are never intentionally logged |

Production registration creates an unverified account, stores only a domain-separated SHA-256
hash of the one-hour token, and does not issue an account session until the token is consumed.
Tokens are single-use, resends are atomically limited to one per minute, and resend responses do
not reveal whether an address exists. Existing accounts are marked verified by migration. The
client exposes verification and resend controls without storing the token in SQLite.

The example Compose file publishes the relay only on host loopback so a host reverse proxy can
reach it. `/health` is process liveness, while `/ready` executes a database query and is the
container/load-balancer readiness endpoint. SIGINT/SIGTERM stops new traffic and closes active
team-terminal WebSockets before the server drains.

`docker-compose.production.yml` is the hardened single-host deployment template. It adds a
digest-pinned unprivileged NGINX TLS/WSS proxy, per-IP registration/auth/general rate limits,
WebSocket connection limits, strict public Host matching, private relay/monitoring networks,
Prometheus rules, an NGINX exporter and Alertmanager. The public proxy does not expose `/health`,
`/ready` or `/metrics`; Prometheus and Alertmanager do not publish host ports. Access logs contain
only a generated request ID, method, status, byte count, duration and upstream status.

Before using the production template, prepare an exact public DNS name, a full-chain TLS
certificate and key, an SMTP password file readable by relay UID/GID `10001`, and an Alertmanager
configuration with a real receiver. Copy `production/alertmanager.example.yml` outside the
repository and replace its mandatory placeholder. TLS files must be readable by the unprivileged
NGINX UID/GID `101` without making the private key world-readable. Then provide the absolute host
paths through the required Compose variables and validate before starting:

```bash
docker compose -f services/team-relay/docker-compose.production.yml config --quiet
docker compose -f services/team-relay/docker-compose.production.yml up -d --build
```

The template hard-codes `CNSHELL_RELAY_ALLOW_UNVERIFIED_ACCOUNTS=0`, never publishes port `8787`,
and separates Relay and Alertmanager egress networks. Image versions, manifest digests, licenses
and sources are recorded in `production/THIRD_PARTY.md`. The Linux smoke command below creates
temporary credentials and a self-signed certificate, verifies HTTPS routing/rate limits/log
redaction and Prometheus scraping, and deletes its containers and volumes afterward:

```bash
npm run test:relay-production-config
```

That smoke validates the deployment mechanics only. A real DNS certificate, SMTP delivery,
production Alertmanager receiver and external network remain mandatory production evidence.

## Backup and operations

`scripts/backup.sh` creates and verifies a consistent SQLite snapshot. Production backup requires
an `age` public recipient and never falls back to plaintext. `scripts/restore.sh` verifies the
SHA-256 sidecar, decrypts and checks the database, refuses an existing target, and requires an
explicit confirmation that the relay has stopped.

```bash
npm run test:relay-ops
```

The local drill uses plaintext only behind an explicit development flag unless verified `age`
tools are supplied. `npm run verify:relay-age` verifies the official release Sigsum proof before
extraction, and `npm run test:relay-container` builds and runs the Compose deployment. Both paths
run in CI; neither claims off-host storage or a production restore test. See
[`docs/TEAM_RELAY_OPERATIONS.md`](../../docs/TEAM_RELAY_OPERATIONS.md) for deployment, retention,
restore, monitoring and incident procedures.

Public DNS, certificates, a real SMTP provider and delivery reputation, proxy rate limits,
production backup scheduling, monitoring and incident response remain deployment responsibilities.

## Stored data

- Argon2id account password hashes and short-lived opaque session-token hashes.
- Workspace/member/device metadata, public keys, roles, revocation state and key epoch.
- One-time workspace invitation and device-challenge hashes.
- Room routing metadata, control leases and signed end-to-end encrypted envelopes.
- At most five minutes, 512 frames and 4 MiB of encrypted output replay data per room.
- Metadata-only audit events capped at 4,096 per workspace.

The schema has no columns for terminal plaintext, room content keys, connection credentials, host
addresses, usernames or local paths.
