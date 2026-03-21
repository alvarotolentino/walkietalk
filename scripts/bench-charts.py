#!/usr/bin/env python
"""Generate performance charts from WalkieTalk benchmark CSVs."""

import os
import sys
import pandas as pd
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
from pathlib import Path

# ── resolve input / output dirs ──────────────────────────────────────────────
if len(sys.argv) > 1:
    data_dir = Path(sys.argv[1])
else:
    data_dir = Path("bench-results/20260320_170854")

out_dir = data_dir / "charts"
out_dir.mkdir(parents=True, exist_ok=True)

plt.rcParams.update({
    "figure.figsize": (14, 5),
    "axes.grid": True,
    "grid.alpha": 0.3,
    "font.size": 10,
    "axes.titlesize": 13,
    "axes.labelsize": 11,
})

def parse_ts(df):
    df["timestamp"] = pd.to_datetime(df["timestamp"])
    df["elapsed_min"] = (df["timestamp"] - df["timestamp"].iloc[0]).dt.total_seconds() / 60
    return df

# ── 1. Client process ────────────────────────────────────────────────────────
client = parse_ts(pd.read_csv(data_dir / "client_process.csv"))

fig, axes = plt.subplots(2, 2, figsize=(16, 10))
fig.suptitle("Tauri Client Process Metrics", fontweight="bold", fontsize=15)

ax = axes[0, 0]
ax.plot(client["elapsed_min"], client["cpu_sec"], color="#2563eb", linewidth=1.5)
ax.set_title("Cumulative CPU Time")
ax.set_ylabel("CPU seconds")
ax.set_xlabel("Elapsed (min)")

ax = axes[0, 1]
ax.plot(client["elapsed_min"], client["working_set_mb"], label="Working Set", color="#16a34a", linewidth=1.5)
ax.plot(client["elapsed_min"], client["private_mem_mb"], label="Private Memory", color="#dc2626", linewidth=1.5, linestyle="--")
ax.set_title("Memory Usage")
ax.set_ylabel("MB")
ax.set_xlabel("Elapsed (min)")
ax.legend()

ax = axes[1, 0]
ax.plot(client["elapsed_min"], client["threads"], color="#9333ea", linewidth=1.5)
ax.set_title("Thread Count")
ax.set_ylabel("Threads")
ax.set_xlabel("Elapsed (min)")

ax = axes[1, 1]
ax.plot(client["elapsed_min"], client["handles"], color="#ea580c", linewidth=1.5)
ax.set_title("Handle Count")
ax.set_ylabel("Handles")
ax.set_xlabel("Elapsed (min)")

fig.tight_layout(rect=[0, 0, 1, 0.95])
fig.savefig(out_dir / "01_client_process.png", dpi=150)
plt.close(fig)
print(f"  -> {out_dir / '01_client_process.png'}")

# ── 2. Docker container resource usage ───────────────────────────────────────
docker = parse_ts(pd.read_csv(data_dir / "docker_stats.csv"))
containers = docker["container"].unique()

# Color map for containers
cmap = {
    "walkietalk-signaling-1-1": "#2563eb",
    "walkietalk-signaling-2-1": "#7c3aed",
    "walkietalk-postgres-1":    "#dc2626",
    "walkietalk-auth-1":        "#16a34a",
    "walkietalk-zmq-proxy-1":   "#ea580c",
}

# 2a. CPU %
fig, axes = plt.subplots(1, 2, figsize=(16, 5))
fig.suptitle("Docker Container Resources", fontweight="bold", fontsize=15)

ax = axes[0]
for c in containers:
    df = docker[docker["container"] == c]
    ax.plot(df["elapsed_min"], df["cpu_pct"], label=c.replace("walkietalk-", ""),
            color=cmap.get(c, "#666"), linewidth=1.2)
ax.set_title("CPU Usage (%)")
ax.set_ylabel("CPU %")
ax.set_xlabel("Elapsed (min)")
ax.legend(fontsize=8)

# 2b. Memory MB
ax = axes[1]
for c in containers:
    df = docker[docker["container"] == c]
    ax.plot(df["elapsed_min"], df["mem_usage_mb"], label=c.replace("walkietalk-", ""),
            color=cmap.get(c, "#666"), linewidth=1.2)
ax.set_title("Memory Usage (MB)")
ax.set_ylabel("MB")
ax.set_xlabel("Elapsed (min)")
ax.legend(fontsize=8)

fig.tight_layout(rect=[0, 0, 1, 0.93])
fig.savefig(out_dir / "02_docker_resources.png", dpi=150)
plt.close(fig)
print(f"  -> {out_dir / '02_docker_resources.png'}")

# 2c. Network I/O
fig, ax = plt.subplots(figsize=(14, 5))
fig.suptitle("Docker Network I/O (Cumulative)", fontweight="bold", fontsize=15)
for c in containers:
    df = docker[docker["container"] == c]
    label = c.replace("walkietalk-", "")
    ax.plot(df["elapsed_min"], df["net_rx_mb"], label=f"{label} RX",
            color=cmap.get(c, "#666"), linewidth=1.2)
    ax.plot(df["elapsed_min"], df["net_tx_mb"], label=f"{label} TX",
            color=cmap.get(c, "#666"), linewidth=1.2, linestyle="--")
ax.set_ylabel("MB")
ax.set_xlabel("Elapsed (min)")
ax.legend(fontsize=7, ncol=2)
fig.tight_layout(rect=[0, 0, 1, 0.93])
fig.savefig(out_dir / "03_docker_network.png", dpi=150)
plt.close(fig)
print(f"  -> {out_dir / '03_docker_network.png'}")

# ── 3. Signaling-1 application metrics ───────────────────────────────────────
sig1 = parse_ts(pd.read_csv(data_dir / "signaling_1_metrics.csv"))

# 3a. WebSocket connections + audio
fig, axes = plt.subplots(2, 2, figsize=(16, 10))
fig.suptitle("Signaling-1 Application Metrics", fontweight="bold", fontsize=15)

ax = axes[0, 0]
ax.plot(sig1["elapsed_min"], sig1["ws_connections_active"], color="#2563eb", linewidth=1.5, label="Active")
ax.plot(sig1["elapsed_min"], sig1["ws_connections_opened"], color="#16a34a", linewidth=1.2, linestyle="--", label="Opened (cum)")
ax.plot(sig1["elapsed_min"], sig1["ws_connections_closed"], color="#dc2626", linewidth=1.2, linestyle=":", label="Closed (cum)")
ax.set_title("WebSocket Connections")
ax.set_ylabel("Count")
ax.set_xlabel("Elapsed (min)")
ax.legend()

ax = axes[0, 1]
ax.plot(sig1["elapsed_min"], sig1["audio_frames_relayed"], color="#ea580c", linewidth=1.5)
ax.set_title("Audio Frames Relayed (Cumulative)")
ax.set_ylabel("Frames")
ax.set_xlabel("Elapsed (min)")

# Compute per-interval rates
sig1["audio_frames_rate"] = sig1["audio_frames_relayed"].diff().fillna(0)
sig1["audio_bytes_rate"] = sig1["audio_bytes_relayed"].diff().fillna(0)
sig1["interval_sec"] = sig1["uptime_secs"].diff().fillna(1)
sig1["audio_fps"] = sig1["audio_frames_rate"] / sig1["interval_sec"]
sig1["audio_kbps"] = (sig1["audio_bytes_rate"] * 8 / 1000) / sig1["interval_sec"]

ax = axes[1, 0]
ax.plot(sig1["elapsed_min"], sig1["audio_fps"], color="#9333ea", linewidth=1.2)
ax.set_title("Audio Frame Rate (frames/sec)")
ax.set_ylabel("frames/s")
ax.set_xlabel("Elapsed (min)")

ax = axes[1, 1]
ax.plot(sig1["elapsed_min"], sig1["audio_kbps"], color="#0891b2", linewidth=1.2)
ax.set_title("Audio Throughput (kbps)")
ax.set_ylabel("kbps")
ax.set_xlabel("Elapsed (min)")

fig.tight_layout(rect=[0, 0, 1, 0.95])
fig.savefig(out_dir / "04_signaling1_audio.png", dpi=150)
plt.close(fig)
print(f"  -> {out_dir / '04_signaling1_audio.png'}")

# 3b. Floor arbitration + room activity
fig, axes = plt.subplots(1, 2, figsize=(16, 5))
fig.suptitle("Signaling-1 Floor & Room Activity", fontweight="bold", fontsize=15)

ax = axes[0]
ax.plot(sig1["elapsed_min"], sig1["floor_requests"], label="Requests", color="#2563eb", linewidth=1.5)
ax.plot(sig1["elapsed_min"], sig1["floor_grants"], label="Grants", color="#16a34a", linewidth=1.5, linestyle="--")
ax.plot(sig1["elapsed_min"], sig1["floor_releases"], label="Releases", color="#ea580c", linewidth=1.5, linestyle=":")
ax.plot(sig1["elapsed_min"], sig1["floor_denials"], label="Denials", color="#dc2626", linewidth=1.5, linestyle="-.")
ax.set_title("Floor Arbitration (Cumulative)")
ax.set_ylabel("Count")
ax.set_xlabel("Elapsed (min)")
ax.legend()

ax = axes[1]
ax.plot(sig1["elapsed_min"], sig1["room_joins"], label="Joins", color="#16a34a", linewidth=1.5)
ax.plot(sig1["elapsed_min"], sig1["room_leaves"], label="Leaves", color="#dc2626", linewidth=1.5, linestyle="--")
ax.set_title("Room Joins / Leaves (Cumulative)")
ax.set_ylabel("Count")
ax.set_xlabel("Elapsed (min)")
ax.legend()

fig.tight_layout(rect=[0, 0, 1, 0.93])
fig.savefig(out_dir / "05_signaling1_floor_rooms.png", dpi=150)
plt.close(fig)
print(f"  -> {out_dir / '05_signaling1_floor_rooms.png'}")

# ── 4. Text message rate ─────────────────────────────────────────────────────
sig1["text_msg_rate"] = sig1["ws_text_messages_received"].diff().fillna(0) / sig1["interval_sec"]

fig, ax = plt.subplots(figsize=(14, 5))
fig.suptitle("Signaling-1 WS Text Message Rate", fontweight="bold", fontsize=15)
ax.plot(sig1["elapsed_min"], sig1["text_msg_rate"], color="#2563eb", linewidth=1.2)
ax.set_ylabel("messages/sec")
ax.set_xlabel("Elapsed (min)")
ax.axhline(y=sig1["text_msg_rate"].mean(), color="#dc2626", linestyle="--", alpha=0.6,
           label=f"avg: {sig1['text_msg_rate'].mean():.2f}/s")
ax.legend()
fig.tight_layout(rect=[0, 0, 1, 0.93])
fig.savefig(out_dir / "06_signaling1_text_rate.png", dpi=150)
plt.close(fig)
print(f"  -> {out_dir / '06_signaling1_text_rate.png'}")

# ── 5. Combined dashboard summary ────────────────────────────────────────────
fig, axes = plt.subplots(2, 3, figsize=(20, 10))
fig.suptitle("WalkieTalk Benchmark Dashboard — {}".format(data_dir.name),
             fontweight="bold", fontsize=16)

# Client CPU
ax = axes[0, 0]
cpu_rate = client["cpu_sec"].diff().fillna(0) / 9  # approx 9s intervals
ax.plot(client["elapsed_min"], cpu_rate, color="#2563eb", linewidth=1.2)
ax.set_title("Client CPU Rate (sec/sec)")
ax.set_ylabel("CPU sec/sec")
ax.set_xlabel("min")

# Client Memory
ax = axes[0, 1]
ax.fill_between(client["elapsed_min"], client["working_set_mb"], alpha=0.3, color="#16a34a")
ax.plot(client["elapsed_min"], client["working_set_mb"], color="#16a34a", linewidth=1.2)
ax.set_title("Client Working Set")
ax.set_ylabel("MB")
ax.set_xlabel("min")

# Docker signaling-1 CPU
ax = axes[0, 2]
s1_docker = docker[docker["container"] == "walkietalk-signaling-1-1"]
ax.plot(s1_docker["elapsed_min"], s1_docker["cpu_pct"], color="#7c3aed", linewidth=1.2)
ax.set_title("Signaling-1 Docker CPU %")
ax.set_ylabel("CPU %")
ax.set_xlabel("min")

# Audio frames rate
ax = axes[1, 0]
ax.plot(sig1["elapsed_min"], sig1["audio_fps"], color="#ea580c", linewidth=1.2)
ax.set_title("Audio Frame Rate")
ax.set_ylabel("frames/s")
ax.set_xlabel("min")

# Floor events cumulative
ax = axes[1, 1]
ax.stackplot(sig1["elapsed_min"],
             sig1["floor_requests"],
             sig1["floor_releases"],
             labels=["Requests", "Releases"],
             colors=["#2563eb", "#ea580c"], alpha=0.5)
ax.set_title("Floor Events (Cumulative)")
ax.set_ylabel("Count")
ax.set_xlabel("min")
ax.legend(fontsize=8)

# Audio throughput kbps
ax = axes[1, 2]
ax.plot(sig1["elapsed_min"], sig1["audio_kbps"], color="#0891b2", linewidth=1.2)
ax.set_title("Audio Throughput")
ax.set_ylabel("kbps")
ax.set_xlabel("min")

fig.tight_layout(rect=[0, 0, 1, 0.95])
fig.savefig(out_dir / "00_dashboard.png", dpi=150)
plt.close(fig)
print(f"  -> {out_dir / '00_dashboard.png'}")

# ── Print summary stats ──────────────────────────────────────────────────────
print("\n=== Benchmark Summary ===")
print(f"Duration:        {client['elapsed_min'].iloc[-1]:.1f} minutes")
print(f"Samples:         {len(client)} client, {len(sig1)} signaling")
print(f"\nClient Process:")
print(f"  Working Set:   {client['working_set_mb'].min():.1f} – {client['working_set_mb'].max():.1f} MB (avg {client['working_set_mb'].mean():.1f})")
print(f"  Private Mem:   {client['private_mem_mb'].min():.1f} – {client['private_mem_mb'].max():.1f} MB (avg {client['private_mem_mb'].mean():.1f})")
print(f"  Threads:       {client['threads'].min()} – {client['threads'].max()}")
print(f"  Handles:       {client['handles'].min()} – {client['handles'].max()}")
print(f"  CPU (total):   {client['cpu_sec'].iloc[-1]:.1f} sec over {client['elapsed_min'].iloc[-1]:.1f} min")

print(f"\nSignaling-1:")
print(f"  WS Conns Open: {sig1['ws_connections_opened'].iloc[-1]}")
print(f"  Audio Frames:  {sig1['audio_frames_relayed'].iloc[-1]:,}")
print(f"  Audio Bytes:   {sig1['audio_bytes_relayed'].iloc[-1]:,} ({sig1['audio_bytes_relayed'].iloc[-1]/1024:.0f} KB)")
print(f"  Floor Reqs:    {sig1['floor_requests'].iloc[-1]}")
print(f"  Floor Grants:  {sig1['floor_grants'].iloc[-1]}")
print(f"  Floor Denials: {sig1['floor_denials'].iloc[-1]}")
print(f"  Room Joins:    {sig1['room_joins'].iloc[-1]}")
print(f"  Avg Audio FPS: {sig1['audio_fps'].mean():.1f}")
print(f"  Peak Audio:    {sig1['audio_kbps'].max():.1f} kbps")

s1 = docker[docker["container"] == "walkietalk-signaling-1-1"]
pg = docker[docker["container"] == "walkietalk-postgres-1"]
print(f"\nDocker (signaling-1):")
print(f"  CPU %:         {s1['cpu_pct'].min():.2f} – {s1['cpu_pct'].max():.2f} (avg {s1['cpu_pct'].mean():.2f})")
print(f"  Memory:        {s1['mem_usage_mb'].min():.1f} – {s1['mem_usage_mb'].max():.1f} MB")
print(f"  Net RX:        {s1['net_rx_mb'].iloc[-1]:.2f} MB  TX: {s1['net_tx_mb'].iloc[-1]:.2f} MB")
print(f"\nDocker (postgres):")
print(f"  CPU %:         {pg['cpu_pct'].min():.2f} – {pg['cpu_pct'].max():.2f} (avg {pg['cpu_pct'].mean():.2f})")
print(f"  Memory:        {pg['mem_usage_mb'].min():.1f} – {pg['mem_usage_mb'].max():.1f} MB")

print(f"\nCharts saved to: {out_dir.resolve()}")
