#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────
# bench-collect.sh — Collect performance metrics for WalkieTalk services
#
# Usage:
#   ./scripts/bench-collect.sh                       # defaults: 5s interval, 120s duration
#   ./scripts/bench-collect.sh --interval 2 --duration 300
#   ./scripts/bench-collect.sh --skip-docker-up      # if services already running
# ─────────────────────────────────────────────────────────────────────────
set -euo pipefail

INTERVAL=5
DURATION=120
SKIP_DOCKER_UP=false
CLIENT_PROCESS="walkietalk-client"
PYTHON_CMD="python"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --interval)   INTERVAL="$2";   shift 2 ;;
    --duration)   DURATION="$2";   shift 2 ;;
    --skip-docker-up) SKIP_DOCKER_UP=true; shift ;;
    --client)     CLIENT_PROCESS="$2"; shift 2 ;;
    *) echo "Unknown flag: $1"; exit 1 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
TS=$(date +%Y%m%d_%H%M%S)
OUT_DIR="$PROJECT_DIR/bench-results/$TS"
mkdir -p "$OUT_DIR"

echo "📂 Results: $OUT_DIR"

# ── Start services ────────────────────────────────────────────────────────
if [ "$SKIP_DOCKER_UP" = false ]; then
  echo "🐳 Starting services with bench overlay (logging OFF)..."
  cd "$PROJECT_DIR"
  docker compose -f docker-compose.yml -f docker-compose.bench.yml up -d --build --force-recreate
  echo "⏳ Waiting 10s for services to stabilise..."
  sleep 10
fi

# ── Health check ──────────────────────────────────────────────────────────
for svc in "auth:3001" "signaling-1:3002" "signaling-2:3003"; do
  name="${svc%%:*}"
  port="${svc##*:}"
  if curl -sf "http://localhost:$port/health" > /dev/null 2>&1; then
    echo "  ✅ $name healthy"
  else
    echo "  ⚠️  $name not reachable on port $port"
  fi
done

# ── CSV files ─────────────────────────────────────────────────────────────
DOCKER_CSV="$OUT_DIR/docker_stats.csv"
SIG1_CSV="$OUT_DIR/signaling_1_metrics.csv"
SIG2_CSV="$OUT_DIR/signaling_2_metrics.csv"
CLIENT_CSV="$OUT_DIR/client_process.csv"

echo "timestamp,container,cpu_pct,mem_usage_mb,mem_limit_mb,mem_pct,net_rx_mb,net_tx_mb,block_read_mb,block_write_mb,pids" \
  > "$DOCKER_CSV"

echo "timestamp,cpu_sec,working_set_mb,private_mem_mb,threads,handles" > "$CLIENT_CSV"

SIG1_HEADER=false
SIG2_HEADER=false

# ── Helpers ───────────────────────────────────────────────────────────────
to_mb() {
  local val="$1"
  if [[ "$val" =~ ([0-9.]+)(GiB|GB) ]]; then
    echo "${BASH_REMATCH[1]}" | awk '{printf "%.3f", $1 * 1024}'
  elif [[ "$val" =~ ([0-9.]+)(MiB|MB) ]]; then
    echo "${BASH_REMATCH[1]}"
  elif [[ "$val" =~ ([0-9.]+)(KiB|kB|KB) ]]; then
    echo "${BASH_REMATCH[1]}" | awk '{printf "%.3f", $1 / 1024}'
  elif [[ "$val" =~ ([0-9.]+)B ]]; then
    echo "${BASH_REMATCH[1]}" | awk '{printf "%.6f", $1 / 1048576}'
  else
    echo "0"
  fi
}

fetch_signaling_metrics() {
  local url="$1" csv="$2" ts="$3" header_var="$4"
  local json
  json=$(curl -sf "$url" 2>/dev/null) || return 0
  # Extract keys and values using python (available on most systems)
  if [ "$header_var" = "false" ]; then
    local keys
    keys=$(echo "$json" | $PYTHON_CMD -c "import sys,json; d=json.load(sys.stdin); print('timestamp,' + ','.join(d.keys()))")
    echo "$keys" > "$csv"
    # Signal header written via file marker
    touch "$csv.header_done"
  fi
  local vals
  vals=$(echo "$json" | $PYTHON_CMD -c "import sys,json; d=json.load(sys.stdin); print(','.join(str(v) for v in d.values()))")
  echo "$ts,$vals" >> "$csv"
}

# ── Collection loop ───────────────────────────────────────────────────────
ITERATIONS=$(( (DURATION + INTERVAL - 1) / INTERVAL ))
echo "📊 Collecting every ${INTERVAL}s for ${DURATION}s ($ITERATIONS samples)..."
echo "   Press Ctrl+C to stop early."

for (( i=1; i<=ITERATIONS; i++ )); do
  NOW=$(date "+%Y-%m-%d %H:%M:%S")
  PCT=$(( i * 100 / ITERATIONS ))
  printf "  [%3d%%] tick %d/%d  (%s)" "$PCT" "$i" "$ITERATIONS" "$NOW"

  # 1) Docker stats
  docker stats --no-stream --format "{{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}\t{{.NetIO}}\t{{.BlockIO}}\t{{.PIDs}}" 2>/dev/null | while IFS=$'\t' read -r name cpu mem mem_pct net blk pids; do
    cpu="${cpu//%/}"
    mem_pct="${mem_pct//%/}"
    IFS='/' read -r mu ml <<< "$mem"
    mu_mb=$(to_mb "$mu"); ml_mb=$(to_mb "$ml")
    IFS='/' read -r nr nt <<< "$net"
    nr_mb=$(to_mb "$nr"); nt_mb=$(to_mb "$nt")
    IFS='/' read -r br bw <<< "$blk"
    br_mb=$(to_mb "$br"); bw_mb=$(to_mb "$bw")
    echo "$NOW,$name,$cpu,$mu_mb,$ml_mb,$mem_pct,$nr_mb,$nt_mb,$br_mb,$bw_mb,$pids" >> "$DOCKER_CSV"
  done

  # 2) Signaling /metrics
  if [ ! -f "$SIG1_CSV.header_done" ]; then
    fetch_signaling_metrics "http://localhost:3002/metrics" "$SIG1_CSV" "$NOW" "false"
  else
    fetch_signaling_metrics "http://localhost:3002/metrics" "$SIG1_CSV" "$NOW" "true"
  fi
  if [ ! -f "$SIG2_CSV.header_done" ]; then
    fetch_signaling_metrics "http://localhost:3003/metrics" "$SIG2_CSV" "$NOW" "false"
  else
    fetch_signaling_metrics "http://localhost:3003/metrics" "$SIG2_CSV" "$NOW" "true"
  fi

  # 3) Client process (Windows-compatible via PowerShell)
  client_line=$(powershell.exe -NoProfile -Command "
    \$p = Get-Process -Name '$CLIENT_PROCESS' -ErrorAction SilentlyContinue | Select-Object -First 1;
    if (\$p) {
      \$cpu = [math]::Round(\$p.TotalProcessorTime.TotalSeconds, 2);
      \$ws  = [math]::Round(\$p.WorkingSet64 / 1MB, 2);
      \$pm  = [math]::Round(\$p.PrivateMemorySize64 / 1MB, 2);
      \$thr = \$p.Threads.Count;
      \$hnd = \$p.HandleCount;
      Write-Output \"\$cpu,\$ws,\$pm,\$thr,\$hnd\";
    }" 2>/dev/null | tr -d '\r')
  if [ -n "$client_line" ]; then
    echo "$NOW,$client_line" >> "$CLIENT_CSV"
  fi

  echo "  ✓"
  [ "$i" -lt "$ITERATIONS" ] && sleep "$INTERVAL"
done

# Cleanup marker files
rm -f "$SIG1_CSV.header_done" "$SIG2_CSV.header_done"

echo ""
echo "═══════════════════════════════════════════"
echo "  Benchmark collection complete!"
echo "  Results: $OUT_DIR"
echo "═══════════════════════════════════════════"
ls -lh "$OUT_DIR"
