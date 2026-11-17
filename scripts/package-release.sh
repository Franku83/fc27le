#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="0.1.0-alpha.1"
NAME="fc27le-v${VERSION}-linux-x86_64"
DIST="${ROOT}/dist"
STAGE="${DIST}/${NAME}"
ARCHIVE="${DIST}/${NAME}.tar.gz"
GIT_COMMIT="${FC27LE_GIT_COMMIT:-$(git -C "${ROOT}" rev-parse --short=12 HEAD)}"
CARGO_HOME="${CARGO_HOME:-${HOME}/.cargo}"
BUILD_ROOT="${TMPDIR:-/tmp}/fc27le-release-build"
BUILD_TARGET="${BUILD_ROOT}/target"

rm -rf "${BUILD_ROOT}"
mkdir -p "${BUILD_ROOT}"
(
  cd "${ROOT}"
  tar \
    --exclude='./target' \
    --exclude='./dist' \
    --exclude='./.git' \
    -cf - .
) | tar -C "${BUILD_ROOT}" -xf -

cd "${BUILD_ROOT}"
RUSTFLAGS="--remap-path-prefix=${BUILD_ROOT}=/build/fc27le --remap-path-prefix=${CARGO_HOME}=/cargo" \
FC27LE_GIT_COMMIT="${GIT_COMMIT}" \
CARGO_TARGET_DIR="${BUILD_TARGET}" \
cargo build --release --workspace

rm -rf "${STAGE}" "${ARCHIVE}" "${ARCHIVE}.sha256"
mkdir -p "${STAGE}/docs"

install -Dm755 "${BUILD_TARGET}/release/fc27le-gui" "${STAGE}/fc27le-gui"
install -Dm755 "${BUILD_TARGET}/release/fc27le" "${STAGE}/fc27le"
install -Dm644 LICENSE "${STAGE}/LICENSE"
install -Dm644 THIRD_PARTY_LICENSES.md "${STAGE}/THIRD_PARTY_LICENSES.md"
install -Dm644 README.md "${STAGE}/README.md"
install -Dm644 CHANGELOG.md "${STAGE}/CHANGELOG.md"
install -Dm644 docs/lua_compatibility.md "${STAGE}/docs/lua_compatibility.md"
install -Dm644 docs/embedded_profile.md "${STAGE}/docs/embedded_profile.md"
install -Dm644 docs/compatibility.md "${STAGE}/docs/compatibility.md"
install -Dm644 docs/release_notes_v0.1.0-alpha.1.md "${STAGE}/docs/release_notes_v0.1.0-alpha.1.md"

# Reproducible archive metadata: sorted paths, zero ownership, fixed timestamp.
tar \
  --sort=name \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  --mtime='UTC 2026-11-16' \
  -C "${DIST}" \
  -czf "${ARCHIVE}" \
  "${NAME}"

(
  cd "${DIST}"
  sha256sum "${NAME}.tar.gz" > "${NAME}.tar.gz.sha256"
)

echo "archive: ${ARCHIVE}"
echo "checksum: ${ARCHIVE}.sha256"
echo "commit: ${GIT_COMMIT}"
echo "contents:"
tar -tzf "${ARCHIVE}"
echo "dynamic dependencies (GUI):"
ldd "${STAGE}/fc27le-gui"
echo "size: $(du -h "${ARCHIVE}" | cut -f1)"
