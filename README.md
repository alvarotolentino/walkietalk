# WalkieTalk

Rust-first push-to-talk communication platform.

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## Architecture

```
┌──────────────┐         ┌─────────────┐
│ Tauri Client │◄──WSS──►│  Signaling  │──┐
│ SolidJS + RS │         │  Service ×N │  │ ZMQ PUSH
└──────────────┘         └─────────────┘  │
                                ▲          ▼
                           REST │    ┌───────────┐
                                │    │ ZMQ Proxy │ PULL → PUB
                                │    └───────────┘
                         ┌──────┴──┐       │
                         │  Auth   │       │ ZMQ SUB
                         │ Service │  ┌────┴──────┐
                         └────┬────┘  │ Signaling │
                              │       │ Service ×N│
                         ┌────┴────┐  └───────────┘
                         │ Postgres│
                         │   16    │
                         └─────────┘
```

## Workspace Crates

| Crate | Path | Description |
|---|---|---|
| `walkietalk-shared` | `crates/shared` | Domain types, IDs, messages, audio codec, JWT helpers, Axum extractors |
| `walkietalk-auth` | `crates/auth-service` | Registration, login, JWT issuance, device management |
| `walkietalk-signaling` | `crates/signaling-service` | WebSocket hub, rooms, floor lock (PG advisory locks), presence, audio relay, ZMQ fan-out |
| `walkietalk-zmq-proxy` | `crates/zmq-proxy` | PULL/PUB fan-out proxy for multi-node signaling |
| `walkietalk-integration-tests` | `crates/integration-tests` | End-to-end tests across services |
| `walkietalk-client` | `client/src-tauri` | Tauri v2 native shell — audio I/O (cpal + Opus), WS client, REST client |

The **client frontend** lives in `client/` (SolidJS + TypeScript + Vite).

## Services

| Service | Default Address | Protocol | Description |
|---|---|---|---|
| Auth | `0.0.0.0:3001` | REST | User registration, login, JWT tokens, device management |
| Signaling | `0.0.0.0:3002` | WebSocket + REST | Rooms, floor lock, presence, audio relay |
| ZMQ Proxy | `0.0.0.0:5559` / `5560` | ZeroMQ | PULL/PUB fan-out for multi-node audio + signaling |
| PostgreSQL | `0.0.0.0:5432` | TCP | Users, rooms, memberships, advisory locks |

## Prerequisites

- Rust 1.88+ (install via [rustup](https://rustup.rs/))
- PostgreSQL 16+
- Node.js 22+ and npm
- sqlx-cli
- Docker & Docker Compose (optional, for containerised dev)

## Development Setup

### Option A — Docker Compose (recommended)

Starts PostgreSQL, Auth, ZMQ Proxy and two Signaling nodes:

```bash
docker compose up --build
```

### Option B — Local

1. Install sqlx-cli:
   ```bash
   cargo install sqlx-cli --no-default-features --features postgres
   ```

2. Copy and edit the env file:
   ```bash
   cp .env.example .env
   # Edit .env with your PostgreSQL credentials
   ```

3. Create the database and run migrations:
   ```bash
   sqlx database create
   sqlx migrate run
   ```

4. Start services (each in its own terminal):
   ```bash
   cargo run -p walkietalk-auth
   cargo run -p walkietalk-zmq-proxy
   cargo run -p walkietalk-signaling
   ```

### Client

```bash
cd client
npm install
npm run tauri dev
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | — | PostgreSQL connection string |
| `JWT_SECRET` | — | HMAC secret for JWT signing |
| `AUTH_LISTEN_ADDR` | `0.0.0.0:3001` | Auth service bind address |
| `SIGNALING_LISTEN_ADDR` | `0.0.0.0:3002` | Signaling service bind address |
| `ZMQ_PULL_ADDR` | `tcp://0.0.0.0:5559` | ZMQ proxy PULL socket |
| `ZMQ_PUB_ADDR` | `tcp://0.0.0.0:5560` | ZMQ proxy PUB socket |
| `ZMQ_PUSH_ADDR` | `tcp://127.0.0.1:5559` | Signaling → proxy PUSH address |
| `ZMQ_SUB_ADDR` | `tcp://127.0.0.1:5560` | Signaling ← proxy SUB address |

## Testing

```bash
# Unit & integration tests
cargo test --workspace

# Clippy lint check
cargo clippy --all-targets --all-features -- -D warnings

# Client type check
cd client && npx tsc --noEmit
```

Signaling FloorManager tests use [testcontainers](https://crates.io/crates/testcontainers) to spin up a PostgreSQL instance automatically — no external database required.

## Project Layout

```
walkietalk/
├── crates/
│   ├── shared/              # Domain types, messages, audio codec, JWT
│   ├── auth-service/        # REST auth service (Axum)
│   ├── signaling-service/   # WebSocket signaling (Axum + ZMQ)
│   │   └── src/
│   │       ├── hub.rs       # WebSocket connection registry
│   │       ├── floor.rs     # Floor lock manager (PG advisory locks)
│   │       ├── presence.rs  # Online/offline/speaking presence
│   │       ├── zmq_relay.rs # ZeroMQ PUSH/SUB relay
│   │       ├── ws/          # WebSocket message handlers
│   │       ├── routes/      # REST room/membership endpoints
│   │       └── models/      # DB models
│   ├── zmq-proxy/           # PULL/PUB fan-out proxy
│   └── integration-tests/   # Cross-service E2E tests
├── client/
│   ├── src/                 # SolidJS + TypeScript frontend
│   ├── src-tauri/           # Tauri v2 Rust backend (audio, WS, REST)
│   └── vite.config.ts
├── migrations/              # sqlx PostgreSQL migrations
├── scripts/                 # Helper scripts (multi-node testing)
├── docker-compose.yml       # Full-stack local dev environment
├── Dockerfile               # Multi-stage build (rust:1.88-bookworm)
└── docs/                    # Technical specification
```

## Key Technical Decisions

- **Floor lock** — PostgreSQL advisory locks guarantee exactly-one-speaker across all signaling nodes, with a 60-second server-side timeout.
- **Multi-node fan-out** — ZeroMQ PUSH/PUB pattern: each signaling node PUSHes events to the proxy, which PUBs to all subscribers. Scales horizontally by adding more signaling nodes.
- **Audio codec** — Opus via `audiopus`, binary-framed over WebSocket for minimal overhead.
- **Shared types** — The `walkietalk-shared` crate is used by both backend services and the Tauri client, ensuring message/type parity.

## License

[Apache License 2.0](LICENSE)
