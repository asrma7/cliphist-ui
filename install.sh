#!/bin/sh
set -eu

APP_NAME="cliphist-ui"
REPO="${CLIPHIST_UI_REPO:-asrma7/cliphist-ui}"
VERSION="latest"
PREFIX="${PREFIX:-$HOME/.local}"
UNINSTALL=0

usage() {
    cat <<EOF
Usage: install.sh [OPTIONS]

Install cliphist-ui from GitHub Releases.

Options:
  --prefix PATH       Install under PATH/bin (default: \$HOME/.local)
  --system            Install under /usr/local
  --version VERSION   Install a specific release, such as v0.1.0 or 0.1.0
  --uninstall         Remove the installed binary
  -h, --help          Show this help

Environment:
  CLIPHIST_UI_REPO    GitHub repo to download from (default: asrma7/cliphist-ui)
  PREFIX              Install prefix (default: \$HOME/.local)
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix)
            [ "$#" -ge 2 ] || {
                echo "error: --prefix requires a path" >&2
                exit 2
            }
            PREFIX="$2"
            shift 2
            ;;
        --system)
            PREFIX="/usr/local"
            shift
            ;;
        --version)
            [ "$#" -ge 2 ] || {
                echo "error: --version requires a version" >&2
                exit 2
            }
            VERSION="$2"
            shift 2
            ;;
        --uninstall)
            UNINSTALL=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "error: unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

BIN_DIR="$PREFIX/bin"
BIN_PATH="$BIN_DIR/$APP_NAME"

if [ "$UNINSTALL" -eq 1 ]; then
    if [ -e "$BIN_PATH" ]; then
        rm -f "$BIN_PATH"
        echo "Removed $BIN_PATH"
    else
        echo "$BIN_PATH is not installed"
    fi
    exit 0
fi

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "error: required command not found: $1" >&2
        exit 1
    fi
}

download() {
    url="$1"
    out="$2"

    if command -v curl >/dev/null 2>&1; then
        curl -fL --retry 3 --connect-timeout 15 -o "$out" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -O "$out" "$url"
    else
        echo "error: install requires curl or wget" >&2
        exit 1
    fi
}

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"

    if [ "$os" != "Linux" ]; then
        echo "error: $APP_NAME release binaries are currently Linux-only" >&2
        exit 1
    fi

    case "$arch" in
        x86_64|amd64)
            echo "x86_64-unknown-linux-gnu"
            ;;
        aarch64|arm64)
            echo "aarch64-unknown-linux-gnu"
            ;;
        *)
            echo "error: unsupported architecture: $arch" >&2
            exit 1
            ;;
    esac
}

release_base_url() {
    if [ "$VERSION" = "latest" ]; then
        echo "https://github.com/$REPO/releases/latest/download"
    else
        case "$VERSION" in
            v*) tag="$VERSION" ;;
            *) tag="v$VERSION" ;;
        esac
        echo "https://github.com/$REPO/releases/download/$tag"
    fi
}

check_runtime_dependencies() {
    missing_cmds=""
    missing_libs=""

    if ! command -v cliphist >/dev/null 2>&1; then
        missing_cmds="$missing_cmds cliphist"
    fi
    if ! command -v wl-copy >/dev/null 2>&1; then
        missing_cmds="$missing_cmds wl-copy"
    fi

    if command -v ldconfig >/dev/null 2>&1; then
        if ! ldconfig -p 2>/dev/null | grep -q 'libgtk-4\.so'; then
            missing_libs="$missing_libs GTK4"
        fi
        if ! ldconfig -p 2>/dev/null | grep -q 'libgtk4-layer-shell'; then
            missing_libs="$missing_libs gtk4-layer-shell"
        fi
    else
        echo "warning: could not check GTK runtime libraries because ldconfig is unavailable" >&2
    fi

    if [ -n "$missing_cmds$missing_libs" ]; then
        cat >&2 <<EOF

warning: installed $APP_NAME, but some runtime dependencies may be missing:
  commands:$missing_cmds
  libraries:$missing_libs

Install hints:
  Arch:   sudo pacman -S cliphist wl-clipboard gtk4 gtk4-layer-shell
  Fedora: sudo dnf install cliphist wl-clipboard gtk4 gtk4-layer-shell
  Debian/Ubuntu: install cliphist, wl-clipboard, GTK4, and gtk4-layer-shell packages available for your release

EOF
    fi
}

require_cmd uname
require_cmd tar
require_cmd sha256sum

TARGET="$(detect_target)"
ASSET="$APP_NAME-$TARGET.tar.gz"
BASE_URL="$(release_base_url)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

echo "Downloading $APP_NAME for $TARGET from $REPO..."
download "$BASE_URL/$ASSET" "$TMP_DIR/$ASSET"
download "$BASE_URL/sha256sums.txt" "$TMP_DIR/sha256sums.txt"

(
    cd "$TMP_DIR"
    grep "  $ASSET\$" sha256sums.txt | sha256sum -c -
)

tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"

if [ ! -f "$TMP_DIR/$APP_NAME" ]; then
    echo "error: release archive did not contain $APP_NAME" >&2
    exit 1
fi

mkdir -p "$BIN_DIR"
install -m 0755 "$TMP_DIR/$APP_NAME" "$BIN_PATH"

echo "Installed $APP_NAME to $BIN_PATH"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
        echo "note: $BIN_DIR is not in PATH"
        echo "      add it to your shell profile or run: $BIN_PATH"
        ;;
esac

check_runtime_dependencies
