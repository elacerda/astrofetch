# AstroFetch

**AstroFetch** is a small Rust terminal fetch app inspired by `screenFetch`.
Instead of showing a static distro logo, it prints procedural astrophysical ASCII
art next to a screenFetch-like system panel.

It is personal, lightweight, and a little starry on purpose: useful enough to run
in a shell, but mostly built for the joy of making terminal output feel alive.

## Features

- Procedural ASCII art models: random, elliptical galaxy, spiral galaxy, cluster,
  and starfield.
- ScreenFetch-like system info in full mode:
  OS, Kernel, Uptime, Packages, Shell, Resolution, DE, WM, themes, Disk, CPU,
  GPU, and RAM.
- Aligned label/value styling in the info panel, with subtle label color by
  default.
- Compact mode with the stable core fields:
  OS, Kernel, Uptime, Disk, CPU, RAM.
- Optional per-filesystem disk details with `--disk-details` on Linux.
- `--info-only` and `--logo-only` modes for scripts, screenshots, and quick checks.
- Deterministic seeds for reproducible art.
- Optional ANSI color for art and info labels, with `--no-color` support.
- Best-effort platform behavior: unavailable local commands or settings simply
  omit optional fields.

## Install

### Recommended: install a release binary

For regular Linux and macOS users, install the latest GitHub Release binary with:

```bash
curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/install.sh | sh
```

To install a specific release:

```bash
curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/install.sh | sh -s -- --version v0.2.0
```

The installer downloads a prebuilt binary into `~/.local/bin` by default. It
does not edit shell startup files or run `astrofetch setup-shell` automatically.

If you do not like piping scripts into `sh`, download and inspect the installer
first:

```bash
curl -fsSLO https://raw.githubusercontent.com/elacerda/astrofetch/main/install.sh
less install.sh
sh install.sh
```

### Manual binary installation

Download the artifact for your platform from
[GitHub Releases](https://github.com/elacerda/astrofetch/releases):

- Linux x86_64: `astrofetch-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
- Linux arm64: `astrofetch-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz`
- macOS x86_64: `astrofetch-vX.Y.Z-x86_64-apple-darwin.tar.gz`
- macOS arm64: `astrofetch-vX.Y.Z-aarch64-apple-darwin.tar.gz`
- Windows x86_64: `astrofetch-vX.Y.Z-x86_64-pc-windows-msvc.zip`

For Linux or macOS:

```bash
tar -xzf astrofetch-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz
mkdir -p "$HOME/.local/bin"
mv astrofetch "$HOME/.local/bin/"
astrofetch
```

For Windows, download the zip file, extract `astrofetch.exe`, and place it in a
directory that is on your `PATH`.

### Rust developer installation

AstroFetch is not yet published to crates.io, so this does not work yet:

```bash
cargo install astrofetch
```

For now, Rust users can install from a local checkout:

```bash
git clone https://github.com/elacerda/astrofetch.git
cd astrofetch
cargo install --path .
```

If `cargo install --path .` succeeds but `astrofetch` is not found, make sure
Cargo's binary directory is in your `PATH`:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
astrofetch
```

### From source

If you do not already have a working Rust toolchain, install Rust with `rustup`
first. On Linux, macOS, or WSL:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Build a release binary:

```bash
git clone https://github.com/elacerda/astrofetch.git
cd astrofetch
cargo build --release
./target/release/astrofetch
```

## Shell Startup Integration

Installing the `astrofetch` binary does not automatically add it to your shell
startup files (e.g., `~/.bashrc`, `~/.zshrc`, fish config, or a PowerShell
profile). Startup integration is explicitly opt-in.

If you want AstroFetch to run automatically when you open a new interactive
terminal, use `setup-shell`. Start with `--dry-run` to see the target file and
the exact managed block before anything is written:

```bash
astrofetch setup-shell --shell bash --dry-run
```

Then install the managed block:

```bash
astrofetch setup-shell --shell bash
```

For compact startup output:

```bash
astrofetch setup-shell --shell bash --compact
```

To remove AstroFetch from shell startup without deleting the binary:

```bash
astrofetch uninstall-shell --shell bash --dry-run
astrofetch uninstall-shell --shell bash
```

Other shells:

```bash
astrofetch setup-shell --shell zsh --dry-run
astrofetch setup-shell --shell fish --dry-run
astrofetch setup-shell --shell powershell --dry-run
```

Startup removal supports the same shells:

```bash
astrofetch uninstall-shell --shell zsh --dry-run
astrofetch uninstall-shell --shell fish --dry-run
astrofetch uninstall-shell --shell powershell --dry-run
```

`--dry-run` prints the selected shell, target startup file, and block without
writing files. If a managed AstroFetch block already exists, `setup-shell` will
not duplicate it; use `--force` to replace only that managed block while
preserving the rest of the file.

`uninstall-shell` removes AstroFetch startup integration only. It removes the
managed block and known legacy AstroFetch startup snippets, but it does not
delete the `astrofetch` binary.

Advanced/manual testing option:

```bash
astrofetch setup-shell --shell bash --target-path /tmp/astrofetch-shell-test --dry-run
```

Manual snippets are still fine if you prefer editing shell configuration
yourself.

### Bash (~/.bashrc)

```bash
if [[ $- == *i* ]] && command -v astrofetch >/dev/null 2>&1; then
    astrofetch
fi
```

The `[[ $- == *i* ]]` guard ensures AstroFetch only runs in interactive shells,
preventing issues in non-interactive contexts (e.g., SSH commands, scripts).

For a compact output, you can use:

```bash
if [[ $- == *i* ]] && command -v astrofetch >/dev/null 2>&1; then
    astrofetch --compact
fi
```

### Zsh (~/.zshrc)

```zsh
if [[ -o interactive ]] && command -v astrofetch >/dev/null 2>&1; then
    astrofetch
fi
```

Or use the compact form:

```zsh
if [[ -o interactive ]] && command -v astrofetch >/dev/null 2>&1; then
    astrofetch --compact
fi
```

### Fish (~/.config/fish/config.fish)

```fish
if status is-interactive; and command -q astrofetch
    astrofetch
end
```

### PowerShell profile

```powershell
if ($Host.Name -eq "ConsoleHost" -and (Get-Command astrofetch -ErrorAction SilentlyContinue)) {
    astrofetch
}
```

After editing your shell config, reload it with `source ~/.bashrc` (or
`source ~/.zshrc`) or open a new terminal to see the changes. For fish and
PowerShell, opening a new terminal is usually the simplest check.

## Usage

### Disk details

To show the aggregate disk line plus per-filesystem details on Linux:

```bash
astrofetch --disk-details
```

Example disk detail output:

```text
Disk:           145.6G / 420.8G (35%)
Disk /:         22.7G / 45.5G (50%)
Disk /home:     123.0G / 374.8G (33%)
Disk /boot/efi: 16.3M / 511.0M (3%)
```

On non-Linux platforms, `--disk-details` currently preserves the standard disk output without extra per-filesystem lines.


```bash
astrofetch
```

```bash
astrofetch --info-only
```

```bash
astrofetch --compact
```

```bash
astrofetch --logo-only --model spiral --width 40 --height 16 --seed 42
```

```bash
astrofetch --no-color
```

Useful discovery commands:

```bash
astrofetch --help
astrofetch --version
```

## Optional Fields

Some fields depend on local commands or desktop settings. On Linux, AstroFetch
uses best-effort probes such as `dpkg-query`, `xrandr`, `lspci`, and `gsettings`
when they are available.

If a probe fails, is missing, or returns unusable output, AstroFetch omits that
field instead of filling the terminal with `N/A`.

## Platform Support

- Linux: actively tested locally and in CI.
- macOS: build, clippy, and test validation enabled in CI.
- Windows: build, clippy, and test validation enabled in CI.

## Development

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

Runtime checks:

```bash
cargo run -- --help
cargo run -- --version
cargo run -- --info-only
cargo run -- --compact
cargo run --
cargo run -- setup-shell --help
cargo run -- setup-shell --shell bash --dry-run
```

Release sanity:

```bash
cargo build --release
./target/release/astrofetch --version
./target/release/astrofetch --info-only
./target/release/astrofetch --compact
```

## Project Shape

```text
src/
  main.rs
  cli.rs
  app.rs
  engine.rs
  render.rs
  layout.rs
  setup_shell.rs
  terminal.rs
  system.rs
  error.rs
```

- `cli.rs`: command-line arguments with `clap`.
- `app.rs`: top-level application flow.
- `engine.rs`: procedural brightness models.
- `render.rs`: numeric canvas to ASCII.
- `layout.rs`: side-by-side composition.
- `setup_shell.rs`: opt-in managed shell startup integration.
- `terminal.rs`: terminal width, ANSI, TTY, and color handling.
- `system.rs`: best-effort system information collection.
- `error.rs`: recoverable application errors.

## Philosophy

AstroFetch is not trying to replace mature tools like `fastfetch` or
`screenFetch`. It is a small, hackable terminal toy with enough practical polish
to be pleasant in daily use.
