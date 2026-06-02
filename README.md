# AstroFetch

AstroFetch is a small, space-themed system information tool for your terminal.

It prints a compact summary of your machine next to an ASCII space logo. It is designed to be simple, fast, and pleasant to run when opening a shell.

## Preview

Run:

```bash
astrofetch
```

By default, AstroFetch renders a procedural spiral galaxy next to a compact system summary.

Example visual models:

```bash
astrofetch --model spiral
astrofetch --model elliptical
astrofetch --model cluster
astrofetch --model starfield
```

Use `--seed` when you want reproducible output for screenshots, demos, or visual checks:

```bash
astrofetch --model spiral --seed 42
```

AstroFetch shows basic system information such as OS, kernel, uptime, disk usage, memory, shell, terminal, desktop environment, CPU, GPU, and RAM when available.

## Installation

AstroFetch is meant to be easy to install on personal laptops, observatory workstations, and research environments where scientists want a quick terminal summary without thinking about Rust tooling.

### Install script (recommended)

The recommended installation method is the install script:

```bash
curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/install.sh | bash
```

After installing, restart your terminal or reload your shell configuration if needed.

Check that AstroFetch is available:

```bash
astrofetch --help
```

### Homebrew

If you use Homebrew on macOS or Linux, you can install AstroFetch with:

```bash
brew tap elacerda/astrofetch
brew install astrofetch
```

Homebrew is a good option for researchers who already use it to manage command-line tools across notebooks, lab machines, or shared scientific workstations.

### Install from a local clone

If you cloned the repository, run:

```bash
git clone https://github.com/elacerda/astrofetch.git
cd astrofetch
./install.sh
```

## Run AstroFetch when opening Bash

To show AstroFetch automatically when opening a Bash terminal, add it to your `~/.bashrc`.

Minimal setup:

```bash
cat >> ~/.bashrc <<'EOF_BASHRC'

# AstroFetch
if command -v astrofetch >/dev/null 2>&1; then
    astrofetch
fi
EOF_BASHRC
```

Then reload Bash:

```bash
source ~/.bashrc
```

For compact output on shell startup, use this block instead:

```bash
cat >> ~/.bashrc <<'EOF_BASHRC'

# AstroFetch
if command -v astrofetch >/dev/null 2>&1; then
    astrofetch --compact
fi
EOF_BASHRC
```

## Usage

Default output:

```bash
astrofetch
```

Compact output:

```bash
astrofetch --compact
```

Choose a visual model:

```bash
astrofetch --model spiral
astrofetch --model elliptical
astrofetch --model cluster
astrofetch --model starfield
```

Available models:

- `spiral`: the default procedural galaxy renderer.
- `elliptical`: a smooth radial galaxy model.
- `cluster`: a sparse stellar cluster-style model.
- `starfield`: a point-like field using `.`, `*`, and `+` instead of diffuse galaxy blocks.

Use a fixed seed for reproducible output:

```bash
astrofetch --model spiral --seed 42
```

Logo only:

```bash
astrofetch --logo-only
```

Disable colors:

```bash
astrofetch --no-color
```

Show help:

```bash
astrofetch --help
```

## Uninstalling

If you installed AstroFetch using the install script, remove the installed binary manually:

```bash
rm -f ~/.local/bin/astrofetch
```

If you added AstroFetch to your `~/.bashrc`, remove the AstroFetch block from that file.

## Development

AstroFetch is written in Rust.

For development from source:

```bash
cargo run
cargo test
```

Formatting and linting:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

Building a release binary:

```bash
cargo build --release
```

## License

MIT
