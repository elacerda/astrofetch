# Animation architecture gate

This note records the smallest implementation path for transient AstroFetch animation from the v1.0.1 baseline. It deliberately avoids production animation code; the purpose is to freeze insertion points and compatibility constraints before Milestone 2.

## Baseline findings

AstroFetch already depends on `crossterm 0.29`, and `src/terminal.rs` already owns terminal capability detection and output through the cross-platform `queue!` API. `Terminal::new()` detects whether stdout is a TTY; non-TTY output disables colors and terminal dimensions fall back to `None`.

Therefore no new terminal abstraction library is required for animation. The existing `Terminal` layer is the correct owner for cursor visibility, cursor movement, line clearing, flushing, and terminal-state restoration.

The application currently builds final output lines in `src/app.rs` and sends them to `Terminal::print_lines`. Scene resolution and density generation happen inside `App::render_art`. Background galaxy stars are renderer-level, deterministic, hash-derived overlays. The dedicated Starfield model stores sparse point brightnesses in the density map and converts those brightnesses to glyphs in `src/render/starfield.rs`.

## Required separation

Keep three responsibilities separate:

```text
resolved/prepared scene
        +
animation frame context
        ↓
renderer produces Vec<String>
        ↓
terminal presentation redraws those lines in place
```

Terminal code must not know anything about galaxy morphology. Generation/render code must not know about Linux/macOS/Windows cursor semantics.

## Minimal Milestone 2 design

The first implementation should add only an opt-in `--animate` path and a terminal frame runner. It should be capable of replaying a fixed frame safely before any star brightness changes are introduced.

Suggested concepts (names are not contractual):

```text
AnimationConfig
AnimationPhase / AnimationFrameContext
AnimationSession or CursorGuard
```

The terminal runner should:

- run only when stdout is interactive;
- hide the cursor while frames are being replaced;
- keep the same terminal region and line count for every frame;
- move back to the beginning of the previous frame rather than clearing the full screen;
- clear/overwrite individual lines so shorter ANSI strings cannot leave stale characters;
- show the cursor again before returning;
- leave the final frame frozen exactly where normal AstroFetch output would remain;
- fall back to one ordinary static frame when stdout is redirected or piped.

Do not use the alternate screen, shell utilities, a persistent background thread, or terminal scroll-region ownership.

## Important seed-resolution constraint

`App::render_art` currently resolves/generates a scene internally. Recalling it independently for every frame is unsafe when no explicit `--seed` is supplied because `resolve_scene(None)` can choose a new random seed each time.

For the first runner this can be avoided simply by generating the static output once and replaying the same `Vec<String>`.

Before deterministic twinkling is added, the art pipeline should be extracted just enough that a scene is resolved exactly once and reused across all frames. A small prepared-art/frame-rendering object is preferable to repeatedly invoking the current top-level `render_art` path.

This is the main architectural reason animation phase belongs beside rendering rather than by wrapping repeated full application executions.

## Static compatibility rule

The non-animated path should remain byte-for-byte on its existing code path wherever practical.

A safe pattern is:

```text
render_art(...)                  -> legacy/static behavior
render_art_frame(..., context)   -> animation-only behavior
```

or an equivalent design where a `None`/static frame context provably delegates to the legacy rendering functions unchanged.

Milestone 2 itself should not change any astrophysical density, star decision, glyph distribution, normalization, layout, or palette behavior.

## Twinkling insertion points for Milestone 3

### Galaxy-background stars

All galaxy renderers ultimately share the deterministic background-star decision in `star_glyph_for_local_density` / related helpers. This is the best insertion point for temporal star appearance because it already centralizes the star policy for HalfBlock, Shade, ASCII, and star-aware Quadrant rendering.

The base existence/location of a star should remain stable. Animation should primarily modulate apparent brightness/glyph tier rather than repeatedly create and destroy stars. Temporal parameters can be derived from a versioned feature seed plus terminal-cell coordinates.

Suggested namespace:

```text
animation/star-twinkle/v1
```

### Dedicated Starfield

`src/render/starfield.rs` maps sparse density values into ` `, `.`, `*`, and `+`. For twinkling, keep the generated star positions fixed and apply a deterministic temporal modifier at render time. This allows Milestone 3 to animate without regenerating the density map.

Starfield spatial motion belongs later and may require a generation/frame layer rather than only a renderer modifier.

## Determinism

Animation must resolve the scene seed once. Per-star temporal parameters should come from isolated deterministic derivation and must not consume or advance the legacy scene RNG.

Desired contract:

```text
same scene seed + same animation config + same frame index
    => same frame bytes
```

The existing static frame remains the reference behavior when animation is not requested.

## Cursor restoration and interruption

A normal RAII guard can restore the cursor on ordinary return and panic-unwind paths. However, default Ctrl+C process termination does not guarantee Rust destructors run.

Milestone 2 must therefore explicitly solve interruption before cursor hiding is considered complete. Two approaches are possible:

1. a small cross-platform Ctrl+C/signal handler that restores cursor visibility before termination; or
2. an input/raw-mode loop that captures Ctrl+C itself.

The first option is likely smaller and less invasive. Raw mode should not be introduced merely for animation unless testing shows it is necessary.

The gate remains: Ctrl+C during animation must not leave the user's cursor hidden.

## Redirected output behavior

The existing `Terminal::new()` already provides the necessary TTY signal. The animation decision should use that existing capability rather than independently probing platform-specific terminal state.

Expected behavior:

```bash
astrofetch --animate > output.txt
astrofetch --animate | less
```

Both commands must emit exactly one ordinary static output frame with no cursor-control frame stream.

## Initial timing defaults

Do not expose FPS/duration configuration in the first infrastructure patch unless needed for testing. Start with internal conservative values suitable for a short intro, then tune after visual work.

A practical initial experiment is roughly 8–12 FPS for about 0.6–1.0 s, but these values are not accepted product defaults yet.

## Milestone 2 implementation sequence

1. add and test the explicit `--animate` CLI flag;
2. add terminal frame replacement primitives in `src/terminal.rs`;
3. add cursor restoration guard;
4. solve Ctrl+C restoration;
5. route interactive `--animate` through a short fixed-frame loop;
6. make non-TTY `--animate` fall back to the legacy one-frame output;
7. verify ordinary static mode remains unchanged;
8. only then introduce animation-aware render contexts for Milestone 3.

## Gate

Milestone 2 is accepted only when:

- ordinary AstroFetch output is unchanged without `--animate`;
- `--animate` runs only on a suitable interactive stdout;
- redirected/piped output contains one clean static frame;
- no full-screen clear or alternate-screen behavior is used;
- final output remains on screen and the shell resumes normally below it;
- Ctrl+C restores cursor visibility;
- Linux behavior is manually verified and macOS/Windows builds remain green.
