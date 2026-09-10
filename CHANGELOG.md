# Changelog

All notable changes to AstroFetch will be documented in this file.

Changelog tracking starts with v0.5.0.

## [Unreleased]

### Added

- Deterministic `spiral/dust/v1` dust-lane configuration (`DustLaneConfig`) for the Spiral model, derived from an isolated versioned feature stream without advancing the legacy scene RNG.
- Dust-lane extinction for the Spiral model: the dust configuration is now consumed as a deterministic multiplicative attenuation of the luminous disk (disk plus gated arms times clumpiness) using `tau = strength * profile * radial_gate` and `extinction = exp(-tau)`. The bulge, stellar bar, and stellar knots are not attenuated. Dustless scenes keep the exact pre-dust density expression.

### Changed

- Made Spiral sampling geometry shape-aware: `SamplingGeometry` in `src/galaxy.rs` now derives its logical dimensions from `CellSamplingShape` via a new `for_terminal(width, height, shape)` constructor, and the Spiral generator has an internal shape-aware path (`generate_spiral_galaxy_with_shape`). The production path always uses `HALF_BLOCK` (1×2), reproducing the legacy `W×2H` logical / `3W×6H` supersampled dimensions bit-for-bit; `QUADRANT` (2×2, logical `2W×2H` / supersampled `6W×6H`) is available to the generator and to tests but is not yet consumed by any renderer and is not user-selectable. Renderer-preserving: existing outputs remain bit-for-bit identical.
- Introduced the terminal-cell sampling topology abstraction (`CellSamplingShape` and `Quadrant` in `src/render/topology.rs`): the current 1×2 half-block shape, the future 2×2 quadrant shape, row-major TL/TR/BL/BR subcell ordering, and the terminal-cell -> logical-density index mapping. Internal renderer-preserving preparation: production rendering, density generation, and all visual outputs remain bit-for-bit unchanged; the 2×2 topology is defined and tested only.
- Made the Spiral sampling geometry explicit (`SamplingGeometry` in `src/galaxy.rs`): terminal W×H -> logical W×2H (1×2 logical samples per terminal cell) -> 3× supersampling -> average reduction back to W×2H. Renderer-preserving: existing outputs remain bit-for-bit identical.

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

[0.5.0]: https://github.com/elacerda/astrofetch/compare/v0.4.0...v0.5.0
