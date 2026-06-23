#!/bin/sh
# install.sh — install xvn from GitHub Releases (macOS / Linux / Windows).
# Usage: curl -fsSL https://raw.githubusercontent.com/latentwill/xvision/main/scripts/install.sh | sh
#
# Detects OS + arch, downloads the matching archive, verifies SHA256,
# and installs to /usr/local/bin (preferred) or ~/.local/bin (fallback).

set -eu

REPO="latentwill/xvision"
FALLBACK_DIR="${HOME}/.local/bin"

# ── detect platform ──────────────────────────────────────────────────────
# Output: target|archive_ext|binary_name
detect_platform() {
    local os arch target ext bin

    os=$(uname -s)
    arch=$(uname -m)

    case "$os" in
        Darwin)
            ext="tar.gz"; bin="xvn"
            case "$arch" in
                arm64|aarch64) target="aarch64-apple-darwin" ;;
                x86_64|amd64)  target="x86_64-apple-darwin" ;;
                *) echo "unsupported macOS arch: $arch" >&2; exit 1 ;;
            esac
            ;;
        Linux)
            ext="tar.gz"; bin="xvn"
            case "$arch" in
                x86_64|amd64) target="x86_64-linux-musl" ;;
                aarch64|arm64) echo "Linux arm64 not yet supported — build from source" >&2; exit 1 ;;
                *) echo "unsupported Linux arch: $arch" >&2; exit 1 ;;
            esac
            ;;
        MINGW*|MSYS*|CYGWIN*)
            ext="zip"; bin="xvn.exe"
            case "$arch" in
                x86_64|amd64|AMD64) target="x86_64-pc-windows-msvc" ;;
                *) echo "unsupported Windows arch: $arch" >&2; exit 1 ;;
            esac
            ;;
        *)
            echo "unsupported OS: $os" >&2; exit 1
            ;;
    esac

    echo "${target}|${ext}|${bin}"
}

# ── resolve install dir ──────────────────────────────────────────────────
resolve_install_dir() {
    if [ -w "/usr/local/bin" ]; then
        INSTALL_DIR="/usr/local/bin"
    elif command -v sudo >/dev/null 2>&1; then
        echo "Installing to /usr/local/bin (sudo required)"
        INSTALL_DIR="/usr/local/bin"
    else
        echo "Installing to $FALLBACK_DIR"
        INSTALL_DIR="$FALLBACK_DIR"
        mkdir -p "$INSTALL_DIR"
    fi
}

# ── main ─────────────────────────────────────────────────────────────────
main() {
    local platform_info target ext bin rest
    platform_info=$(detect_platform)

    # Parse target|ext|bin
    target="${platform_info%%|*}"
    rest="${platform_info#*|}"
    ext="${rest%%|*}"
    bin="${rest#*|}"

    echo "Detected platform: $target"

    INSTALL_DIR=""
    resolve_install_dir

    # Fetch latest release version
    local latest_tag
    latest_tag=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$latest_tag" ]; then
        echo "Failed to fetch latest release version" >&2
        exit 1
    fi
    echo "Latest version: $latest_tag"

    local archive="xvn-${target}.${ext}"
    local artifact_url="https://github.com/${REPO}/releases/download/${latest_tag}/${archive}"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    # Download archive + checksum
    echo "Downloading $artifact_url ..."
    curl -sL "$artifact_url" -o "$tmpdir/archive.${ext}"
    curl -sL "${artifact_url}.sha256" -o "$tmpdir/archive.${ext}.sha256"

    # Verify SHA256 (cross-platform: sha256sum or shasum -a 256)
    local expected actual
    expected=$(awk '{print $1}' "$tmpdir/archive.${ext}.sha256")
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$tmpdir/archive.${ext}" | awk '{print $1}')
    else
        actual=$(shasum -a 256 "$tmpdir/archive.${ext}" | awk '{print $1}')
    fi
    if [ "$expected" != "$actual" ]; then
        echo "SHA256 mismatch!" >&2
        echo "  expected: $expected" >&2
        echo "  got:      $actual" >&2
        exit 1
    fi
    echo "SHA256 verified"

    # Extract
    case "$ext" in
        tar.gz) tar xzf "$tmpdir/archive.${ext}" -C "$tmpdir" ;;
        zip)    unzip -qo "$tmpdir/archive.${ext}" -d "$tmpdir" ;;
    esac

    # Install
    if [ "$INSTALL_DIR" = "/usr/local/bin" ] && [ ! -w "$INSTALL_DIR" ]; then
        sudo mv "$tmpdir/$bin" "$INSTALL_DIR/"
        sudo chmod +x "$INSTALL_DIR/$bin"
    else
        mv "$tmpdir/$bin" "$INSTALL_DIR/"
        chmod +x "$INSTALL_DIR/$bin"
    fi

    echo "Installed to $INSTALL_DIR/$bin"
    echo ""
    echo "Run 'xvn dashboard serve' to start."
    echo "Docs: https://github.com/${REPO}#readme"
}

main
