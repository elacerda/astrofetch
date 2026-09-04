#!/bin/sh
set -eu


astrofetch_startup_references() {
    found=0

    for file in \
        "$HOME/.bashrc" \
        "$HOME/.bash_profile" \
        "$HOME/.profile" \
        "$HOME/.zshrc" \
        "$HOME/.zprofile" \
        "$HOME/.config/fish/config.fish" \
        "$HOME/.config/powershell/Microsoft.PowerShell_profile.ps1" \
        "$HOME/Documents/PowerShell/Microsoft.PowerShell_profile.ps1" \
        "$HOME/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1"
    do
        if [ -f "$file" ] && grep -Eq '(^|[[:space:]])astrofetch([[:space:]]|$)' "$file"; then
            printf '%s\n' "$file"
            found=1
        fi
    done

    return "$found"
}

REPO_OWNER="elacerda"
REPO_NAME="astrofetch"
DEFAULT_INSTALL_DIR="${HOME}/.local/bin"

usage() {
    cat <<'EOF'
Install AstroFetch from GitHub Releases.

Usage:
  sh install.sh [options]

Options:
  --version <tag>  Install a specific release tag, such as v0.2.0.
  --dir <path>     Install directory. Defaults to ~/.local/bin.
  --dry-run        Print what would happen without downloading or installing.
  --help           Show this help.

Examples:
  sh install.sh
  sh install.sh --version v0.2.0
  sh install.sh --dir "$HOME/bin" --version v0.2.0
  sh install.sh --dry-run --version v0.2.0
EOF
}

die() {
    printf 'install.sh: %s\n' "$*" >&2
    exit 1
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

expand_dir() {
    case "$1" in
        "~")
            printf '%s\n' "$HOME"
            ;;
        "~/"*)
            printf '%s/%s\n' "$HOME" "${1#~/}"
            ;;
        *)
            printf '%s\n' "$1"
            ;;
    esac
}

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$arch" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        aarch64|arm64)
            arch="aarch64"
            ;;
        *)
            die "unsupported architecture: $arch"
            ;;
    esac

    case "$os" in
        Linux)
            printf '%s-unknown-linux-gnu\n' "$arch"
            ;;
        Darwin)
            printf '%s-apple-darwin\n' "$arch"
            ;;
        *)
            die "unsupported operating system: $os. install.sh supports Linux and macOS."
            ;;
    esac
}

sha256_of() {
    # Print the SHA-256 hex digest of a file.
    # Linux provides sha256sum; macOS provides shasum.
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        die "no SHA-256 tool available (need sha256sum or shasum)"
    fi
}

latest_version() {
    need_cmd curl
    api_url="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/latest"
    tag="$(curl -fsSL "$api_url" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | sed -n '1p')"
    [ -n "$tag" ] || die "could not resolve latest release tag from GitHub"
    printf '%s\n' "$tag"
}

install_dir="$DEFAULT_INSTALL_DIR"
version=""
dry_run=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --help|-h)
            usage
            exit 0
            ;;
        --version)
            [ "$#" -ge 2 ] || die "--version requires a tag"
            version="$2"
            shift 2
            ;;
        --dir)
            [ "$#" -ge 2 ] || die "--dir requires a path"
            install_dir="$2"
            shift 2
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        *)
            die "unknown option: $1"
            ;;
    esac
done

target="$(detect_target)"
install_dir="$(expand_dir "$install_dir")"

if [ -z "$version" ]; then
    version="$(latest_version)"
fi

artifact="astrofetch-${version}-${target}.tar.gz"
download_url="https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download/${version}/${artifact}"
checksums_url="https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download/${version}/SHA256SUMS"

if [ "$dry_run" -eq 1 ]; then
    cat <<EOF
AstroFetch install dry run
  version:     ${version}
  target:      ${target}
  artifact:    ${artifact}
  download:    ${download_url}
  checksum:    ${checksums_url}
  install dir: ${install_dir}

No files were downloaded or installed.
EOF
    exit 0
fi

need_cmd curl
need_cmd tar
need_cmd mktemp

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT INT HUP TERM

printf 'Downloading AstroFetch %s for %s...\n' "$version" "$target"
curl -fsSL "$download_url" -o "$tmpdir/$artifact"

printf 'Verifying checksum against release SHA256SUMS...\n'
curl -fsSL "$checksums_url" -o "$tmpdir/SHA256SUMS"
expected_sha="$(awk -v f="$artifact" '{ name=$NF; sub(/^\*/, "", name); if (name == f) { print $1; exit } }' "$tmpdir/SHA256SUMS")"
[ -n "$expected_sha" ] || die "SHA256SUMS has no entry for $artifact; refusing to install"
actual_sha="$(sha256_of "$tmpdir/$artifact")"
[ "$expected_sha" = "$actual_sha" ] || die "checksum mismatch for $artifact (expected $expected_sha, got $actual_sha)"
printf 'Checksum OK.\n'

tar -xzf "$tmpdir/$artifact" -C "$tmpdir"
[ -f "$tmpdir/astrofetch" ] || die "release artifact did not contain an astrofetch binary"

mkdir -p "$install_dir"
cp "$tmpdir/astrofetch" "$install_dir/astrofetch"
chmod +x "$install_dir/astrofetch"

printf 'AstroFetch %s installed to %s/astrofetch\n' "$version" "$install_dir"

case ":$PATH:" in
    *":$install_dir:"*)
        astrofetch_cmd="astrofetch"
        printf 'Run it with: astrofetch\n'
        ;;
    *)
        astrofetch_cmd="${install_dir}/astrofetch"
        printf '\nNote: %s is not in your PATH.\n' "$install_dir"
        printf 'Add it to your shell profile or run AstroFetch directly:\n'
        printf '  %s/astrofetch\n' "$install_dir"
        ;;
esac

echo
startup_refs="$(astrofetch_startup_references || true)"
if [ -n "$startup_refs" ]; then
    echo "Shell startup integration appears to already reference AstroFetch in:"
    printf '%s\n' "$startup_refs" | sed 's/^/  /'
    echo
    echo "To avoid duplicate startup output, inspect those files before running setup-shell again."
    echo "To preview startup changes, run:"
    echo "  $astrofetch_cmd setup-shell --shell bash --dry-run"
    echo "To remove startup integration, run:"
    echo "  $astrofetch_cmd uninstall-shell --shell bash --dry-run"
else
    echo "Shell startup integration is opt-in. To preview it, run:"
    echo "  $astrofetch_cmd setup-shell --shell bash --dry-run"
fi
