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
- Compact mode with the stable core fields:
  OS, Kernel, Uptime, Disk, CPU, RAM.
- `--info-only` and `--logo-only` modes for scripts, screenshots, and quick checks.
- Deterministic seeds for reproducible art.
- Optional ANSI color, with `--no-color` support.
- Best-effort platform behavior: unavailable local commands or settings simply
  omit optional fields.

## Install

Build a release binary:

```bash
cargo build --release
```

Install from this checkout:

```bash
cargo install --path .
```

After install, run:

```bash
astrofetch
```

## Usage

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
  terminal.rs
  system.rs
  error.rs
```

- `cli.rs`: command-line arguments with `clap`.
- `app.rs`: top-level application flow.
- `engine.rs`: procedural brightness models.
- `render.rs`: numeric canvas to ASCII.
- `layout.rs`: side-by-side composition.
- `terminal.rs`: terminal width, ANSI, TTY, and color handling.
- `system.rs`: best-effort system information collection.
- `error.rs`: recoverable application errors.

## Philosophy

AstroFetch is not trying to replace mature tools like `fastfetch` or
`screenFetch`. It is a small, hackable terminal toy with enough practical polish
to be pleasant in daily use.
