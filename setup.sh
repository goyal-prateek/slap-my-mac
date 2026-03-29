#!/usr/bin/env bash
# One-shot local setup: prerequisites (when missing), pnpm install, optional Tauri build.
set -euo pipefail

PNPM_VERSION="9.4.0"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"

DO_BUILD=1
for arg in "$@"; do
  case "$arg" in
    --no-build) DO_BUILD=0 ;;
    -h|--help)
      echo "Usage: $0 [--no-build] [--help]"
      echo "  Installs missing dependencies when possible, runs pnpm install, then pnpm tauri build."
      echo "  --no-build  Skip the Tauri release build (only toolchain + pnpm install)."
      echo "  After a successful build, removes src-tauri/target and runs open -a \"Slap My Mac\" (app must already be in Applications)."
      exit 0
      ;;
    *)
      echo "Unknown option: $arg (try --help)" >&2
      exit 1
      ;;
  esac
done

die() {
  echo "Error: $*" >&2
  exit 1
}

ensure_macos() {
  [[ "$(uname -s)" == "Darwin" ]] || die "This script is for macOS only."
}

ensure_xcode_clt() {
  if xcode-select -p &>/dev/null; then
    echo "✓ Xcode Command Line Tools"
    return
  fi
  echo "Xcode Command Line Tools are required. Opening the installer…"
  echo "Complete the dialog, then run this script again."
  xcode-select --install || true
  exit 1
}

ensure_node() {
  if command -v node &>/dev/null; then
    echo "✓ Node.js $(node -v)"
    # Corepack ships with Node 16.13+; Tauri/Vite expect a reasonably current Node.
    local major
    major="$(node -p "parseInt(process.versions.node.split('.')[0], 10)" 2>/dev/null || echo 0)"
    if (( major < 18 )); then
      echo "Warning: Node $(node -v) may be too old. Prefer current LTS (20+)." >&2
    fi
    return
  fi

  if command -v brew &>/dev/null; then
    echo "Installing Node.js via Homebrew…"
    brew install node
    echo "✓ Node.js $(node -v)"
    return
  fi

  die "Node.js is not installed and Homebrew was not found. Install Node LTS from https://nodejs.org/ or install Homebrew from https://brew.sh/ then re-run this script."
}

ensure_pnpm() {
  if command -v pnpm &>/dev/null; then
    local v
    v="$(pnpm -v 2>/dev/null || true)"
    if [[ "$v" == "$PNPM_VERSION" ]] || [[ "$v" == 9.* ]]; then
      echo "✓ pnpm $v"
      return
    fi
    echo "Adjusting pnpm to $PNPM_VERSION (was $v)…"
  else
    echo "Enabling pnpm $PNPM_VERSION via Corepack…"
  fi

  corepack enable
  corepack prepare "pnpm@${PNPM_VERSION}" --activate
  echo "✓ pnpm $(pnpm -v)"
}

ensure_rust() {
  export PATH="${HOME}/.cargo/bin:${PATH}"

  if command -v cargo &>/dev/null && command -v rustc &>/dev/null; then
    echo "✓ Rust $(rustc -V | cut -d' ' -f2)"
    return
  fi

  echo "Installing Rust via rustup (stable, default toolchain)…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck source=/dev/null
  [[ -f "${HOME}/.cargo/env" ]] && source "${HOME}/.cargo/env"
  export PATH="${HOME}/.cargo/bin:${PATH}"

  command -v cargo &>/dev/null || die "rustup finished but cargo is not on PATH. Open a new terminal or run: source \"\$HOME/.cargo/env\""
  echo "✓ Rust $(rustc -V | cut -d' ' -f2)"
}

main() {
  cd "$REPO_ROOT"
  [[ -f package.json ]] || die "Run this from the repository root (missing package.json)."
  [[ -d src-tauri ]] || die "Missing src-tauri/ directory."

  echo "== Slap My Mac — setup =="
  ensure_macos
  ensure_xcode_clt
  ensure_node
  ensure_pnpm
  ensure_rust

  echo ""
  echo "== pnpm install =="
  pnpm install

  if (( DO_BUILD )); then
    echo ""
    echo "== pnpm tauri build (first run can take several minutes) =="
    pnpm tauri build
    echo ""
    echo "== Removing src-tauri/target =="
    rm -rf "${REPO_ROOT}/src-tauri/target"
    echo "✓ Removed src-tauri/target"
    echo ""
    echo "== Launching Slap My Mac =="
    open -a "Slap My Mac"
    echo "Done."
  else
    echo ""
    echo "Done (build skipped). Run: pnpm tauri build"
  fi
}

main "$@"
