#!/bin/sh
# install.sh — install xvn from GitHub Releases (macOS / Linux).
# Usage: curl -fsSL https://raw.githubusercontent.com/latentwill/xvision/main/scripts/install.sh | sh
#
# Detects OS + arch, downloads the matching tarball, verifies SHA256,
# and installs to /usr/local/bin (preferred) or ~/.local/bin (fallback).

set -eu

REPO="latentwill/xvision"
BINARY_NAME="xvn"
INSTALL_DIR=""
FALLBACK_DIR="${HOME}/.local/bin"

# ── detect platform ──────────────────────────────────────────────────────
detect_platform() {
    local os arch target
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    arch=$(uname -m)

    case "$os" in
        darwin)
            case "$arch" in
                arm64|aarch64) target="aarch64-apple-darwin" ;;
                x86_64|amd64)  target="x86_64-apple-darwin" ;;
                *) echo "unsupported macOS arch: $arch" >&2; exit 1 ;;
            esac
            ;;
        linux)
            case "$arch" in
                x86_64|amd64) target="x86_64-linux-musl" ;;
                aarch64|arm64) echo "Linux arm64 not yet supported — build from source" >&2; exit 1 ;;
                *) echo "unsupported Linux arch: $arch" >&2; exit 1 ;;
            esac
            ;;
        mingw*|msys*|cygwin*)
            case "$arch" in
                x86_64|amd64) target="x86_64-pc-windows-msvc" ;;
                *) echo "unsupported Windows arch: $arch" >&2; exit 1 ;;
            esac
            ;;
        *)
            echo "unsupported OS: $os" >&2; exit 1
            ;;
    esac

    echo "$target"
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
    local platform artifact_url checksum_url tmpdir
    platform=$(detect_platform)
    echo "Detected platform: $platform"

    resolve_install_dir

    # Fetch latest release info
    local latest_tag
    latest_tag=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$latest_tag" ]; then
        echo "Failed to fetch latest release tag from GitHub" >&2
        exit 1
    fi
    echo "Latest version: $latest_tag"

    local ext
    case "$platform" in
        *windows*) ext="zip" ;;
        *)         ext="tar.gz" ;;
    esac

    artifact_url="https://github.com/${REPO}/releases/download/${latest_tag}/${file_prefix}.${ext}"
    checksum_url="${artifact_url}.sha256"

    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    # Download tarball + checksum
    echo "Downloading $artifact_url ..."
    curl -sL "$artifact_url" -o "$tmpdir/xvn.${ext}"
    curl -sL "$checksum_url" -o "$tmpdir/xvn.${ext}.sha256"

    # Verify checksum
    local expected actual
    expected=$(awk '{print $1}' "$tmpdir/xvn.${ext}.sha256")
    actual=$(sha256sum "$tmpdir/xvn.${ext}" | awk '{print $1}')
    if [ "$expected" != "$actual" ]; then
        echo "SHA256 mismatch!" >&2
        echo "  expected: $expected" >&2
        echo "  got:      $actual" >&2
        exit 1
    fi
    echo "SHA256 verified"

    case "$ext" in
        zip) unzip -o "$tmpdir/xvn.${ext}" -d "$tmpdir" ;;
        *)   tar xzf "$tmpdir/xvn.${ext}" -C "$tmpdir" ;;
    esac

    if [ "$INSTALL_DIR" = "/usr/local/bin" ] && [ ! -w "$INSTALL_DIR" ]; then
        sudo mv "$tmpdir/$BINARY_NAME" "$INSTALL_DIR/"
        sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
    else
        mv "$tmpdir/$BINARY_NAME" "$INSTALL_DIR/"
        chmod +x "$INSTALL_DIR/$BINARY_NAME"
    fi

    echo "Installed to $INSTALL_DIR/$BINARY_NAME"
    echo ""
    echo "Run 'xvn dashboard serve' to start."
    echo "Docs: https://github.com/${REPO}#readme"
}

main
