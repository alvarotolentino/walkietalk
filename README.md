# WalkieTalk

Rust-first push-to-talk communication platform.

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![CI](https://github.com/alvarotolentino/walkietalk/actions/workflows/ci.yml/badge.svg)](https://github.com/alvarotolentino/walkietalk/actions/workflows/ci.yml)

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
| `walkietalk-signaling` | `crates/signaling-service` | WebSocket hub, rooms, invite-code join, floor lock (Redis SET NX EX), presence, metrics, audio relay, ZMQ fan-out |
| `walkietalk-zmq-proxy` | `crates/zmq-proxy` | PULL/PUB fan-out proxy for multi-node signaling |
| `walkietalk-integration-tests` | `crates/integration-tests` | End-to-end tests across services |
| `walkietalk-client` | `client/src-tauri` | Tauri v2 native shell — audio I/O (cpal + Opus), WS client, REST client |

The **client frontend** lives in `client/` (SolidJS + TypeScript + Vite).

## Services

| Service | Default Address | Protocol | Description |
|---|---|---|---|
| Auth | `0.0.0.0:3001` | REST | User registration, login, JWT tokens, device management |
| Signaling | `0.0.0.0:3002` | WebSocket + REST | Rooms, invite-code join, floor lock, presence, metrics, audio relay |
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

## REST API

### Auth Service

| Method | Path | Auth | Description |
|---|---|---|---|
| `POST` | `/auth/register` | — | Create user account |
| `POST` | `/auth/login` | — | Authenticate and get JWT + refresh token |
| `POST` | `/auth/refresh` | — | Refresh expired JWT |
| `POST` | `/auth/logout` | — | Revoke refresh tokens |
| `GET` | `/users/me` | JWT | Get authenticated user profile |
| `GET` | `/users/me/devices` | JWT | List user's devices |
| `POST` | `/users/me/devices` | JWT | Register device |
| `DELETE` | `/users/me/devices/:id` | JWT | Delete device |
| `GET` | `/health` | — | Liveness check |

### Signaling Service

| Method | Path | Auth | Description |
|---|---|---|---|
| `POST` | `/rooms` | JWT | Create room (auto-generates invite code) |
| `GET` | `/rooms` | JWT | List user's rooms |
| `GET` | `/rooms/:id` | JWT | Get room details + members |
| `PATCH` | `/rooms/:id` | JWT | Update room (owner only) |
| `DELETE` | `/rooms/:id` | JWT | Delete room (owner only) |
| `POST` | `/rooms/:id/join` | JWT | Join room by ID + invite code |
| `POST` | `/rooms/join` | JWT | Join room by invite code only |
| `POST` | `/rooms/:id/invite` | JWT | Generate new invite code (owner only) |
| `DELETE` | `/rooms/:id/leave` | JWT | Leave room |
| `GET` | `/ws?token=JWT` | JWT | WebSocket upgrade |
| `GET` | `/health` | — | Liveness check |
| `GET` | `/metrics` | — | Atomic counters (requires `metrics` feature) |

## WebSocket Protocol

**Client → Server** (JSON text frames):

| Message | Fields | Description |
|---|---|---|
| `JoinRoom` | `room_id` | Subscribe to room events |
| `LeaveRoom` | `room_id` | Unsubscribe from room |
| `FloorRequest` | `room_id` | Request permission to speak |
| `FloorRelease` | `room_id` | Release the floor |
| `Heartbeat` | `ts` | Keep-alive ping |

**Server → Client** (JSON text frames):

| Message | Key Fields | Description |
|---|---|---|
| `RoomState` | `room_id`, `members`, `floor_holder`, `lock_key` | Full state on join |
| `FloorGranted` | `room_id`, `user_id` | Floor acquired |
| `FloorDenied` | `room_id`, `reason` | Floor request rejected |
| `FloorReleased` | `room_id`, `user_id` | Speaker released floor |
| `FloorTimeout` | `room_id`, `user_id` | 60s server-side timeout |
| `PresenceUpdate` | `room_id`, `user_id`, `status` | Online/Offline/Speaking |
| `MemberJoined` | `room_id`, `user` | New member joined room |
| `MemberLeft` | `room_id`, `user_id` | Member left room |
| `Error` | `code`, `message` | Error response |
| `HeartbeatAck` | `ts` | Keep-alive reply |

**Audio**: Binary WebSocket frames carry Opus-encoded audio during floor hold.

## Testing

### Unit tests (no Redis required)

```bash
cargo test -p walkietalk-shared
cargo test -p walkietalk-zmq-proxy
cargo test -p walkietalk-auth
cargo test -p walkietalk-signaling --lib
```

### Integration tests (require Redis)

The integration tests spin up real Redis containers via [testcontainers](https://crates.io/crates/testcontainers). You also need `REDIS_URL` and `JWT_SECRET` for the signaling-service integration test:

```bash
# Cross-service tests (testcontainers handles Redis automatically)
cargo test -p walkietalk-integration-tests

# Signaling-service integration test (needs a running Redis)
REDIS_URL=redis://localhost:6379 JWT_SECRET=test-secret \
  cargo test -p walkietalk-signaling --test integration_test
```

### Lint

```bash
cargo clippy --workspace --exclude walkietalk-client --exclude walkietalk_client_lib -- -D warnings
cargo fmt --all -- --check
```

### Client type check

```bash
cd client && npx tsc --noEmit
```

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
├── .github/workflows/
│   └── ci.yml                  # CI pipeline: check, unit-tests, integration-tests
├── crates/
│   ├── shared/                 # Domain types, messages, audio codec, JWT, Redis DB layer
│   │   └── src/
│   │       ├── audio.rs        # Binary audio frame wire format (19-byte header)
│   │       ├── auth.rs         # JWT claims, Argon2 password hashing, token encode/decode
│   │       ├── db.rs           # Redis data-access: users, rooms, devices, floor locks
│   │       ├── enums.rs        # Visibility, RoomRole enums
│   │       ├── error.rs        # AppError → HTTP status mapping
│   │       ├── extractors.rs   # Axum JWT extractor (AuthUser)
│   │       ├── ids.rs          # Newtype UUIDs: UserId, RoomId, DeviceId
│   │       └── messages.rs     # ClientMessage / ServerMessage enums (WS protocol)
│   ├── auth-service/           # REST auth service (Axum)
│   │   └── src/
│   │       ├── config.rs       # Env config: REDIS_URL, JWT_SECRET, AUTH_LISTEN_ADDR
│   │       ├── models.rs       # Request/response DTOs (register, login, refresh, etc.)
│   │       ├── state.rs        # AppState: RedisConn + JWT secret
│   │       └── routes/
│   │           ├── auth.rs     # POST /auth/{register,login,refresh,logout}
│   │           ├── users.rs    # GET/POST/DELETE /users/me, /users/me/devices
│   │           └── health.rs   # GET /health
│   ├── signaling-service/      # WebSocket signaling (Axum + ZMQ)
│   │   └── src/
│   │       ├── config.rs       # Env config inc. ZMQ_PUSH_ADDR, ZMQ_SUB_ADDR
│   │       ├── state.rs        # AppState: Redis, Hub, Floor, Presence, ZMQ, Metrics
│   │       ├── hub.rs          # WsHub: lock-free per-room connection registry (DashMap)
│   │       ├── floor.rs        # FloorManager: Redis SET NX EX 60 + DashMap fast-path cache
│   │       ├── presence.rs     # PresenceManager: per-room user status tracking
│   │       ├── metrics.rs      # Feature-gated atomic counters (audio, WS, floor, rooms)
│   │       ├── utils.rs        # Invite code and slug generation
│   │       ├── zmq_relay.rs    # ZeroMQ PUSH/SUB relay for multi-node fan-out
│   │       ├── models/
│   │       │   └── room.rs     # Room DTOs: Create/Join/Update requests, RoomResponse
│   │       ├── routes/
│   │       │   ├── rooms.rs    # CRUD + join + invite + leave endpoints
│   │       │   └── health.rs   # GET /health
│   │       └── ws/
│   │           ├── handler.rs  # GET /ws upgrade with JWT validation
│   │           └── connection.rs # WS loop: message routing, audio relay, presence
│   ├── zmq-proxy/              # PULL/PUB fan-out proxy
│   │   └── src/
│   │       ├── main.rs         # Standalone proxy binary
│   │       └── lib.rs          # run_proxy: stateless frame relay PULL→PUB
│   └── integration-tests/      # Cross-service E2E tests
│       └── tests/
│           ├── common/         # Shared fixtures: Docker containers, service startup
│           ├── auth_tests.rs   # Auth registration, login, refresh, device tests
│           ├── signaling_tests.rs  # Room CRUD, floor control, WS tests
│           └── cross_service_tests.rs  # Auth→Signaling full PTT journey
├── client/
│   ├── src/                    # SolidJS + TypeScript frontend
│   │   ├── App.tsx             # Root app component
│   │   ├── router.ts           # Client-side routing
│   │   ├── screens/
│   │   │   ├── Login.tsx       # User login
│   │   │   ├── Register.tsx    # New account
│   │   │   ├── RoomList.tsx    # Browse rooms
│   │   │   ├── RoomView.tsx    # Active room (PTT, members, floor)
│   │   │   ├── RoomSettings.tsx
│   │   │   ├── CreateRoom.tsx
│   │   │   ├── JoinByCode.tsx  # Invite-code room join
│   │   │   ├── Profile.tsx
│   │   │   └── Splash.tsx
│   │   ├── components/
│   │   │   ├── PttButton.tsx   # Push-to-Talk (hold to transmit)
│   │   │   ├── VuMeter.tsx     # Audio level visualisation
│   │   │   ├── FloorBanner.tsx # Current speaker banner
│   │   │   ├── MemberList.tsx  # Room member roster
│   │   │   ├── PresenceDot.tsx # Online/speaking indicator
│   │   │   ├── ConnectionBar.tsx # Network status
│   │   │   ├── Countdown.tsx   # Floor hold timer
│   │   │   └── ...             # Avatar, Badge, Modal, Toast, Toggle, etc.
│   │   ├── stores/
│   │   │   ├── auth.ts         # User & token state
│   │   │   ├── activeRoom.ts   # Current room, members, floor holder
│   │   │   ├── rooms.ts        # Room list
│   │   │   ├── audio.ts        # Capture/playback state
│   │   │   ├── connection.ts   # WebSocket connection state
│   │   │   └── settings.ts     # User preferences
│   │   ├── hooks/
│   │   │   └── useTauriEvent.ts
│   │   ├── utils/              # format, haptics, sounds
│   │   └── styles/             # global.css, reset.css, tokens.css
│   ├── src-tauri/              # Tauri v2 Rust backend
│   │   └── src/
│   │       ├── lib.rs          # Tauri app builder, command registration
│   │       ├── state.rs        # AppState: user, tokens, active room
│   │       ├── http_client.rs  # JWT-injected reqwest wrapper
│   │       ├── audio/
│   │       │   ├── engine.rs   # AudioEngine main loop, VU meter
│   │       │   ├── capture.rs  # cpal mic → Opus encode → WS send
│   │       │   └── playback.rs # Opus decode → jitter buffer → cpal speaker
│   │       ├── commands/
│   │       │   ├── audio.rs    # start/stop capture/playback
│   │       │   ├── auth.rs     # login, register, logout, refresh
│   │       │   ├── connection.rs # WS connect/disconnect/heartbeat
│   │       │   ├── floor.rs    # request/release floor
│   │       │   ├── rooms.rs    # list/create/join/leave rooms
│   │       │   ├── realtime.rs # event listeners, presence
│   │       │   ├── settings.rs # user preferences
│   │       │   └── misc.rs     # app version, platform info
│   │       └── transport/
│   │           ├── manager.rs  # WS lifecycle, heartbeat (30s), auto-reconnect
│   │           └── ws.rs       # tokio-tungstenite split read/write channels
│   └── vite.config.ts
├── scripts/
│   ├── bench-collect.sh        # Metrics collection during benchmarks
│   ├── bench-charts.py         # Chart generation from collected data
│   └── test-multinode.sh       # Multi-node deployment test
├── docs/                       # Technical specification & diagrams
├── docker-compose.yml          # Full-stack dev: LuxDB, Auth, ZMQ Proxy, 2× Signaling
├── docker-compose.bench.yml    # Benchmark overlay (logging off)
└── Dockerfile                  # Multi-stage build (rust:1.88-bookworm → debian:bookworm-slim)
```

## Key Technical Decisions

- **Data store** — [LuxDB](https://github.com/lux-db/lux), a Redis-compatible server written in Rust. Data is modelled as Redis hashes (users, rooms, devices, tokens), sets (room members), and sorted sets (public rooms). Accessed via the `redis` crate with `ConnectionManager` for automatic reconnection.
- **Invite-code join** — All room joins require a valid invite code. Codes are auto-generated on room creation and can be rotated by the owner via `POST /rooms/:id/invite`. Rooms are capped at 500 members.
- **Floor lock** — Redis `SET floor:{room_id} {user_id} NX EX 60` guarantees exactly-one-speaker across all signaling nodes, with a 60-second server-side timeout. A local `DashMap` cache provides zero-cost holder checks on the hot path.
- **Multi-node fan-out** — ZeroMQ PUSH/PUB pattern: each signaling node PUSHes events to the proxy, which PUBs to all subscribers. Scales horizontally by adding more signaling nodes. If ZMQ addresses are unset, the signaling service runs standalone.
- **Audio codec** — Opus via `audiopus`, binary-framed over WebSocket for minimal overhead.
- **Shared types** — The `walkietalk-shared` crate is used by both backend services and the Tauri client, ensuring message/type parity.
- **Observability** — Each signaling node exposes `GET /metrics` (behind the `metrics` feature flag) with atomic counters for WebSocket connections, audio frames/bytes, floor operations, and room activity.
- **Release profile** — `opt-level=3`, `lto=fat`, `codegen-units=1`, `strip=symbols` for production builds.

## License

[Apache License 2.0](LICENSE)
