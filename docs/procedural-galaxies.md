# Procedural galaxies in AstroFetch

This document explains how AstroFetch creates its procedural astrophysical terminal art.

AstroFetch is **not** an N-body simulation, a hydrodynamics code, or a radiative-transfer pipeline. It is a compact terminal renderer inspired by real galactic morphology. The goal is to create visually plausible galaxies, clusters, and star fields under severe terminal constraints: low spatial resolution, monospaced glyphs, ANSI colors, and fast startup time.

## Design goals

The renderer is designed to be:

- fast enough to run when opening a shell;
- deterministic when `--seed` is provided;
- portable across Linux, macOS, and Windows terminals;
- visually readable at roughly 40 by 20 terminal cells;
- simple enough to maintain as a small Rust CLI project.

The visual model therefore favors robust analytic approximations and terminal-friendly rendering over physical completeness.

## Rendering pipeline

The high-level pipeline is unified across all models:

```text
seeded RNG
  -> procedural model parameters
  -> 2D density map (DensityMap)
  -> model-specific RenderProfile
  -> normalization (exactly once)
  -> contrast stretch (exactly once)
  -> threshold / render policy
  -> renderer (HalfBlock, Shade, ASCII, Starfield, or experimental Quadrant)
  -> palette selection
  -> optional ANSI color
```

The `RenderProfile` is an explicit per-model configuration that determines:
- whether normalization is applied;
- which contrast stretch is used;
- how the visibility threshold is determined;
- which preparation family is used (galaxy or raw starfield).

This separation makes post-processing decisions explicit and testable.

For galaxy-like models, the intermediate representation is a scalar density field. Each cell stores a normalized luminosity-like value, not a physical flux. The selected galaxy renderer then maps this field into terminal glyphs.

## Density preparation

Galaxy models share a common density preparation path:

1. **Density generation**: Procedural models (Spiral, Elliptical, Cluster) generate a 2D density map.
2. **Normalization**: Robust percentile normalization using only finite positive values.
3. **Contrast stretch**: Model-specific gamma stretching (γ=0.65-0.85).
4. **Threshold**: Target-occupancy threshold computed from vertical pair maxima.

This preparation is **renderer-neutral**: the same prepared density is consumed by all three renderers (HalfBlock, Shade, ASCII). The choice of renderer happens after density preparation.

### Starfield preparation

With the dedicated Starfield renderer (selected by `auto`), the Starfield model uses its own rendering path. It does not undergo normalization or contrast stretch. Starfield density is rendered directly with its point-like glyphs.

When Starfield is rendered through HalfBlock, Shade, or ASCII, its sparse density is prepared like a galaxy model: robust percentile normalization (2nd–98th), gamma stretch (γ=0.85), and a target-occupancy threshold (0.10).

## Renderer selection

After the requested model is resolved to a concrete model, the effective renderer is determined from that model and the requested renderer choice. Galaxy density preparation remains independent of the selected galaxy renderer.

### Resolved model renderer matrix

| Resolved model | Auto      | HalfBlock    | Shade        | ASCII     | Quadrant (experimental) |
| -------------- | --------- | ------------ | ------------ | --------- | ----------------------- |
| Spiral         | HalfBlock | HalfBlock    | Shade        | ASCII     | Quadrant                |
| Elliptical     | HalfBlock | HalfBlock    | Shade        | ASCII     | error (unsupported)     |
| Cluster        | HalfBlock | HalfBlock    | Shade        | ASCII     | error (unsupported)     |
| Starfield      | Starfield | HalfBlock    | Shade        | ASCII     | error (unsupported)     |

### Compatibility rules

Every explicit renderer choice works with every concrete model, with the
exception of the experimental Quadrant renderer. `auto` keeps the
model-specific default:

- **Galaxy models** (Spiral, Elliptical, Cluster):
  - `auto` → HalfBlock
  - `half-block` → HalfBlock
  - `shade` → Shade
  - `ascii` → ASCII

- **Starfield model**:
  - `auto` → Starfield (dedicated renderer)
  - `half-block` → HalfBlock
  - `shade` → Shade
  - `ascii` → ASCII

- **Experimental Quadrant renderer** (`--renderer quadrant`):
  - `--model spiral` → Quadrant (color supported: foreground-only ANSI)
  - `--model elliptical` / `--model cluster` / `--model starfield` → clear
    CLI error (Quadrant currently supports Spiral only)
  - `--model random` → clear CLI error, rejected **before** random model
    resolution so the request never succeeds or fails by chance
  - `auto` never selects Quadrant

### Random model resolution

The `random` model is resolved to a concrete model (Spiral, Elliptical, Cluster, or Starfield) **before** renderer selection. The matrix applies to the resolved model, not the unresolved `random` choice.

### Implementation

Renderer resolution is implemented in `App::resolve_effective_renderer` in
`src/app.rs`. Density preparation is selected by
`RenderProfile::for_model_and_renderer` in `src/render/profile.rs`. An
unresolved `random` model reaching renderer selection is an internal error.

## Density map representation

Internally, the main numerical representation is a row-major 2D scalar field:

```text
DensityMap(width, height, data: Vec<f64>)
```

The values are interpreted as relative brightness or density. They are later normalized to `[0, 1]`, optionally stretched for contrast, and rendered to terminal characters.

The spiral model generates the field at a higher internal sampling resolution and then downsamples by averaging. This reduces aliasing and helps preserve smooth structures in a very small terminal canvas.

### Spiral sampling geometry

The Spiral model evaluates the density field on a fixed, non-configurable
sampling geometry (`SamplingGeometry` in `src/galaxy.rs`). Phase 3 made the
geometry explicit without changing it; Phase 5A made it shape-aware: the
logical dimensions now derive from the cell sampling shape
(`CellSamplingShape`), and the production path always uses the `HALF_BLOCK`
shape, so all existing outputs remain bit-for-bit identical:

```text
terminal W×H
  -> logical max(W,1) × max(H,1)×2        (HALF_BLOCK, production)
  -> supersampled logical_width×3 × logical_height×3
  -> average reduction back to the logical W×2H field
```

- The terminal requests `W × H` cells.
- The production `HALF_BLOCK` shape (1×2) gives the legacy logical field
  `W × 2H`: 1 logical sample per terminal cell horizontally and 2
  vertically (the renderer consumes two density rows per visible terminal
  row via half-block glyphs), and the supersampled field `3W × 6H`.
- The `QUADRANT` shape (2×2) gives the quadrant dimensions: logical
  `2W × 2H` and supersampled `6W × 6H` (approximately double the sampling
  evaluations of `HALF_BLOCK`). It is consumed by the experimental
  `--renderer quadrant` path for the Spiral model (color supported:
  foreground-only ANSI); every other production path uses `HALF_BLOCK`.
- The density is evaluated on a `3×` supersampled grid (3 high-resolution
  samples per logical cell on each axis).
- The supersampled field is reduced back to the logical size by averaging
  3×3 blocks.

### Terminal-cell topology

The mapping from terminal cells to logical density samples is an explicit
topology abstraction (`CellSamplingShape` in `src/render/topology.rs`):

- **Current production topology**: 1×2 — one logical sample horizontally
  and two vertically per terminal cell (the half-block contract).
- **Quadrant topology**: 2×2 — four logical samples per terminal cell.
  Consumed by the experimental `--renderer quadrant` path for the Spiral
  model (color supported: foreground-only ANSI).
- Subcell ordering is row-major: top-left, top-right, bottom-left,
  bottom-right (TL, TR, BL, BR), with offsets (0,0), (1,0), (0,1), (1,1).
- A subcell `(sx, sy)` of terminal cell `(cx, cy)` maps to the logical
  sample `(cx × columns + sx, cy × rows + sy)`.

Phase 5A made Spiral generation shape-aware: the generator derives its
logical dimensions from the shape. The default production path uses
`HALF_BLOCK`, so production rendering and all existing outputs remain
bit-for-bit unchanged. `QUADRANT` is consumed only by the experimental
`--renderer quadrant` path for the Spiral model (color supported: foreground-only ANSI).

## Terminal constraints

A terminal cell is not a square pixel. AstroFetch compensates partly by generating galaxy density maps at twice the requested terminal height. The renderer then collapses two vertical density samples into one visible terminal row using Unicode block characters.

For galaxy models, one terminal row represents two internal density rows:

```text
top density row    -> upper half of the glyph (top visible)
bottom density row -> lower half of the glyph (bottom visible)
```

This gives a useful vertical resolution boost without increasing the number of printed terminal lines.

## Art dimensions

Art dimensions are determined by the planner based on terminal capabilities and explicit overrides:

### Automatic dimensions

When `--width` and `--height` are omitted, dimensions adapt to the terminal:

- **Preferred automatic art size**: 40 × 20 cells
- **Available side-by-side art width**: terminal width - layout gap (2) - measured information width
- **Side-by-side selection**: used when preferred art fits within available width
- **Width shrinking**: automatic art width may shrink to at least the side-by-side minimum (20 columns)
- **Stacked fallback**: layout becomes stacked when available art width is below the side-by-side minimum

### Explicit overrides

When `--width` and/or `--height` are specified:

- Width must be between 1 and 200
- Height must be between 1 and 100
- Explicit dimensions are never shrunk by the planner
- Missing explicit dimension is derived from the other (width:height ≈ 2:1)

### Fallback behavior

When no terminal dimensions are available (non-TTY):

- Default art dimensions are 40×20
- Layout defaults to side-by-side

### Layout selection

The planner automatically chooses between side-by-side and stacked:

- **Side-by-side**: preferred when art fits alongside info
- **Stacked**: used when art doesn't fit alongside info, or when explicitly requested

### Stacked vertical space

For stacked layout, vertical space is calculated as:

- **Reserved space**: information lines + 1 separator line (when information is non-empty)
- **Available art height**: terminal height - reserved space
- **Automatic height**: derived from final width (2:1 ratio), capped at available height
- **Minimum height**: 1 line even when space is insufficient

### Information never truncated

Information lines are never truncated regardless of terminal size. The planner preserves all information lines without truncation.

### Explicit layout always honored

When `--layout` is explicitly set, the planner respects the choice regardless of available space.

The half-block renderer uses independent visibility for each half:
- **Top only visible**: `▀` (upper half block)
- **Bottom only visible**: `▄` (lower half block)
- **Both visible**: `█` (full block)
- **Neither visible**: ` ` (space)

In color mode, the renderer uses foreground color for the top half and background color for the bottom half with the `▀` glyph to preserve both samples independently.

## Spiral galaxy model

The spiral renderer builds a face-on analytic galaxy and then applies projection-like transformations.

### Parameters

For each generated spiral, seeded random parameters define:

- number of arms, from 2 to 5;
- spiral pitch;
- inclination;
- sky-plane rotation;
- bulge width;
- exponential disk scale;
- arm width;
- arm strength;
- noise scale.

These parameters vary the morphology while keeping the result deterministic for a fixed seed.

### Coordinates and projection

Each point is mapped into normalized coordinates centered on the canvas:

```text
x, y in approximately [-1, 1]
```

The coordinates are rotated in the sky plane and then deprojected with a simple inclination term:

```text
x_r = x cos(phi) + y sin(phi)
y_r = -x sin(phi) + y cos(phi)
y_d = y_r / cos(i)
r   = sqrt(x_r^2 + y_d^2)
theta = atan2(y_d, x_r)
```

This is a visual approximation of an inclined disk, not a full 3D radiative model.

### Bulge and disk

The central bulge is modeled as a Gaussian radial component:

```text
B(r) = A_b exp[-0.5 (r / sigma_b)^2]
```

The disk is modeled as a faint exponential component:

```text
D(r) = A_d exp(-r / R_d)
```

The bulge gives the galaxy a compact center, while the disk supplies low-level diffuse structure.

### Spiral arms

The arms are based on a logarithmic spiral:

```text
r = a exp(b theta)
```

For a given radius, the model estimates the corresponding arm angle and measures the angular distance to the nearest arm. That angular distance is converted into an approximate transverse distance:

```text
distance = r * |Delta theta|
```

Each arm contributes a Gaussian profile around its ridge:

```text
A_arm ~ exp[-0.5 (distance / width)^2]
```

The arm contribution fades with radius and is scaled by the configured arm strength.

### Bar

A subset of spiral scenes contains a central stellar bar. Bar presence and parameters (half-length, axis ratio, strength, angle) are derived deterministically from the isolated `spiral/bar/v1` feature stream, so a fixed seed always produces the same bar or no bar, without advancing the legacy scene RNG.

The bar is a finite elliptical Ferrers profile in the intrinsic disk plane:

```text
m2 = (x_b / a)^2 + (y_b / b)^2
B_bar = strength * (1 - m2)^2   for m2 < 1
B_bar = 0                        otherwise
```

where `a` is the bar half-length, `b` the bar half-width, and `(x_b, y_b)` are the intrinsic disk coordinates rotated into the bar frame.

When a bar is present, the spiral arms are radially gated between 0.65 and 1.05 times the bar half-length, and the logarithmic spiral phase is shifted so arm 0 reaches the bar orientation near the bar end. Stellar knots follow the same gate, so no spiral-associated structure appears inside the gated nuclear region.

At current terminal resolution a valid intrinsic bar can be visually subtle: central structure, projection, normalization, and sampling can all reduce its contrast. Bar visibility is intentionally not tuned by distorting the galaxy model; future visibility improvements belong to later sampling and rendering work.

### Dust lanes

A subset of spiral scenes carries an optional `DustLaneConfig`. Presence and parameters are derived deterministically from the isolated `spiral/dust/v1` feature stream, so a fixed seed always produces the same dust configuration or no dust, without advancing the legacy scene RNG or the `spiral/bar/v1` stream.

- **Presence**: a scene is dusty with probability 0.60; otherwise it is genuinely dustless.
- **`strength`**: a dimensionless optical-depth (tau) amplitude in `0.25..0.55`. It is *not* a fractional attenuation depth.
- **`offset`**: a signed arm-phase offset in radians, in `-0.30..0.30`. The model has no explicit chirality or rotation-direction semantics, so the offset is not a leading/trailing statement.
- **`width_factor`**: a factor in `0.5..1.2` scaling the local spiral-arm width for the dust lanes.

These are procedural calibration ranges chosen for visual plausibility, not empirically validated astrophysical distributions.

Dust is rendered as a deterministic multiplicative extinction of the luminous disk:

- The dust lanes are a **signed phase-offset copy of the stellar-arm geometry**: each dust ridge sits at the stellar arm angle plus `offset`, evaluated in the same intrinsic/deprojected disk frame as the arms (including the bar phase alignment when a bar is present).
- Each arm contributes a Gaussian profile `exp[-0.5 (distance / width)^2]` with `distance = r * |Delta theta|` and `width = arm_width * (1 + 0.75 r) * width_factor`; the arms are combined with a **MAX (not a SUM)** so overlapping lanes do not stack. The profile is finite, non-negative, and at most 1.
- The optical depth is `tau = strength * profile * radial_gate`, and the extinction factor is `exp(-tau)`.

The radial gate controls where dust appears:

- **Barred scenes**: the same transition as the spiral-arm gate, a cubic smoothstep between 0.65 and 1.05 times the bar half-length.
- **Unbarred scenes**: a cubic smoothstep between `bulge_sigma` and `2 * bulge_sigma` (a fixed first-version morphology relation, not an RNG parameter and not a calibrated astrophysical law).

Composition: dust attenuates the disk and the gated arms times clumpiness (the "luminous disk"). Dust does **not** attenuate the bulge, the stellar bar, or the stellar knots. This is a deliberate first-version composition choice for procedural terminal art, not physical radiative-transfer behavior.

For dustless scenes the density equation is exactly the pre-dust expression; the dustless branch is kept explicit (rather than using a mathematically equivalent `extinction = 1` formulation) to preserve the floating-point evaluation order and bit-identical output.

### Noise and stellar knots

Smooth analytic spirals look too artificial in a terminal. AstroFetch adds OpenSimplex noise at two scales:

- coarse noise modulates arm clumpiness;
- fine noise creates rare bright knots along the arms.

This produces a more organic appearance reminiscent of star-forming regions, without simulating gas, dust, or stellar populations.

The final spiral density is approximately:

```text
I(r, theta) = bulge + bar + (disk + gate * arms * clumpiness) * extinction + gate * stellar_knots
```

where `extinction` is 1 for dustless scenes or `exp(-tau)` for dusty scenes, `bar` is zero for unbarred scenes, and `gate` is 1 for unbarred scenes or the radial arm gate described above for barred scenes.

The model preserves its native positive density. Mathematically invalid negative values (from noise subtraction) are clamped to zero. Visibility sparsification is handled by the target-occupancy threshold in the post-processing pipeline, not by generation-time cutoffs.

## Elliptical galaxy model

The elliptical model uses a smooth projected radial profile. It applies:

- a random ellipticity;
- a random sky-plane rotation;
- a broad Gaussian-like component;
- a compact central core;
- a faint-outskirts cutoff;
- very light noise only where the galaxy is visible.

This creates a diffuse, centrally concentrated object with smoother morphology than the spiral model.

## Cluster model

The cluster model places a sparse set of bright points around a center using a radial distribution. It also adds a faint central nebulous component so the output does not look like purely random noise.

This model is meant to evoke a stellar cluster rather than a detailed dynamical simulation.

## Starfield model

The starfield model is intentionally sparse. It uses point-like glyphs rather than diffuse block shading:

```text
.  faint star
*  medium star
+  bright star
```

The starfield renderer has its own glyph and color mapping so that sparse stars do not get converted into large diffuse blocks.

## Normalization and contrast stretching

After density generation, galaxy-like models are normalized and stretched according to their `RenderProfile`. The profile determines:

- **Normalization**: Robust percentile normalization using only finite positive values. Starfield uses no normalization.
- **Contrast stretch**: Model-specific gamma stretching (γ=0.65-0.85).
- **Threshold**: Target occupancy percentiles or dedicated renderer behavior.

This step is visual rather than physical. Its purpose is to make faint structure readable without filling the entire canvas.

### Per-model normalization strategy

| Model      | Normalization | Stretch    | Target Occupancy |
|------------|---------------|------------|------------------|
| Spiral     | Robust        | Gamma 0.85 | 26%              |
| Elliptical | Robust        | Gamma 0.70 | 23%              |
| Cluster    | Robust        | Gamma 0.65 | 10%              |
| Starfield  | None          | None       | Dedicated        |

The occupancy targets are measured before background star injection and may be adjusted based on visual inspection.

### Robust percentile normalization

Robust normalization uses only finite positive values for percentile estimation:

1. Collect all finite positive values from the density map.
2. Sort them deterministically using `f64::total_cmp`.
3. Estimate low and high percentiles from the sorted values.
4. Map values to `[0, 1]` using the estimated range.
5. Non-finite, negative, and zero values map to 0.0.
6. Empty or all-zero maps remain zero.
7. Collapsed positive ranges (all equal values) map positive cells to 1.0.

### Target-occupancy threshold

The threshold is computed from vertical pair maxima:

1. Iterate over density rows in vertical pairs.
2. For each x coordinate, calculate `pair_value = top.max(bottom)`.
3. Sanitize non-finite or negative values to zero.
4. Include every terminal cell, including zero cells.
5. Sort deterministically with `f64::total_cmp`.
6. Choose the threshold at quantile `(1.0 - target)` using the index `floor((n-1) * quantile)` where `n` is the number of pairs.

**Note**: Occupancy is defined per terminal cell, where a cell is visible if **either** half is visible. This remains equivalent to the pair maximum meeting the threshold.

## Half-block glyph rendering

Galaxy models can be rendered with three families of characters:

### Half-block renderer (default for galaxy models)

Uses Unicode half-block characters:

```text
▀  top half visible only
▄  bottom half visible only
█  both halves visible
   (space)  neither half visible
```

### Shade renderer

Uses Unicode density characters:

```text
░  lowest intensity
▒  low intensity
▓  high intensity
█  highest intensity
```

### ASCII renderer

Uses ASCII characters ordered by intensity:

```text
. : - = + * # % @
```

### Quadrant renderer (experimental)

The experimental `--renderer quadrant` option renders a Spiral galaxy with
the 2×2 quadrant topology: each terminal cell owns four logical subcells
(TL, TR, BL, BR) and one of 16 Unicode quadrant/half/full-block glyphs is
selected by the 4-bit visibility mask of those subcells. The
application-level path also overlays deterministic sparse background stars
on completely empty cells (mask == 0).

Current scope (by design, for this phase):

- **Spiral only**: explicit `--renderer quadrant` with any other model is a
  clear CLI error; there is no silent fallback.
- **Foreground-only color**: color is supported, but only as a
  foreground-only ANSI channel. Glyph geometry remains determined by the
  2×2 visibility mask; one foreground intensity per cell is derived from
  the maximum visible subcell density (the Phase-6A max-visible rule), and
  the terminal background color is intentionally not used. Effective
  no-color rendering remains ANSI-free (glyphs plus uncolored stars).
- **Background stars**: deterministic sparse background stars are rendered
  only in completely empty cells, using the existing deterministic
  hash-based galaxy-star convention (no new RNG stream); stars remain
  uncolored even when galaxy color is enabled.
- **Star-free reference primitives**: the pure `render_quadrant` and
  `render_quadrant_colored` primitives intentionally remain star-free
  reference and compatibility functions (frozen Phase-5B/6A behavior,
  exercised by tests only).
- **Visually accepted, not yet anchored**: the output has been visually
  accepted, but permanent Quadrant visual anchors have not yet been
  established and the 0.26 occupancy has not yet been separately
  recalibrated.

### Galaxy renderer sampling

All three galaxy renderers consume the same prepared density and reuse the same deterministic background-star policy, but they sample each vertical pair differently:

- **HalfBlock** evaluates the top and bottom samples independently and renders `▀`, `▄`, `█`, or a space.
- **Shade** collapses the pair with `max(top, bottom)` and maps the threshold-relative intensity to `░`, `▒`, `▓`, or `█`.
- **ASCII** also collapses the pair with `max(top, bottom)` and maps the threshold-relative intensity to its ASCII palette.
- **Background stars** are considered only when no visible galaxy glyph occupies the terminal cell and the shared local-density guard permits them. The experimental Quadrant renderer reuses the same policy for its completely empty cells.

## ANSI color

When color is enabled and supported by the terminal, AstroFetch maps brightness to ANSI color sequences. Color is disabled when `--no-color` is used, when `NO_COLOR` is set, or when stdout is not a suitable TTY.

The design avoids making the background too colorful because excessive ANSI output can make terminal art noisy or less portable.

### Palette selection

After the renderer is selected, the color palette is resolved. The palette controls the ANSI xterm-256 colors used for each intensity level while preserving the same intensity thresholds:

- **galaxy models**: all effective palettes use the same 7 intensity bands (0.16, 0.30, 0.44, 0.58, 0.72, 0.88), but with different xterm-256 color indices.
- **starfield model**: all palettes share the same faint, medium, and bright thresholds; the palettes differ only in the ANSI colors selected within those categories.

The shared Starfield thresholds are:

```text
faint:  value < 0.085
medium: 0.085 <= value < 0.150
bright: value >= 0.150
```

Available palettes:
- `nebula`: The original AstroFetch color scheme (byte-compatible ANSI sequences).
- `cividis`: A color-vision-friendly xterm-256 approximation inspired by the Cividis colormap.
- `amber`: A warm monochromatic-style ramp using orange-yellow gradients.
- `mono`: An xterm-256 grayscale ramp for accessibility and portability.
- `auto`: Resolves to `nebula` at runtime (CLI-only choice).

The palette is selected after density preparation and renderer selection. It affects procedural art only; system information headers, labels, and values use separate, hardcoded ANSI sequences.

## Reproducibility

The `--seed` option makes the procedural output deterministic:

```bash
astrofetch --model spiral --seed 42
```

This is useful for screenshots, tests, visual comparisons, and documentation.

Without a seed, AstroFetch draws from randomness so each run can produce a different object.

## Scientific limitations

AstroFetch output should not be interpreted as scientific data. In particular, it does not model:

- gravitational dynamics;
- stellar population synthesis;
- gas hydrodynamics;
- physical dust attenuation (the rendered dust lanes are a bounded procedural extinction approximation, not a physical model);
- radiative transfer;
- cosmological environment;
- observational PSF or detector response.

The renderer is best understood as a compact procedural visualization inspired by astrophysical morphology.

## Future improvements

Possible future directions include:

- ring galaxies;
- improved inclination handling;
- color maps tuned for color-blind accessibility;
- terminal-size-aware model selection;
- benchmarked startup performance;
- snapshot-based visual regression tests.
