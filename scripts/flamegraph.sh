#!/usr/bin/env bash
# =============================================================================
# flamegraph.sh — Generate per-implementation SVG flamegraphs
#
# Prerequisites (Linux):
#   1. Install the flamegraph cargo sub-command:
#        cargo install flamegraph
#
#   2. Install Linux perf:
#        # Debian / Ubuntu:
#        sudo apt-get install linux-perf
#        # or
#        sudo apt-get install linux-tools-$(uname -r) linux-tools-generic
#
#        # Fedora / RHEL:
#        sudo dnf install perf
#
#        # Arch Linux:
#        sudo pacman -S perf
#
#   3. Allow unprivileged perf (temporary, reset on reboot):
#        echo -1 | sudo tee /proc/sys/kernel/perf_event_paranoid
#        echo  0 | sudo tee /proc/sys/kernel/kptr_restrict
#
#   4. (Optional) Permanent setting via sysctl:
#        echo 'kernel.perf_event_paranoid = -1' | sudo tee /etc/sysctl.d/99-perf.conf
#        sudo sysctl --system
#
# macOS (DTrace-based, no perf needed):
#   cargo install flamegraph   # same command; uses DTrace under the hood
#
# Usage:
#   bash scripts/flamegraph.sh
#
# Output:
#   flamegraphs/compare_basic.svg
#   flamegraphs/compare_sharded_lru.svg
#   flamegraphs/compare_sharded_fifo.svg
# =============================================================================
set -euo pipefail

BOLD=$(tput bold 2>/dev/null || echo "")
GREEN=$(tput setaf 2 2>/dev/null || echo "")
YELLOW=$(tput setaf 3 2>/dev/null || echo "")
RED=$(tput setaf 1 2>/dev/null || echo "")
RESET=$(tput sgr0 2>/dev/null || echo "")

step() { echo "${BOLD}▶ $*${RESET}"; }
ok()   { echo "${GREEN}  ✓ $*${RESET}"; }
warn() { echo "${YELLOW}  ⚠ $*${RESET}"; }
fail() { echo "${RED}  ✗ $*${RESET}"; }

# Move to the workspace root.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}/.."

echo ""
echo "${BOLD}╔════════════════════════════════════════╗${RESET}"
echo "${BOLD}║   LRU Cache — Flamegraph Generator     ║${RESET}"
echo "${BOLD}╚════════════════════════════════════════╝${RESET}"
echo ""

# ── Check cargo-flamegraph ────────────────────────────────────────────────────
if ! command -v cargo-flamegraph &>/dev/null; then
  warn "cargo-flamegraph not found. Installing..."
  if cargo install flamegraph; then
    ok "flamegraph installed"
  else
    fail "Failed to install flamegraph. Please install manually:"
    echo "      cargo install flamegraph"
    exit 1
  fi
fi
ok "cargo-flamegraph: $(cargo flamegraph --version 2>/dev/null || echo 'installed')"

# ── Check perf (Linux only) ───────────────────────────────────────────────────
if [[ "$(uname)" == "Linux" ]]; then
  if ! command -v perf &>/dev/null; then
    fail "perf not found. Install it with:"
    echo ""
    echo "  Ubuntu/Debian:"
    echo "    sudo apt-get install linux-tools-\$(uname -r) linux-tools-generic"
    echo ""
    echo "  Fedora/RHEL:"
    echo "    sudo dnf install perf"
    echo ""
    echo "  Arch:"
    echo "    sudo pacman -S perf"
    exit 1
  fi
  ok "perf: $(perf --version 2>/dev/null | head -1)"

  # Check perf_event_paranoid setting.
  PARANOID=$(cat /proc/sys/kernel/perf_event_paranoid 2>/dev/null || echo "3")
  if [[ "${PARANOID}" -gt 1 ]]; then
    warn "perf_event_paranoid=${PARANOID} (>1). Flamegraph may fail."
    warn "To fix (temporary, resets on reboot):"
    warn "  echo -1 | sudo tee /proc/sys/kernel/perf_event_paranoid"
    warn "  echo  0 | sudo tee /proc/sys/kernel/kptr_restrict"
    echo ""
  fi
fi

# ── Output directory ──────────────────────────────────────────────────────────
mkdir -p flamegraphs

# ── Build the compare bench binary in release ─────────────────────────────────
step "Building compare bench (release)"
cargo build --bench compare --release -q
ok "built"
echo ""

# Criterion bench binary path (Cargo puts it here after `cargo build --bench`).
BENCH_BIN="$(cargo build --bench compare --release --message-format=json 2>/dev/null \
  | grep '"executable"' \
  | tail -1 \
  | sed 's/.*"executable":"\([^"]*\)".*/\1/' || true)"

# Fallback: find it in target/release/deps
if [[ -z "${BENCH_BIN}" ]]; then
  BENCH_BIN=$(find target/release/deps -maxdepth 1 -name 'compare-*' -perm /u+x \
    -not -name '*.d' 2>/dev/null | sort | tail -1)
fi

if [[ -z "${BENCH_BIN}" ]] || [[ ! -f "${BENCH_BIN}" ]]; then
  warn "Could not locate the compiled bench binary automatically."
  warn "Try: cargo build --bench compare --release  then re-run this script."
  exit 1
fi
ok "bench binary: ${BENCH_BIN}"
echo ""

# ── Flamegraph helper ─────────────────────────────────────────────────────────
# Run the bench binary under flamegraph for a specific Criterion filter.
flamegraph_bench() {
  local filter="$1"
  local output="$2"
  step "Flamegraph: ${filter}"
  if cargo flamegraph \
      --output "flamegraphs/${output}" \
      --bin "$(basename "${BENCH_BIN}")" \
      -- --bench "${filter}" 2>/dev/null; then
    ok "flamegraphs/${output}"
  else
    # Fallback: run the pre-built binary directly
    if flamegraph --output "flamegraphs/${output}" -- \
        "${BENCH_BIN}" --bench "${filter}" 2>/dev/null; then
      ok "flamegraphs/${output} (direct)"
    else
      warn "Flamegraph for '${filter}' failed. Check perf permissions (see above)."
    fi
  fi
  echo ""
}

# ── Generate flamegraphs for each impl filter ─────────────────────────────────
# Criterion bench filters select which benchmarks to run — the filter string
# matches the benchmark function name registered with criterion_group!.

flamegraph_bench "workload/read_heavy_80_20/basic_global_mutex" "compare_basic_lru.svg"
flamegraph_bench "workload/read_heavy_80_20/sharded_lru_4"       "compare_sharded_lru.svg"
flamegraph_bench "workload/read_heavy_80_20/sharded_fifo_4"      "compare_sharded_fifo.svg"

# ── Summary ───────────────────────────────────────────────────────────────────
echo "${BOLD}╔═══════════════════════════════════════════╗${RESET}"
echo "${BOLD}║  Flamegraphs written to flamegraphs/      ║${RESET}"
echo "${BOLD}║  Open *.svg in a browser to explore       ║${RESET}"
echo "${BOLD}╚═══════════════════════════════════════════╝${RESET}"
echo ""

# Try to open them.
for f in flamegraphs/*.svg; do
  [[ -f "$f" ]] || continue
  if command -v xdg-open &>/dev/null; then
    xdg-open "$f" &
  elif command -v open &>/dev/null; then
    open "$f"
  fi
done
