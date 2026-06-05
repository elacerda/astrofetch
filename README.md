# AstroFetch

AstroFetch is a small, space-themed system information tool for your terminal.

It prints a compact summary of your machine next to procedural astrophysical ASCII art. It is designed to be simple, fast, pleasant to run on shell startup, and easy to install on Linux, macOS, and Windows.

## Preview

Run:

```bash
astrofetch
```

By default, AstroFetch randomly selects one of the available procedural visual models and renders it next to a compact system summary.

Examples:

```bash
astrofetch --model spiral
astrofetch --model elliptical
astrofetch --model cluster
astrofetch --model starfield
astrofetch --model spiral --seed 42
```

AstroFetch can show OS, kernel, uptime, packages, shell, resolution, desktop environment, window manager, themes, disk usage, CPU, GPU, and RAM when available.

For the scientific and technical background of the procedural renderer, see [`docs/procedural-galaxies.md`](docs/procedural-galaxies.md).

## Installation

### Install script

The recommended install method downloads the latest GitHub Release binary:

```bash
curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/install.sh | sh
```

Preview what the installer would do:

```bash
curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/install.sh | sh -s -- --dry-run
```

Install a specific release tag:

```bash
curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/install.sh | sh -s -- --version v0.3.1
```

Check the installed binary:

```bash
astrofetch --version
astrofetch --help
```

### Homebrew

If you use Homebrew on macOS or Linux:

```bash
brew tap elacerda/astrofetch
brew install astrofetch
```

### Source install for development

For local development or source-based installs:

```bash
git clone https://github.com/elacerda/astrofetch.git
cd astrofetch
cargo install --path .
```

This installs `astrofetch` under Cargo's binary directory, usually `~/.cargo/bin`.

## Usage

Basic commands:

```bash
astrofetch
astrofetch --compact
astrofetch --logo-only
astrofetch --info-only
astrofetch --no-color
```

Choose a visual model:

```bash
astrofetch --model random
astrofetch --model spiral
astrofetch --model elliptical
astrofetch --model cluster
astrofetch --model starfield
```

Available models:

- `random`: randomly selects one of the available models.
- `spiral`: a procedural spiral galaxy renderer.
- `elliptical`: a smooth radial galaxy model.
- `cluster`: a sparse stellar cluster-style model.
- `starfield`: a point-like star field using `.`, `*`, and `+`.

Use a fixed seed for reproducible output:

```bash
astrofetch --model spiral --seed 42
```

## Shell startup integration

AstroFetch can add a managed block to your shell startup file. The block is marked with:

```text
# >>> astrofetch >>>
# <<< astrofetch <<<
```

Preview startup integration:

```bash
astrofetch setup-shell --shell bash --dry-run
```

Install startup integration:

```bash
astrofetch setup-shell --shell bash
```

Use compact output on startup:

```bash
astrofetch setup-shell --shell bash --compact --force
```

Supported shells:

```bash
astrofetch setup-shell --shell bash
astrofetch setup-shell --shell zsh
astrofetch setup-shell --shell fish
astrofetch setup-shell --shell powershell
```

Remove startup integration:

```bash
astrofetch uninstall-shell --shell bash --dry-run
astrofetch uninstall-shell --shell bash
```

## Uninstalling

The uninstall method depends on how AstroFetch was installed.

If you installed with `install.sh`:

```bash
curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/uninstall.sh | sh
```

Also remove shell startup integration:

```bash
curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/uninstall.sh | sh -s -- --remove-shell-integration --shell bash
```

If you installed to a custom directory:

```bash
curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/uninstall.sh | sh -s -- --dir "$HOME/bin"
```

If you installed with Homebrew:

```bash
brew uninstall astrofetch
brew untap elacerda/astrofetch
```

If you installed with Cargo:

```bash
astrofetch uninstall-shell --shell bash
cargo uninstall astrofetch
```

For a development clone, you can remove the shell integration using the current source tree:

```bash
cargo run -- uninstall-shell --shell bash
```

Verify removal:

```bash
type -a astrofetch || true
which -a astrofetch || true
grep -n 'astrofetch' ~/.bashrc ~/.bash_profile ~/.profile ~/.zshrc ~/.zprofile 2>/dev/null || true
```

## Development

AstroFetch is written in Rust.

Common development commands:

```bash
cargo run
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Release binaries are built by GitHub Actions when a `v*` tag is pushed.

## Documentation

- [`docs/procedural-galaxies.md`](docs/procedural-galaxies.md): scientific and technical notes about the procedural renderer.
- [`docs/DEVELOPMENT_NOTES.md`](docs/DEVELOPMENT_NOTES.md): historical implementation notes and roadmap-style development context.

## License

MIT
