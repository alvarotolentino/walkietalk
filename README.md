# WalkieTalk

Rust-first push-to-talk communication platform.

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- PostgreSQL 16+
- sqlx-cli

## Development Setup

1. Install sqlx-cli:
   ```bash
   cargo install sqlx-cli --no-default-features --features postgres
   ```

2. Create the database:
   ```bash
   cp .env.example .env
   # Edit .env with your PostgreSQL credentials
   sqlx database create
   sqlx migrate run
   ```

3. Run the Auth Service:
   ```bash
   cargo run -p walkietalk-auth
   ```

## Services

| Service | Default Address | Description |
|---|---|---|
| Auth | `0.0.0.0:3001` | User registration, login, JWT, device management |
