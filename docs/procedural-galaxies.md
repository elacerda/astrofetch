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

The high-level pipeline is:

```text
seeded RNG
  -> procedural model parameters
  -> 2D density map
  -> normalization / contrast stretch
  -> adaptive thresholding
  -> Unicode/ASCII glyph rendering
  -> optional ANSI color
```

For galaxy-like models, the intermediate representation is a scalar density field. Each cell stores a normalized luminosity-like value, not a physical flux. The renderer then maps this field into terminal glyphs.

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
top density row    -> upper half of the glyph
bottom density row -> lower half of the glyph
```

This gives a useful vertical resolution boost without increasing the number of printed terminal lines.

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
- noise scale;
- a faint-density threshold floor.

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
I(r, theta) = bulge + disk + arms * clumpiness + stellar_knots - threshold_floor
```

Values below zero are clipped.

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

After density generation, galaxy-like models are normalized and stretched for terminal display. The spiral pipeline uses gamma stretching after downsampling. Other models can use normalization and stretch functions to increase contrast in the low dynamic range of terminal glyphs.

This step is visual rather than physical. Its purpose is to make faint structure readable without filling the entire canvas.

## Adaptive thresholding

The half-block renderer computes an adaptive threshold from non-zero density values. It uses a percentile-like value and clamps it to avoid two common failure modes:

- threshold too low: the galaxy becomes a solid blob;
- threshold too high: only the nucleus remains visible.

The threshold controls which density pairs become visible block glyphs and which cells remain background.

## Half-block glyph rendering

Galaxy models are rendered with Unicode shading glyphs:

```text
░  faint structure
▒  low/intermediate structure
▓  bright structure
█  brightest structure
```

The renderer consumes pairs of density rows and converts them to one terminal row. It chooses a shaded block based on the average and maximum density in the pair. This preserves thin structures while still emphasizing diffuse light.

Background stars are added only where local galaxy density is low, so they do not overwrite visible galaxy structure.

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
