# Changelog

All notable changes to AstroFetch will be documented in this file.

Changelog tracking starts with v0.5.0.

## [Unreleased]

### Added

- Deterministic `spiral/dust/v1` dust-lane configuration (`DustLaneConfig`) for the Spiral model.
- Configuration is derived from an isolated versioned feature stream; it is configuration-only and produces **no rendering change** (dust attenuation is not yet integrated into the density model).

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
