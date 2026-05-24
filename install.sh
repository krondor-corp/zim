#!/bin/sh
# zim install script
# Usage: curl -fsSL https://raw.githubusercontent.com/zim/zim/main/install.sh | sh
#        curl -fsSL ... | sh -s -- --fuse
#        curl -fsSL ... | sh -s -- --version 0.1.9
set -eu

REPO="zim/zim"
BINARY="zim-peer"
INSTALL_DIR="${ZIM_INSTALL_DIR:-$HOME/.local/bin}"

# Parse arguments
VERSION=""
FUSE=""  # empty = unset, "yes" = requested, "no" = explicitly declined
while [ $# -gt 0 ]; do
  case "$1" in
    --version)
      VERSION="$2"
      shift 2
      ;;
    --version=*)
      VERSION="${1#--version=}"
      shift
      ;;
    --fuse)
      FUSE="yes"
      shift
      ;;
    --no-fuse)
      FUSE="no"
      shift
      ;;
    --help|-h)
      echo "Usage: install.sh [OPTIONS]"
      echo ""
      echo "Install or update zim from GitHub releases."
      echo ""
      echo "Options:"
      echo "  --version VERSION  Install a specific version (default: latest)"
      echo "  --fuse             Install FUSE variant (macOS Apple Silicon only)"
      echo "  --no-fuse          Install without FUSE support (skip prompt)"
      echo ""
      echo "Environment:"
      echo "  ZIM_INSTALL_DIR    Installation directory (default: ~/.local/bin)"
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

# Detect OS
detect_os() {
  case "$(uname -s)" in
    Darwin) echo "darwin" ;;
    Linux)  echo "linux" ;;
    *)
      echo "Unsupported OS: $(uname -s)" >&2
      exit 1
      ;;
  esac
}

# Detect architecture
detect_arch() {
  case "$(uname -m)" in
    arm64|aarch64) echo "arm64" ;;
    x86_64|amd64)  echo "x64" ;;
    *)
      echo "Unsupported architecture: $(uname -m)" >&2
      exit 1
      ;;
  esac
}

# Check if FUSE is supported on this platform
fuse_supported() {
  [ "$OS" = "darwin" ] && [ "$ARCH" = "arm64" ]
}

OS="$(detect_os)"
ARCH="$(detect_arch)"

# Validate platform support
if [ "$OS" = "linux" ] && [ "$ARCH" = "arm64" ]; then
  echo "Error: Linux ARM64 binaries are not yet available." >&2
  echo "Install from source: cargo install zim-peer" >&2
  exit 1
fi

# Handle FUSE variant selection
if [ "$FUSE" = "yes" ]; then
  if ! fuse_supported; then
    echo "Error: FUSE builds are only available for macOS Apple Silicon." >&2
    echo "Your platform: ${OS}/${ARCH}" >&2
    exit 1
  fi
elif [ -z "$FUSE" ] && fuse_supported; then
  # No flag provided and platform supports FUSE — prompt if interactive
  if [ -t 0 ] && [ -t 1 ]; then
    printf "FUSE mount support is available for your platform (requires macFUSE).\n"
    printf "Install FUSE variant? [y/N] "
    read -r answer
    case "$answer" in
      [yY]|[yY][eE][sS]) FUSE="yes" ;;
      *) FUSE="no" ;;
    esac
  else
    # Non-interactive (piped) — default to no FUSE
    FUSE="no"
  fi
fi

echo "Detected platform: ${OS}/${ARCH}"
if [ "$FUSE" = "yes" ]; then
  echo "Variant: FUSE"
fi

# Resolve version
if [ -z "$VERSION" ]; then
  echo "Fetching latest version..."
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases" \
    | grep -o '"tag_name": *"zim-peer-v[^"]*"' \
    | head -1 \
    | sed 's/.*"zim-peer-v\([^"]*\)".*/\1/')

  if [ -z "$VERSION" ]; then
    echo "Error: Could not determine latest version." >&2
    echo "Check https://github.com/${REPO}/releases for available versions." >&2
    exit 1
  fi
fi

echo "Installing zim v${VERSION}..."

# Build download URL
SUFFIX=""
if [ "$FUSE" = "yes" ]; then
  SUFFIX="-fuse"
fi
ARTIFACT="zim-${OS}-${ARCH}${SUFFIX}-${VERSION}"
URL="https://github.com/${REPO}/releases/download/zim-peer-v${VERSION}/${ARTIFACT}"

# Download binary
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading ${URL}..."
if ! curl -fSL --progress-bar -o "${TMPDIR}/zim" "$URL"; then
  echo "Error: Download failed." >&2
  if [ "$FUSE" = "yes" ]; then
    echo "The FUSE variant may not be available for this version." >&2
    echo "Try without --fuse, or check:" >&2
  else
    echo "Check that version ${VERSION} exists at:" >&2
  fi
  echo "  https://github.com/${REPO}/releases/tag/zim-peer-v${VERSION}" >&2
  exit 1
fi

chmod +x "${TMPDIR}/zim"

# Install
mkdir -p "$INSTALL_DIR"
mv "${TMPDIR}/zim" "${INSTALL_DIR}/zim"

echo "Installed zim to ${INSTALL_DIR}/zim"
if [ "$FUSE" = "yes" ]; then
  echo "  (FUSE variant — requires macFUSE: https://osxfuse.github.io/)"
fi

# Check PATH
case ":${PATH}:" in
  *":${INSTALL_DIR}:"*)
    ;;
  *)
    echo ""
    echo "Add ${INSTALL_DIR} to your PATH:"
    echo ""
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo ""
    echo "Add the above line to your shell profile (~/.bashrc, ~/.zshrc, etc.)"
    ;;
esac

echo ""
echo "Run 'zim --help' to get started."
