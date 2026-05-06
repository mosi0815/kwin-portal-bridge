#!/usr/bin/env bash
# Reproduce the CI cross-build pipeline against the local source tree.
#
# Base image is ubuntu:noble (glibc 2.39) — fine because kwin-portal-bridge
# requires Plasma 6.6+, and Noble is the oldest base any 6.6+ distro ships.
#
# Output binaries land in ./dist/ (override with OUTPUT_DIR=...).
# A persistent build cache lives in ./.pipeline-cache/ so re-runs are fast.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-${REPO_ROOT}/dist}"
CACHE_DIR="${CACHE_DIR:-${REPO_ROOT}/.pipeline-cache}"
IMAGE="${IMAGE:-ubuntu:noble}"
RUST_VERSION="${RUST_VERSION:-stable}"

mkdir -p "${OUTPUT_DIR}"
mkdir -p "${CACHE_DIR}/target" "${CACHE_DIR}/cargo-registry" "${CACHE_DIR}/cargo-git" \
         "${CACHE_DIR}/rustup" "${CACHE_DIR}/apt-archives" "${CACHE_DIR}/apt-lists"

echo ">>> Building kwin-portal-bridge from ${REPO_ROOT}"
echo ">>> Base:    ${IMAGE}"
echo ">>> Output:  ${OUTPUT_DIR}"
echo ">>> Cache:   ${CACHE_DIR}"

docker run --rm \
  -v "${REPO_ROOT}:/src:ro" \
  -v "${OUTPUT_DIR}:/output" \
  -v "${CACHE_DIR}/target:/build/target" \
  -v "${CACHE_DIR}/cargo-registry:/root/.cargo/registry" \
  -v "${CACHE_DIR}/cargo-git:/root/.cargo/git" \
  -v "${CACHE_DIR}/rustup:/root/.rustup" \
  -v "${CACHE_DIR}/apt-archives:/var/cache/apt/archives" \
  -v "${CACHE_DIR}/apt-lists:/var/lib/apt/lists" \
  -e CARGO_TARGET_DIR=/build/target \
  -e DEBIAN_FRONTEND=noninteractive \
  -e RUST_VERSION="${RUST_VERSION}" \
  "${IMAGE}" \
  bash -c '
    set -eo pipefail

    # --- multiarch apt: split host (amd64) and ports (arm64) ---
    # Ubuntu Noble uses deb822 sources. We rewrite ubuntu.sources from
    # scratch so dpkg --add-architecture arm64 doesnt make apt fetch
    # arm64 indexes from archive.ubuntu.com (which 404s for non-x86).
    dpkg --add-architecture arm64
    cat >/etc/apt/sources.list.d/ubuntu.sources <<SOURCES
Types: deb
URIs: http://archive.ubuntu.com/ubuntu
Suites: noble noble-updates noble-backports
Components: main restricted universe multiverse
Architectures: amd64
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg

Types: deb
URIs: http://security.ubuntu.com/ubuntu
Suites: noble-security
Components: main restricted universe multiverse
Architectures: amd64
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg

Types: deb
URIs: http://ports.ubuntu.com/ubuntu-ports
Suites: noble noble-updates noble-security
Components: main restricted universe multiverse
Architectures: arm64
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
SOURCES

    apt-get update
    apt-get install -y --no-install-recommends \
      build-essential pkg-config clang cmake curl ca-certificates \
      gcc-aarch64-linux-gnu g++-aarch64-linux-gnu \
      libxkbcommon-dev libpipewire-0.3-dev \
      libxkbcommon-dev:arm64 libpipewire-0.3-dev:arm64

    # --- Rust toolchain (cached on host) ---
    export CARGO_HOME=/root/.cargo
    export RUSTUP_HOME=/root/.rustup
    export PATH="$CARGO_HOME/bin:$PATH"
    if [ ! -x "$CARGO_HOME/bin/cargo" ]; then
      curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | \
        sh -s -- -y --profile minimal --default-toolchain "$RUST_VERSION" \
                 --target x86_64-unknown-linux-gnu \
                 --target aarch64-unknown-linux-gnu
    else
      rustup default "$RUST_VERSION" >/dev/null
      rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
    fi

    # --- writable copy of source so cargo can touch Cargo.lock if needed ---
    mkdir -p /build/src
    cp -a /src/. /build/src/
    cd /build/src

    # --- x86_64 build (native) ---
    cargo build --release --target x86_64-unknown-linux-gnu
    cp /build/target/x86_64-unknown-linux-gnu/release/kwin-portal-bridge /output/kwin-portal-bridge
    echo "kwin-portal-bridge x86_64 built successfully"

    # --- aarch64 cross build (gcc-aarch64-linux-gnu as linker) ---
    export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
    export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
    export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
    export AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar
    export PKG_CONFIG_ALLOW_CROSS=1
    export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
    export PKG_CONFIG_SYSROOT_DIR=/
    # bindgen-using crates need clang pointed at the arm64 sysroot
    export BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu="--target=aarch64-linux-gnu -I/usr/include/aarch64-linux-gnu -I/usr/include -I/usr/include/pipewire-0.3 -I/usr/include/spa-0.2"

    cargo build --release --target aarch64-unknown-linux-gnu
    cp /build/target/aarch64-unknown-linux-gnu/release/kwin-portal-bridge /output/kwin-portal-bridge-aarch64
    echo "kwin-portal-bridge aarch64 built successfully"
  '

echo
echo ">>> Artifacts:"
ls -lh "${OUTPUT_DIR}"
