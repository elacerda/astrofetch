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
  -> renderer (HalfBlock or Starfield)
  -> optional ANSI color
```

The `RenderProfile` is an explicit per-model configuration that determines:
- whether normalization is applied;
- which contrast stretch is used;
- how the visibility threshold is determined;
- which renderer family is used.

This separation makes post-processing decisions explicit and testable.

For galaxy-like models, the intermediate representation is a scalar density field. Each cell stores a normalized luminosity-like value, not a physical flux. The selected galaxy renderer then maps this field into terminal glyphs.

## Density preparation

Galaxy models share a common density preparation path:

1. **Density generation**: Procedural models (Spiral, Elliptical, Cluster) generate a 2D density map.
2. **Normalization**: Robust percentile normalization using only finite positive values.
3. **Contrast stretch**: Model-specific gamma stretching (γ=0.65-0.85).
4. **Threshold**: Target-occupancy threshold computed from vertical pair maxima.

This preparation is **renderer-neutral**: the same prepared density is consumed by all three renderers (HalfBlock, Shade, ASCII). The choice of renderer happens after density preparation.

### Starfield bypass

The Starfield model uses its dedicated rendering path. It does not undergo normalization or contrast stretch. Starfield density is rendered directly with its point-like glyphs.

## Renderer selection

After the requested model is resolved to a concrete model, the effective renderer is determined from that model and the requested renderer choice. Galaxy density preparation remains independent of the selected galaxy renderer.

### Resolved model renderer matrix

| Resolved model | Auto      | HalfBlock    | Shade        | ASCII     |
| -------------- | --------- | ------------ | ------------ | --------- |
| Spiral         | HalfBlock | HalfBlock    | Shade        | ASCII     |
| Elliptical     | HalfBlock | HalfBlock    | Shade        | ASCII     |
| Cluster        | HalfBlock | HalfBlock    | Shade        | ASCII     |
| Starfield      | Starfield | incompatible | incompatible | Starfield |

### Compatibility rules

- **Galaxy models** (Spiral, Elliptical, Cluster) accept all renderer choices:
  - `auto` → HalfBlock
  - `half-block` → HalfBlock
  - `shade` → Shade
  - `ascii` → ASCII

- **Starfield model**:
  - `auto` → Starfield (dedicated renderer)
  - `ascii` → Starfield (dedicated renderer)
  - `half-block` → CLI error
  - `shade` → CLI error

### Random model resolution

The `random` model is resolved to a concrete model (Spiral, Elliptical, Cluster, or Starfield) **before** renderer selection. The matrix applies to the resolved model, not the unresolved `random` choice.

### No silent fallback

When an incompatible renderer is selected for Starfield, the CLI returns an explicit error. There is no silent fallback to a compatible renderer.

### Implementation

Renderer resolution is implemented in `App::resolve_effective_renderer` in `src/app.rs`. Invalid combinations return `AppError::Cli` with a descriptive message.

## Density map representation

Internally, the main numerical representation is a row-major 2D scalar field:

```text
DensityMap(width, height, data: Vec<f64>)
```

The values are interpreted as relative brightness or density. They are later normalized to `[0, 1]`, optionally stretched for contrast, and rendered to terminal characters.

The spiral model generates the field at a higher internal sampling resolution and then downsamples by averaging. This reduces aliasing and helps preserve smooth structures in a very small terminal canvas.

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

- number of arms, usually 2 or 3;
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

### Noise and stellar knots

Smooth analytic spirals look too artificial in a terminal. AstroFetch adds OpenSimplex noise at two scales:

- coarse noise modulates arm clumpiness;
- fine noise creates rare bright knots along the arms.

This produces a more organic appearance reminiscent of star-forming regions, without simulating gas, dust, or stellar populations.

The final spiral density is approximately:

```text
I(r, theta) = bulge + disk + arms * clumpiness + stellar_knots
```

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

All three galaxy renderers consume the same prepared density and reuse the same deterministic background-star policy, but they sample each vertical pair differently:

- **HalfBlock** evaluates the top and bottom samples independently and renders `▀`, `▄`, `█`, or a space.
- **Shade** collapses the pair with `max(top, bottom)` and maps the threshold-relative intensity to `░`, `▒`, `▓`, or `█`.
- **ASCII** also collapses the pair with `max(top, bottom)` and maps the threshold-relative intensity to its ASCII palette.
- **Background stars** are considered only when no visible galaxy glyph occupies the terminal cell and the shared local-density guard permits them.

## ANSI color

When color is enabled and supported by the terminal, AstroFetch maps brightness to ANSI color sequences. Color is disabled when `--no-color` is used, when `NO_COLOR` is set, or when stdout is not a suitable TTY.

The design avoids making the background too colorful because excessive ANSI output can make terminal art noisy or less portable.

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
- dust attenuation;
- radiative transfer;
- cosmological environment;
- observational PSF or detector response.

The renderer is best understood as a compact procedural visualization inspired by astrophysical morphology.

## Future improvements

Possible future directions include:

- barred spiral models;
- dust lanes;
- ring galaxies;
- improved inclination handling;
- color maps tuned for color-blind accessibility;
- terminal-size-aware model selection;
- benchmarked startup performance;
- snapshot-based visual regression tests.
