# Changelog

All notable changes to AstroFetch will be documented in this file.

Changelog tracking starts with v0.5.0.

## [Unreleased]

## [1.1.0] - 2026-09-17

### Added

- Opt-in `--animate` terminal intro: a short deterministic 6-frame (~600 ms)
  animation on interactive terminals, with star twinkle, bounded Starfield
  micro-motion, and subtle coherent Spiral phase sway. Pipes/redirections fall
  back to static output; Ctrl+C and terminal resize are handled safely, and
  ordinary non-animated output remains unchanged.
- Elliptical galaxy morphology v2: a deterministic Sersic-based elliptical
  model with four procedural families (CompactDisky, Classical, GiantBoxy,
  CdLike), boxy or disky isophotes, per-scene central structure, and a
  faint extended outer-halo presentation.

## [1.0.1] - 2026-09-14

### Fixed

- Fixed repeated `setup-shell` runs unnecessarily rewriting an already
  managed startup block and reporting it as updated.
- Standardized top-level CLI error prefixes to English.

### Documentation

- Corrected post-v1.0 wording around visual baselines, Quadrant topology
  states, renderer compatibility, and pinned-install examples.

## [1.0.0] - 2026-09-14

The first stable release. It introduces the opt-in 2×2 Quadrant renderer
for Spiral galaxies and deterministic dust lanes for the Spiral model,
built on a shape-aware sampling and scene-resolution architecture.

### Added

- Experimental `--renderer quadrant` option for the Spiral model: a 2×2
  quadrant renderer that maps each terminal cell's four logical subcells
  (TL, TR, BL, BR) to one of 16 Unicode quadrant, half-block, and full-block
  glyphs, producing finer, direction-aware contours than the 1×2
  half-block renderer. Quadrant is production-capable — deterministic,
  calibrated, and permanently anchored — but remains opt-in: `auto` never
  selects it.
- Foreground-only color for the Quadrant renderer: one foreground
  intensity per cell is derived from the maximum density among visible
  subcells, and the terminal background color is intentionally never
  used, so glyph geometry stays determined solely by the 2×2 visibility
  mask. Effective no-color output remains ANSI-free.
- Deterministic sparse background stars for the Quadrant renderer: stars
  are rendered only in completely empty cells, using the existing
  hash-based star convention (no new RNG stream), and remain uncolored
  even when galaxy color is enabled.
- Calibrated occupancy for the Quadrant renderer: the 0.26 target
  occupancy — the target fraction of non-empty terminal cells, selected as
  the `(1.0 - 0.26)` quantile over per-cell `max(TL, TR, BL, BR)` — was
  measured against the quadrant topology and deliberately retained
  (realized terminal-cell occupancy approximately 26.1% at 40×20).
- Permanent no-color visual anchors for the Quadrant renderer at 40×20 for
  seeds 4, 16, and 42, covering shape-aware generation, quadrant
  occupancy preparation, glyph geometry, and the deterministic star
  overlay. Colored output is covered by focused unit tests rather than
  ANSI fingerprints.
- Deterministic `spiral/dust/v1` dust-lane configuration for the Spiral
  model, derived from an isolated versioned feature stream: a scene is
  dusty with probability 0.60, with a tau amplitude (`0.25..0.55`), a
  signed arm-phase offset (`-0.30..0.30` rad), and a lane width factor
  (`0.5..1.2`).
- Dust-lane extinction for the Spiral model: a deterministic multiplicative
  attenuation of the luminous disk (disk plus gated arms times
  clumpiness) using `tau = strength * profile * radial_gate` and
  `extinction = exp(-tau)`. Dust lanes follow the stellar-arm geometry as
  a signed phase-offset copy with per-arm Gaussian profiles combined by
  maximum (not sum). The bulge, stellar bar, and stellar knots are not
  attenuated, and dustless scenes keep the exact pre-dust density
  expression.

### Changed

- Default Spiral scenes can now carry dust lanes: about 60% of Spiral
  scenes render attenuated dust lanes along the spiral arms, which
  legitimately changes the default Spiral output for those seeds.
- Explicit `--renderer quadrant` with `elliptical`, `cluster`, or
  `starfield` returns a clear CLI error instead of silently falling back,
  and `--model random --renderer quadrant` is rejected before random model
  resolution, so the request never succeeds or fails by chance of the
  model draw.
- Spiral sampling is now shape-aware: the generator derives its logical
  dimensions from an explicit terminal-cell topology (`CellSamplingShape`).
  All legacy paths use `HALF_BLOCK` (1×2, logical `W×2H`, supersampled
  `3W×6H`); the quadrant path uses `QUADRANT` (2×2, logical `2W×2H`,
  supersampled `6W×6H`). For a fixed seed the RNG stream is identical
  regardless of shape.
- Scene resolution is now split from density generation: the engine
  resolves a request into a concrete model and a concrete seed (the seed
  is concretized exactly once) before generating density at the requested
  sampling shape. The legacy `generate_scene` entry point is preserved and
  produces bit-identical output to the split path for `HALF_BLOCK`.

### Compatibility / Determinism

- Legacy RNG contracts preserved: no legacy RNG draw was added or
  reordered. The `spiral/bar/v1` and `spiral/dust/v1` streams are
  isolated versioned feature streams that never advance the legacy scene
  RNG or each other, and the `random` model's resolution table is frozen.
- Legacy renderer contracts preserved: HalfBlock, Shade, and ASCII
  rendering, the per-subcell visibility rule, and the deterministic
  background-star policy are unchanged. Scenes whose density is unchanged
  render bit-for-bit identically to v0.5.0; the legacy anchors for seeds
  16 and 42 are unchanged. The legacy anchor for seed 4 was intentionally
  re-captured because seed 4 is one of the scenes that now carries dust
  lanes.
- Spiral `auto` intentionally remains HalfBlock: the measured default
  overhead of Quadrant is only about +2–3 ms at 40×20 (performance is not
  the blocker), but switching the default would immediately and visibly
  change the default Spiral output in a tool commonly used on shell
  startup. This is a product and backward-compatibility decision, not a
  renderer-correctness limitation; the default may be reconsidered after
  at least one release of opt-in Quadrant exposure without meaningful
  compatibility reports, or after Quadrant support expands beyond Spiral,
  and would be an explicitly announced decision.

## [0.5.0] - 2026-09-09

### Added

- Deterministic barred-spiral morphology for the Spiral model.
- Optional central stellar bars using a finite elliptical Ferrers profile.
- Seed-derived bar length, axis ratio, strength, and orientation.
- Feature-specific deterministic random streams using versioned namespaces such as `spiral/bar/v1`.
- Visual regression baselines for deterministic Spiral rendering.
- Documentation of the procedural-generation determinism contract.

### Changed

- Barred spiral galaxies now gate spiral arms and stellar knots around the nuclear region.
- Spiral-arm phase is adjusted so the arms emerge naturally near the ends of the stellar bar.
- Optional morphology features no longer perturb the legacy Spiral random-number stream.
- Procedural-galaxy documentation now describes barred morphology and reproducibility guarantees.

[Unreleased]: https://github.com/elacerda/astrofetch/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/elacerda/astrofetch/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/elacerda/astrofetch/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/elacerda/astrofetch/compare/v0.5.0...v1.0.0
[0.5.0]: https://github.com/elacerda/astrofetch/compare/v0.4.0...v0.5.0
