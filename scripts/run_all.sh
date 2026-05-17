#!/usr/bin/env bash
# =============================================================================
# run_all.sh — One-shot benchmarking + report generation script
#
# Usage:
#   bash scripts/run_all.sh
#
# What it does:
#   1. Checks prerequisites (Rust toolchain)
#   2. Runs cargo bench --bench compare  (Criterion throughput, saves baseline)
#   3. Runs cargo bench --bench latency  (HDR histogram latency + CSV)
#   4. Builds & runs examples/report.rs  (HTML analytics report)
#   5. Opens report.html in the default browser
#
# To compare against a saved baseline on the next run, use:
#   COMPARE_BASELINE=1 bash scripts/run_all.sh
# =============================================================================
set -euo pipefail

BOLD=$(tput bold 2>/dev/null || echo "")
GREEN=$(tput setaf 2 2>/dev/null || echo "")
CYAN=$(tput setaf 6 2>/dev/null || echo "")
YELLOW=$(tput setaf 3 2>/dev/null || echo "")
RESET=$(tput sgr0 2>/dev/null || echo "")

step() { echo "${CYAN}${BOLD}▶ $*${RESET}"; }
ok()   { echo "${GREEN}  ✓ $*${RESET}"; }
warn() { echo "${YELLOW}  ⚠ $*${RESET}"; }

# Move to the workspace root regardless of where the script is called from.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}/.."

echo ""
echo "${BOLD}╔════════════════════════════════════════════════════╗${RESET}"
echo "${BOLD}║   Thread-Safe LRU Cache — Full Benchmark Suite     ║${RESET}"
echo "${BOLD}╚════════════════════════════════════════════════════╝${RESET}"
echo ""

# ── Prerequisites ─────────────────────────────────────────────────────────────
step "Checking prerequisites"
if ! command -v cargo &>/dev/null; then
  echo "  ✗ cargo not found. Install Rust: https://rustup.rs"
  exit 1
fi
ok "cargo $(cargo --version)"
echo ""

# ── Build check ───────────────────────────────────────────────────────────────
step "Build check (all benches + examples)"
cargo build --benches --examples --release -q
ok "build successful"
echo ""

# ── Criterion compare bench ───────────────────────────────────────────────────
step "Running: cargo bench --bench compare"
echo "  (HTML report → target/criterion/  |  estimated ~3-5 min)"
echo ""

COMPARE_FLAGS="--bench compare"

# Save a baseline named after the current git commit (or 'local' if no git).
BASELINE_NAME=$(git rev-parse --short HEAD 2>/dev/null || echo "local")

if [[ "${COMPARE_BASELINE:-0}" == "1" ]]; then
  # Compare against the last saved 'stable' baseline.
  if cargo bench ${COMPARE_FLAGS} -- --load-baseline stable --baseline stable 2>/dev/null; then
    ok "compare bench done (compared against 'stable' baseline)"
  else
    warn "No 'stable' baseline found, running without comparison"
    cargo bench ${COMPARE_FLAGS} -- --save-baseline "${BASELINE_NAME}"
  fi
else
  # First run: save baseline named by commit hash AND as 'stable'.
  cargo bench ${COMPARE_FLAGS} -- --save-baseline "${BASELINE_NAME}"
  cargo bench ${COMPARE_FLAGS} -- --save-baseline stable
  ok "compare bench done (baseline '${BASELINE_NAME}' + 'stable' saved)"
fi
echo ""

# ── HDR latency bench ─────────────────────────────────────────────────────────
step "Running: cargo bench --bench latency"
echo "  (CSV → latency_results.csv  |  estimated ~1-2 min)"
echo ""
cargo bench --bench latency
ok "latency bench done"
echo ""

# ── HTML analytics report ─────────────────────────────────────────────────────
step "Generating HTML report"
echo "  cargo run --example report --release"
echo ""
cargo run --example report --release
echo ""

# ── Open HTML report ──────────────────────────────────────────────────────────
REPORT_PATH="$(pwd)/report.html"
if [[ -f "${REPORT_PATH}" ]]; then
  ok "Report available at: ${REPORT_PATH}"
  if command -v xdg-open &>/dev/null; then
    xdg-open "${REPORT_PATH}" &
    ok "Opened in default browser (xdg-open)"
  elif command -v open &>/dev/null; then
    open "${REPORT_PATH}"
    ok "Opened in default browser (open)"
  else
    warn "Cannot auto-open browser. Open manually: file://${REPORT_PATH}"
  fi
fi

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "${BOLD}╔════════════════════════════════════════════════════╗${RESET}"
echo "${BOLD}║   All done! Artifacts:                             ║${RESET}"
echo "${BOLD}║   • report.html            ← Analytics HTML        ║${RESET}"
echo "${BOLD}║   • latency_results.csv    ← Latency percentiles   ║${RESET}"
echo "${BOLD}║   • target/criterion/      ← Criterion HTML charts  ║${RESET}"
echo "${BOLD}╚════════════════════════════════════════════════════╝${RESET}"
echo ""

# Tip: next run with regression comparison
echo "  Tip: run with COMPARE_BASELINE=1 to diff against the saved baseline:"
echo "    COMPARE_BASELINE=1 bash scripts/run_all.sh"
echo ""
