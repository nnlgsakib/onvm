#!/usr/bin/env bash
set -euo pipefail

# Bash equivalent of scripts/release.ps1 for running inside WSL/Linux.
# Requires: cross (for non-Windows targets), rustup toolchains, zip, and Docker.

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}/.."

declare -A OUT_NAMES=(
  ["x86_64-pc-windows-msvc"]="onvm-windows.exe"
  ["x86_64-unknown-linux-musl"]="onvm-linux-x64"
  ["aarch64-unknown-linux-musl"]="onvm-linux-arm64"
  ["x86_64-apple-darwin"]="onvm-macos-x64"
  ["aarch64-apple-darwin"]="onvm-macos-arm64"
)

CROSS_TARGETS=(
  "x86_64-unknown-linux-musl"
  "aarch64-unknown-linux-musl"
  "x86_64-apple-darwin"
  "aarch64-apple-darwin"
)

WINDOWS_TARGET="x86_64-pc-windows-msvc"

if ! command -v cross >/dev/null 2>&1; then
  echo "error: cross is not installed. Install with: cargo install cross" >&2
  exit 1
fi

if ! command -v zip >/dev/null 2>&1; then
  echo "error: zip is not installed. Install your distro's zip package." >&2
  exit 1
fi

REPO_PATH="$(pwd)"
REMAP_FLAG="--remap-path-prefix=${REPO_PATH}=."
export RUSTFLAGS="${RUSTFLAGS:-} ${REMAP_FLAG}"

echo "Starting multi-platform build (WSL/Linux) with path remapping..."

BUILT_TARGETS=()

for target in "${CROSS_TARGETS[@]}"; do
  echo "Building ${target} via cross"
  cross build --release --target "${target}"
  BUILT_TARGETS+=("${target}")
done

echo "Building ${WINDOWS_TARGET} with native toolchain (skips if not installed)"
if rustup target list --installed | grep -qx "${WINDOWS_TARGET}"; then
  cargo build --release --target "${WINDOWS_TARGET}"
  BUILT_TARGETS+=("${WINDOWS_TARGET}")
else
  echo "warning: ${WINDOWS_TARGET} is not installed; skipping Windows build." >&2
  echo "         install with: rustup toolchain install stable-${WINDOWS_TARGET} && rustup target add ${WINDOWS_TARGET}" >&2
fi

rm -rf dist
mkdir -p dist/zipped

for target in "${BUILT_TARGETS[@]}"; do
  out_name="${OUT_NAMES[${target}]}"
  mapfile -t bins < <(find "target/${target}/release" -maxdepth 1 -type f -name 'onvm*' -printf '%f\n')

  if [[ ${#bins[@]} -eq 0 ]]; then
    echo "error: Binary not found for target ${target}" >&2
    exit 1
  fi

  for bin in "${bins[@]}"; do
    src="target/${target}/release/${bin}"
    dest_name="${bin}"
    case "${bin}" in
      onvm|onvm.exe) dest_name="${out_name}" ;;
    esac
    # Avoid remapping debug symbols like .pdb/.dSYM to the main binary name.
    if [[ "${bin}" == "onvm.pdb" || "${bin}" == *.dSYM ]]; then
      dest_name="${bin}"
    fi
    cp "${src}" "dist/${dest_name}"
    (cd dist && zip -q "zipped/${dest_name}.zip" "${dest_name}")
  done
done

echo
echo "Build finished. Binaries are in dist/ and zipped versions in dist/zipped/."
