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
                         │  LuxDB  │
                         │ (Redis) │
                         └─────────┘
```

## Workspace Crates

| Crate | Path | Description |
|---|---|---|
| `walkietalk-shared` | `crates/shared` | Domain types, IDs, messages, audio codec, JWT helpers, Axum extractors, Redis data-access layer |
| `walkietalk-auth` | `crates/auth-service` | Registration, login, JWT issuance, device management |
| `walkietalk-signaling` | `crates/signaling-service` | WebSocket hub, rooms, floor lock (Redis SET NX EX), presence, metrics, audio relay, ZMQ fan-out |
| `walkietalk-zmq-proxy` | `crates/zmq-proxy` | PULL/PUB fan-out proxy for multi-node signaling |
| `walkietalk-integration-tests` | `crates/integration-tests` | End-to-end tests across services |
| `walkietalk-client` | `client/src-tauri` | Tauri v2 native shell — audio I/O (cpal + Opus), WS client, REST client |

The **client frontend** lives in `client/` (SolidJS + TypeScript + Vite).

## Services

| Service | Default Address | Protocol | Description |
|---|---|---|---|
| Auth | `0.0.0.0:3001` | REST | User registration, login, JWT tokens, device management |
| Signaling | `0.0.0.0:3002` | WebSocket + REST | Rooms, floor lock, presence, metrics, audio relay |
| ZMQ Proxy | `0.0.0.0:5559` / `5560` | ZeroMQ | PULL/PUB fan-out for multi-node audio + signaling |
| LuxDB | `0.0.0.0:6379` | RESP (Redis) | Users, rooms, memberships, floor locks, refresh tokens |

## Prerequisites

- Rust 1.88+ (install via [rustup](https://rustup.rs/))
- Node.js 22+ and npm
- Docker & Docker Compose (recommended for LuxDB and full-stack dev)

For local development without Docker, install [LuxDB](https://github.com/lux-db/lux) or any Redis-compatible server.

## Development Setup

### Option A — Docker Compose (recommended)

Starts LuxDB, Auth, ZMQ Proxy and two Signaling nodes:

```bash
docker compose up --build
```

### Option B — Local

1. Start a LuxDB (or Redis) instance on port 6379.

2. Copy and edit the env file:
   ```bash
   cp .env.example .env
   # Edit .env — set REDIS_URL and JWT_SECRET
   ```

3. Start services (each in its own terminal):
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
| `REDIS_URL` | — | Redis/LuxDB connection string (e.g. `redis://localhost:6379`) |
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

FloorManager and integration tests use [testcontainers](https://crates.io/crates/testcontainers) to spin up a Redis instance automatically — no external database required.

## Benchmarking

Collect performance metrics from running services:

```bash
# Start services with logging off for clean measurements
docker compose -f docker-compose.yml -f docker-compose.bench.yml up -d --build

# Run the Tauri client in release mode
cd client && RUST_LOG=off npm run tauri:build

# Collect metrics (5s interval, 120s duration)
./scripts/bench-collect.sh --skip-docker-up --interval 5 --duration 120

# Generate charts from collected data
python scripts/bench-charts.py bench-results/<timestamp>
```

## Project Layout

```
walkietalk/
├── crates/
│   ├── shared/              # Domain types, messages, audio codec, JWT, Redis DB layer
│   │   └── src/
│   │       └── db.rs        # Redis data-access: users, rooms, devices, floor locks
│   ├── auth-service/        # REST auth service (Axum)
│   ├── signaling-service/   # WebSocket signaling (Axum + ZMQ)
│   │   └── src/
│   │       ├── hub.rs       # WebSocket connection registry
│   │       ├── floor.rs     # Floor lock manager (Redis SET NX EX + DashMap cache)
│   │       ├── metrics.rs   # Atomic counters for WS, audio, floor, room stats
│   │       ├── presence.rs  # Online/offline/speaking presence
│   │       ├── zmq_relay.rs # ZeroMQ PUSH/SUB relay
│   │       ├── ws/          # WebSocket message handlers
│   │       ├── routes/      # REST room/membership + /health + /metrics endpoints
│   │       └── models/      # Domain models
│   ├── zmq-proxy/           # PULL/PUB fan-out proxy
│   └── integration-tests/   # Cross-service E2E tests
├── client/
│   ├── src/                 # SolidJS + TypeScript frontend
│   ├── src-tauri/           # Tauri v2 Rust backend (audio, WS, REST)
│   └── vite.config.ts
├── scripts/                 # Benchmark collection and charting tools
├── docker-compose.yml       # Full-stack local dev environment
├── docker-compose.bench.yml # Benchmark overlay (logging off)
├── Dockerfile               # Multi-stage build (rust:1.88-bookworm)
└── docs/                    # Technical specification
```

## Key Technical Decisions

- **Data store** — [LuxDB](https://github.com/lux-db/lux), a Redis-compatible server written in Rust. Data is modelled as Redis hashes (users, rooms, devices, tokens), sets (room members), and sorted sets (public rooms). Accessed via the `redis` crate with `ConnectionManager` for automatic reconnection.
- **Floor lock** — Redis `SET floor:{room_id} {user_id} NX EX 60` guarantees exactly-one-speaker across all signaling nodes, with a 60-second server-side timeout. A local `DashMap` cache provides zero-cost holder checks on the hot path.
- **Multi-node fan-out** — ZeroMQ PUSH/PUB pattern: each signaling node PUSHes events to the proxy, which PUBs to all subscribers. Scales horizontally by adding more signaling nodes.
- **Audio codec** — Opus via `audiopus`, binary-framed over WebSocket for minimal overhead.
- **Shared types** — The `walkietalk-shared` crate is used by both backend services and the Tauri client, ensuring message/type parity.
- **Observability** — Each signaling node exposes `GET /metrics` with atomic counters for WebSocket connections, audio frames/bytes, floor operations, and room activity.

## License

[Apache License 2.0](LICENSE)
