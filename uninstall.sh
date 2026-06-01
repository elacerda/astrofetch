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
  sh uninstall.sh --dir "$HOME/.local/bin"
  sh uninstall.sh --remove-shell-integration --shell bash
  sh uninstall.sh --remove-shell-integration --shell bash --dry-run
  sh uninstall.sh --remove-shell-integration --shell bash --target-path /tmp/bashrc
EOF
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
    if [ -x "$astrofetch_bin" ]; then
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

        "$astrofetch_bin" "$@"
    else
        echo "AstroFetch binary was not found at: $astrofetch_bin" >&2
        echo "Cannot remove shell startup integration automatically without the binary." >&2
        echo "Reinstall AstroFetch temporarily or remove the managed block manually." >&2
        if [ "$dry_run" -eq 0 ]; then
            exit 1
        fi
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
