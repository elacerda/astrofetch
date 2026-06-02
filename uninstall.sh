#!/bin/sh
set -eu

DEFAULT_INSTALL_DIR="${HOME}/.local/bin"

install_dir="${ASTROFETCH_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
remove_shell_integration=0
shell_name=""
target_path=""
dry_run=0

usage() {
    cat <<'EOF'
AstroFetch uninstaller

Usage:
  uninstall.sh [OPTIONS]

Options:
  --dir DIR                     Directory containing the astrofetch binary
                                [default: ~/.local/bin]
  --remove-shell-integration    Also remove AstroFetch shell startup integration
  --shell SHELL                 Shell to update when removing startup integration
                                Values: bash, zsh, fish, powershell
  --target-path PATH            Advanced startup file override for shell integration removal
  --dry-run                     Show what would be removed without changing files
  -h, --help                    Show this help

Examples:
  curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/uninstall.sh | sh
  curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/uninstall.sh | sh -s -- --dry-run
  curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/uninstall.sh | sh -s -- --remove-shell-integration --shell bash
  sh uninstall.sh --dir "$HOME/.local/bin"
  sh uninstall.sh --remove-shell-integration --shell bash
  sh uninstall.sh --remove-shell-integration --shell bash --dry-run
  sh uninstall.sh --remove-shell-integration --shell bash --target-path /tmp/bashrc

Notes:
  This script removes binaries installed by install.sh from --dir.
  If AstroFetch was installed with Cargo, also run:
    cargo uninstall astrofetch
  If AstroFetch was installed with Homebrew, use:
    brew uninstall astrofetch
EOF
}

command_exists() {
    command -v "$1" >/dev/null 2>&1
}

home_dir() {
    if [ -n "${HOME:-}" ]; then
        printf '%s\n' "$HOME"
    elif [ -n "${USERPROFILE:-}" ]; then
        printf '%s\n' "$USERPROFILE"
    else
        return 1
    fi
}

infer_shell() {
    if [ -n "$shell_name" ]; then
        printf '%s\n' "$shell_name"
        return 0
    fi

    if [ -n "${SHELL:-}" ]; then
        case "${SHELL##*/}" in
            bash)
                printf '%s\n' bash
                return 0
                ;;
            zsh)
                printf '%s\n' zsh
                return 0
                ;;
            fish)
                printf '%s\n' fish
                return 0
                ;;
        esac
    fi

    return 1
}

default_target_path() {
    resolved_shell="$1"
    home="$(home_dir)" || return 1

    case "$resolved_shell" in
        bash)
            printf '%s\n' "$home/.bashrc"
            ;;
        zsh)
            printf '%s\n' "$home/.zshrc"
            ;;
        fish)
            printf '%s\n' "$home/.config/fish/config.fish"
            ;;
        powershell)
            if [ -n "${USERPROFILE:-}" ]; then
                printf '%s\n' "$USERPROFILE/Documents/PowerShell/Microsoft.PowerShell_profile.ps1"
            else
                printf '%s\n' "$home/.config/powershell/Microsoft.PowerShell_profile.ps1"
            fi
            ;;
        *)
            return 1
            ;;
    esac
}

fallback_remove_shell_integration() {
    if [ -n "$target_path" ]; then
        startup_file="$target_path"
        resolved_shell="${shell_name:-custom}"
    else
        resolved_shell="$(infer_shell)" || {
            echo "Cannot infer shell startup file." >&2
            echo "Pass --shell bash, --shell zsh, --shell fish, or --shell powershell." >&2
            return 1
        }
        startup_file="$(default_target_path "$resolved_shell")" || {
            echo "Cannot determine startup file for shell: $resolved_shell" >&2
            echo "Pass --target-path PATH." >&2
            return 1
        }
    fi

    if [ ! -f "$startup_file" ]; then
        echo "AstroFetch startup integration is not installed."
        echo "Target startup file: $startup_file"
        return 0
    fi

    if ! grep -q '# >>> astrofetch >>>' "$startup_file"; then
        echo "AstroFetch managed startup integration is not installed."
        echo "Target startup file: $startup_file"
        echo "If you added AstroFetch manually, remove that custom block yourself."
        return 0
    fi

    if [ "$dry_run" -eq 1 ]; then
        echo "Shell: $resolved_shell"
        echo "Target startup file: $startup_file"
        echo "AstroFetch startup integration would be removed."
        return 0
    fi

    tmp_file="$(mktemp)"
    set +e
    awk '
        /# >>> astrofetch >>>/ {
            in_block = 1
            removed = 1
            next
        }
        /# <<< astrofetch <<</ {
            if (in_block) {
                in_block = 0
                next
            }
        }
        !in_block {
            print
        }
        END {
            if (in_block) {
                exit 2
            }
            if (removed) {
                exit 0
            }
            exit 3
        }
    ' "$startup_file" > "$tmp_file"
    awk_status=$?
    set -e

    case "$awk_status" in
        0)
            mv "$tmp_file" "$startup_file"
            echo "AstroFetch startup integration removed."
            echo "Target startup file: $startup_file"
            ;;
        2)
            rm -f "$tmp_file"
            echo "Found an AstroFetch start marker without a matching end marker." >&2
            echo "Please fix the file manually: $startup_file" >&2
            return 1
            ;;
        3)
            rm -f "$tmp_file"
            echo "AstroFetch startup integration is not installed."
            echo "Target startup file: $startup_file"
            ;;
        *)
            rm -f "$tmp_file"
            echo "Could not update startup file: $startup_file" >&2
            return 1
            ;;
    esac
}

find_uninstall_shell_binary() {
    if [ -x "$astrofetch_bin" ] && "$astrofetch_bin" uninstall-shell --help >/dev/null 2>&1; then
        printf '%s\n' "$astrofetch_bin"
        return 0
    fi

    if command_exists astrofetch; then
        path_bin="$(command -v astrofetch)"
        if [ "$path_bin" != "$astrofetch_bin" ] && "$path_bin" uninstall-shell --help >/dev/null 2>&1; then
            printf '%s\n' "$path_bin"
            return 0
        fi
    fi

    return 1
}

run_uninstall_shell() {
    tool="$1"

    if [ "$dry_run" -eq 1 ]; then
        set -- uninstall-shell --dry-run
    else
        set -- uninstall-shell
    fi

    if [ -n "$shell_name" ]; then
        set -- "$@" --shell "$shell_name"
    fi

    if [ -n "$target_path" ]; then
        set -- "$@" --target-path "$target_path"
    fi

    "$tool" "$@"
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dir)
            if [ "$#" -lt 2 ]; then
                echo "Error: --dir requires a value." >&2
                exit 2
            fi
            install_dir="$2"
            shift 2
            ;;
        --remove-shell-integration)
            remove_shell_integration=1
            shift
            ;;
        --shell)
            if [ "$#" -lt 2 ]; then
                echo "Error: --shell requires a value." >&2
                exit 2
            fi
            shell_name="$2"
            shift 2
            ;;
        --target-path)
            if [ "$#" -lt 2 ]; then
                echo "Error: --target-path requires a value." >&2
                exit 2
            fi
            target_path="$2"
            shift 2
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Error: unexpected argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

case "$shell_name" in
    ""|bash|zsh|fish|powershell)
        ;;
    *)
        echo "Error: unsupported shell: $shell_name" >&2
        echo "Supported shells: bash, zsh, fish, powershell" >&2
        exit 2
        ;;
esac

astrofetch_bin="${install_dir%/}/astrofetch"

if [ "$remove_shell_integration" -eq 1 ]; then
    if shell_tool="$(find_uninstall_shell_binary)"; then
        run_uninstall_shell "$shell_tool"
    else
        echo "Could not find an AstroFetch binary with uninstall-shell support."
        echo "Falling back to direct removal of managed startup blocks."
        fallback_remove_shell_integration
    fi
fi

if [ "$dry_run" -eq 1 ]; then
    if [ -e "$astrofetch_bin" ]; then
        echo "Would remove AstroFetch binary: $astrofetch_bin"
    else
        echo "AstroFetch binary is not installed at: $astrofetch_bin"
    fi
    exit 0
fi

if [ -e "$astrofetch_bin" ]; then
    rm -f "$astrofetch_bin"
    echo "Removed AstroFetch binary: $astrofetch_bin"
else
    echo "AstroFetch binary is not installed at: $astrofetch_bin"
fi

echo
echo "AstroFetch uninstall complete."

if [ "$remove_shell_integration" -eq 0 ]; then
    echo "Shell startup integration was not removed."
    echo "To remove it too, run:"
    echo "  sh uninstall.sh --remove-shell-integration --shell bash"
fi
