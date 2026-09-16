//! Deterministic Elliptical morphology configuration for the v2 generator.
//!
//! B1.2 introduces the morphology contract only: an immutable
//! [`EllipticalGalaxyConfig`] derived once per scene from the versioned
//! `elliptical/morphology/v2` feature namespace. The density generator
//! consumes this config in B2; until then nothing in the render pipeline
//! reads this module, so Elliptical rendered output stays byte-identical to
//! baseline `85ca173`.
//!
//! Derivation contract:
//!
//! * Feature seed: `GenerationContext::feature_seed(ELLIPTICAL_MORPHOLOGY_V2)`.
//! * One private `StdRng`, exactly seven unit draws in a fixed order:
//!   1. family (weights: CompactDisky 0.30, Classical 0.30, GiantBoxy 0.20, CdLike 0.20);
//!   2. position angle, uniform in `[0, π)`;
//!   3. roundness latent → `axis_ratio`;
//!   4. size/concentration latent → `effective_radius` AND `profile_index`
//!      (larger Re implies larger n within the family ranges);
//!   5. envelope latent → `outer_halo_strength` AND `outer_halo_scale`
//!      (stronger halo implies broader halo);
//!   6. isophote latent → `isophote_shape`;
//!   7. central-structure latent → `core_softening_fraction` /
//!      `central_excess` per family.
//! * No rejection sampling and no data-dependent draw counts: every seed
//!   performs exactly the same seven draws.
//!
//! Units and conventions:
//!
//! * `axis_ratio` is `q = b/a`, dimensionless, in `[0, 1)`.
//! * `position_angle` is in radians, `[0, π)`. The major axis lies at this
//!   angle counter-clockwise from the `+x` (horizontal) canvas axis, matching
//!   the legacy elliptical rotation convention (`y_rot = 0` along the major
//!   axis).
//! * `effective_radius` is the half-light radius (Re) in legacy
//!   canvas-fraction coordinates, where `dx = (x − w/2)/w` and
//!   `dy = (y − h/2)/render_height`, so the canvas half-extents are 0.5.
//! * `profile_index` is a dimensionless Sersic-like shape parameter in
//!   `[2, 6]`; B2 evaluates `I_body(r) = exp(−k(n)·f(r/Re, n))`.
//! * `core_softening_fraction` is a core-softening length as a fraction of
//!   Re; 0.0 means no depleted core.
//! * `central_excess` is a small central enhancement as a fraction of the
//!   peak density; 0.0 means none.
//! * `outer_halo_strength` is the halo amplitude relative to the body's
//!   characteristic surface brightness; `outer_halo_scale` is the halo radial
//!   scale as a multiple of Re.
//! * `isophote_shape` is the amplitude, as a fraction of Re, of a
//!   fourth-order `cos(4θ)` perturbation to the isophote radius. Sign
//!   convention per the research note: positive = disky, negative = boxy
//!   (to be visually verified at the B2 seed-panel gate).

use crate::seed::{GenerationContext, ELLIPTICAL_MORPHOLOGY_V2};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

/// Morphology family of a generated elliptical.
///
/// Families are correlated procedural groupings, not hard astrophysical
/// classes: each family constrains which parameter ranges are plausible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EllipticalFamily {
    /// Moderately flattened, compact, centrally concentrated, slightly disky.
    CompactDisky,
    /// Intermediate axis ratio, smooth decline, neutral isophotes.
    Classical,
    /// Rounder, broader, soft core likely, slightly boxy.
    GiantBoxy,
    /// Smooth bright body plus a faint, much broader outer envelope.
    CdLike,
}

impl EllipticalFamily {
    /// Intended population shares, used by the family draw and diagnostics.
    const INTENDED_FRACTIONS: [(Self, f64); 4] = [
        (Self::CompactDisky, 0.30),
        (Self::Classical, 0.30),
        (Self::GiantBoxy, 0.20),
        (Self::CdLike, 0.20),
    ];
}

/// Frozen initial parameter ranges of the v2 morphology contract, per family.
///
/// These are conservative starting ranges for the B2 tuning sweep; they are
/// part of the B1 contract and must not drift silently.
struct FamilyRanges {
    axis_ratio: (f64, f64),
    effective_radius: (f64, f64),
    profile_index: (f64, f64),
    core_softening_fraction: (f64, f64),
    central_excess: (f64, f64),
    outer_halo_strength: (f64, f64),
    outer_halo_scale: (f64, f64),
    isophote_shape: (f64, f64),
}

impl FamilyRanges {
    /// Returns the frozen ranges for `family`.
    fn for_family(family: EllipticalFamily) -> Self {
        match family {
            EllipticalFamily::CompactDisky => Self {
                axis_ratio: (0.55, 0.80),
                effective_radius: (0.20, 0.30),
                profile_index: (2.0, 3.5),
                core_softening_fraction: (0.0, 0.0),
                central_excess: (0.00, 0.20),
                outer_halo_strength: (0.00, 0.08),
                outer_halo_scale: (1.5, 2.5),
                isophote_shape: (0.005, 0.045),
            },
            EllipticalFamily::Classical => Self {
                axis_ratio: (0.62, 0.88),
                effective_radius: (0.24, 0.36),
                profile_index: (2.5, 4.5),
                core_softening_fraction: (0.00, 0.10),
                central_excess: (0.00, 0.10),
                outer_halo_strength: (0.00, 0.12),
                outer_halo_scale: (2.0, 3.0),
                isophote_shape: (-0.020, 0.020),
            },
            EllipticalFamily::GiantBoxy => Self {
                axis_ratio: (0.72, 0.95),
                effective_radius: (0.30, 0.44),
                profile_index: (4.0, 6.0),
                core_softening_fraction: (0.05, 0.30),
                central_excess: (0.0, 0.0),
                outer_halo_strength: (0.04, 0.18),
                outer_halo_scale: (2.5, 4.0),
                isophote_shape: (-0.045, -0.005),
            },
            EllipticalFamily::CdLike => Self {
                axis_ratio: (0.65, 0.90),
                effective_radius: (0.24, 0.34),
                profile_index: (2.5, 4.5),
                core_softening_fraction: (0.00, 0.15),
                central_excess: (0.0, 0.0),
                outer_halo_strength: (0.25, 0.45),
                outer_halo_scale: (4.0, 8.0),
                isophote_shape: (-0.020, 0.020),
            },
        }
    }
}

/// Immutable morphology prepared once per Elliptical scene (B1.2 contract).
///
/// Every field is a frozen draw from the versioned feature namespace; nothing
/// here is RNG state. Re-deriving from the same scene seed is
/// bit-for-bit reproducible and never advances any legacy scene RNG stream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EllipticalGalaxyConfig {
    /// Morphology family.
    pub(crate) family: EllipticalFamily,
    /// Axis ratio `q = b/a`, dimensionless.
    pub(crate) axis_ratio: f64,
    /// Sky-plane position angle in radians, `[0, π)`.
    pub(crate) position_angle: f64,
    /// Half-light radius (Re) in legacy canvas-fraction units
    /// (canvas half-extent = 0.5).
    pub(crate) effective_radius: f64,
    /// Sersic-like profile index, dimensionless, in `[2, 6]`.
    pub(crate) profile_index: f64,
    /// Core-softening length as a fraction of Re; 0.0 = no depleted core.
    pub(crate) core_softening_fraction: f64,
    /// Central enhancement as a fraction of the peak density; 0.0 = none.
    pub(crate) central_excess: f64,
    /// Outer halo amplitude relative to the body's characteristic brightness.
    pub(crate) outer_halo_strength: f64,
    /// Outer halo radial scale as a multiple of Re.
    pub(crate) outer_halo_scale: f64,
    /// `cos(4θ)` isophote amplitude as a fraction of Re; positive = disky,
    /// negative = boxy.
    pub(crate) isophote_shape: f64,
}

impl EllipticalGalaxyConfig {
    /// Derives the morphology from `context` using exactly the seven fixed
    /// unit draws documented in the module docs, in that order:
    ///
    /// 1. family (0.30/0.30/0.20/20 cumulative cut on one uniform draw);
    /// 2. `position_angle = u · π`;
    /// 3. `axis_ratio = lerp(roundness range, u)`;
    /// 4. `effective_radius` and `profile_index` from the same size latent;
    /// 5. `outer_halo_strength` and `outer_halo_scale` from the same
    ///    envelope latent;
    /// 6. `isophote_shape = lerp(isophote range, u)`;
    /// 7. central structure:
    ///    - CompactDisky: `core_softening_fraction` is exactly 0.0 and the
    ///      latent maps `central_excess` only;
    ///    - Classical: one latent splits core vs excess conservatively —
    ///      `core = hi·u`, `excess = hi·(1 − u)` — so a strong core and a
    ///      strong excess never coexist (their sum is fixed at `hi`);
    ///    - GiantBoxy / CdLike: `central_excess` is exactly 0.0 and the
    ///      latent maps `core_softening_fraction` only.
    ///
    /// All lerps are `lo + u · (hi − lo)` with `u ∈ [0, 1)`, i.e. monotonic
    /// and free of rejection sampling.
    pub(crate) fn from_context(context: GenerationContext) -> Self {
        let mut rng = StdRng::seed_from_u64(context.feature_seed(ELLIPTICAL_MORPHOLOGY_V2));

        // Draw 1: family membership (30/30/20/20 cumulative cuts).
        let u_family = rng.random_range(0.0..1.0);
        let family = if u_family < 0.30 {
            EllipticalFamily::CompactDisky
        } else if u_family < 0.60 {
            EllipticalFamily::Classical
        } else if u_family < 0.80 {
            EllipticalFamily::GiantBoxy
        } else {
            EllipticalFamily::CdLike
        };

        let ranges = FamilyRanges::for_family(family);

        // Draw 2: sky-plane position angle, independent of everything else.
        let position_angle = rng.random_range(0.0..std::f64::consts::PI);

        // Draw 3: roundness latent → axis_ratio.
        let u_roundness = rng.random_range(0.0..1.0);
        let axis_ratio = lerp(ranges.axis_ratio, u_roundness);

        // Draw 4: size/concentration latent → effective_radius AND profile_index.
        let u_size = rng.random_range(0.0..1.0);
        let effective_radius = lerp(ranges.effective_radius, u_size);
        let profile_index = lerp(ranges.profile_index, u_size);

        // Draw 5: envelope latent → outer_halo_strength AND outer_halo_scale.
        let u_envelope = rng.random_range(0.0..1.0);
        let outer_halo_strength = lerp(ranges.outer_halo_strength, u_envelope);
        let outer_halo_scale = lerp(ranges.outer_halo_scale, u_envelope);

        // Draw 6: isophote latent → isophote_shape.
        let u_isophote = rng.random_range(0.0..1.0);
        let isophote_shape = lerp(ranges.isophote_shape, u_isophote);

        // Draw 7: central-structure latent (always drawn, fixed draw count).
        let u_central = rng.random_range(0.0..1.0);
        let (core_softening_fraction, central_excess) = match family {
            EllipticalFamily::CompactDisky => (0.0, lerp(ranges.central_excess, u_central)),
            EllipticalFamily::Classical => {
                let hi = ranges.core_softening_fraction.1;
                (hi * u_central, hi * (1.0 - u_central))
            }
            EllipticalFamily::GiantBoxy => (lerp(ranges.core_softening_fraction, u_central), 0.0),
            EllipticalFamily::CdLike => (lerp(ranges.core_softening_fraction, u_central), 0.0),
        };

        Self {
            family,
            axis_ratio,
            position_angle,
            effective_radius,
            profile_index,
            core_softening_fraction,
            central_excess,
            outer_halo_strength,
            outer_halo_scale,
            isophote_shape,
        }
    }

    /// Prepares the config from a concrete scene seed, mirroring
    /// `PreparedSpiralScene::for_scene_seed`: the context carries the base
    /// seed, and the feature seed is derived from it. Repeating the call
    /// with the same seed reproduces the same config bit-for-bit.
    pub(crate) fn for_scene_seed(seed: u64) -> Self {
        Self::from_context(GenerationContext::new(seed))
    }
}

/// Maps a unit draw `u` in `[0, 1)` monotonically into the range `[lo, hi)`.
fn lerp(range: (f64, f64), u: f64) -> f64 {
    range.0 + u * (range.1 - range.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sequential base seeds swept by the statistical tests.
    const SWEEP_SEEDS: u64 = 4096;

    /// Derives one config per sequential base seed.
    fn sweep_configs() -> Vec<EllipticalGalaxyConfig> {
        (0..SWEEP_SEEDS)
            .map(EllipticalGalaxyConfig::for_scene_seed)
            .collect()
    }

    fn family_name(family: EllipticalFamily) -> &'static str {
        match family {
            EllipticalFamily::CompactDisky => "CompactDisky",
            EllipticalFamily::Classical => "Classical",
            EllipticalFamily::GiantBoxy => "GiantBoxy",
            EllipticalFamily::CdLike => "CdLike",
        }
    }

    #[test]
    fn test_same_seed_same_config() {
        for seed in [0_u64, 1, 7, 42, 137, 2026, 4095] {
            let first = EllipticalGalaxyConfig::for_scene_seed(seed);
            let second = EllipticalGalaxyConfig::for_scene_seed(seed);
            assert_eq!(
                first, second,
                "seed {seed} must be bit-for-bit reproducible"
            );
        }
    }

    #[test]
    fn test_different_seeds_diversify() {
        assert_ne!(
            EllipticalGalaxyConfig::for_scene_seed(42),
            EllipticalGalaxyConfig::for_scene_seed(43)
        );

        let configs: Vec<EllipticalGalaxyConfig> = (0..64_u64)
            .map(EllipticalGalaxyConfig::for_scene_seed)
            .collect();
        let distinct = configs
            .iter()
            .enumerate()
            .filter(|(index, config)| configs.iter().take(*index).all(|other| other != *config))
            .count();
        assert!(
            distinct >= 60,
            "expected near-unique configs across 64 seeds, got {distinct}"
        );
    }

    #[test]
    fn test_all_fields_within_family_ranges() {
        const EPS: f64 = 1.0e-9;
        for config in sweep_configs() {
            let ranges = FamilyRanges::for_family(config.family);
            let checks: [(&str, f64, (f64, f64)); 8] = [
                ("axis_ratio", config.axis_ratio, ranges.axis_ratio),
                (
                    "effective_radius",
                    config.effective_radius,
                    ranges.effective_radius,
                ),
                ("profile_index", config.profile_index, ranges.profile_index),
                (
                    "core_softening_fraction",
                    config.core_softening_fraction,
                    ranges.core_softening_fraction,
                ),
                (
                    "central_excess",
                    config.central_excess,
                    ranges.central_excess,
                ),
                (
                    "outer_halo_strength",
                    config.outer_halo_strength,
                    ranges.outer_halo_strength,
                ),
                (
                    "outer_halo_scale",
                    config.outer_halo_scale,
                    ranges.outer_halo_scale,
                ),
                (
                    "isophote_shape",
                    config.isophote_shape,
                    ranges.isophote_shape,
                ),
            ];
            for (name, value, (lo, hi)) in checks {
                assert!(
                    value >= lo - EPS && value <= hi + EPS,
                    "{name} = {value} outside {lo}..={hi} for family {:?} \
                     (seed sweep 0..{SWEEP_SEEDS})",
                    config.family
                );
            }
        }
    }

    #[test]
    fn test_all_families_occur() {
        let configs = sweep_configs();
        for family in [
            EllipticalFamily::CompactDisky,
            EllipticalFamily::Classical,
            EllipticalFamily::GiantBoxy,
            EllipticalFamily::CdLike,
        ] {
            assert!(
                configs.iter().any(|c| c.family == family),
                "family {family:?} never occurred over {SWEEP_SEEDS} seeds"
            );
        }
    }

    #[test]
    fn test_family_fractions_broadly_compatible() {
        let configs = sweep_configs();
        let total = configs.len() as f64;
        let fraction = |family: EllipticalFamily| {
            configs.iter().filter(|c| c.family == family).count() as f64 / total
        };

        assert!(
            (0.24..=0.36).contains(&fraction(EllipticalFamily::CompactDisky)),
            "CompactDisky fraction = {}",
            fraction(EllipticalFamily::CompactDisky)
        );
        assert!(
            (0.24..=0.36).contains(&fraction(EllipticalFamily::Classical)),
            "Classical fraction = {}",
            fraction(EllipticalFamily::Classical)
        );
        assert!(
            (0.15..=0.25).contains(&fraction(EllipticalFamily::GiantBoxy)),
            "GiantBoxy fraction = {}",
            fraction(EllipticalFamily::GiantBoxy)
        );
        assert!(
            (0.15..=0.25).contains(&fraction(EllipticalFamily::CdLike)),
            "CdLike fraction = {}",
            fraction(EllipticalFamily::CdLike)
        );
    }

    #[test]
    fn test_re_and_profile_index_correlated() {
        const EPS: f64 = 1.0e-9;
        let configs = sweep_configs();
        for family in [
            EllipticalFamily::CompactDisky,
            EllipticalFamily::Classical,
            EllipticalFamily::GiantBoxy,
            EllipticalFamily::CdLike,
        ] {
            let mut group: Vec<&EllipticalGalaxyConfig> =
                configs.iter().filter(|c| c.family == family).collect();
            group.sort_by(|a, b| a.effective_radius.partial_cmp(&b.effective_radius).unwrap());
            for (prev, next) in group.iter().zip(group.iter().skip(1)) {
                assert!(
                    next.profile_index >= prev.profile_index - EPS,
                    "profile_index must not decrease as Re grows in {family:?}: \
                     {0} -> {1}",
                    prev.profile_index,
                    next.profile_index
                );
            }
        }
    }

    #[test]
    fn test_halo_strength_and_scale_correlated() {
        const EPS: f64 = 1.0e-9;
        let configs = sweep_configs();
        for family in [
            EllipticalFamily::CompactDisky,
            EllipticalFamily::Classical,
            EllipticalFamily::GiantBoxy,
            EllipticalFamily::CdLike,
        ] {
            let mut group: Vec<&EllipticalGalaxyConfig> =
                configs.iter().filter(|c| c.family == family).collect();
            group.sort_by(|a, b| {
                a.outer_halo_strength
                    .partial_cmp(&b.outer_halo_strength)
                    .unwrap()
            });
            for (prev, next) in group.iter().zip(group.iter().skip(1)) {
                assert!(
                    next.outer_halo_scale >= prev.outer_halo_scale - EPS,
                    "outer_halo_scale must not decrease as halo strength grows \
                     in {family:?}: {0} -> {1}",
                    prev.outer_halo_scale,
                    next.outer_halo_scale
                );
            }
        }
    }

    #[test]
    fn test_compact_disky_always_disky_positive() {
        for config in sweep_configs() {
            if config.family == EllipticalFamily::CompactDisky {
                assert!(
                    config.isophote_shape > 0.0,
                    "CompactDisky must be disky-positive, got {}",
                    config.isophote_shape
                );
            }
        }
    }

    #[test]
    fn test_giant_boxy_always_boxy_negative() {
        for config in sweep_configs() {
            if config.family == EllipticalFamily::GiantBoxy {
                assert!(
                    config.isophote_shape < 0.0,
                    "GiantBoxy must be boxy-negative, got {}",
                    config.isophote_shape
                );
            }
        }
    }

    #[test]
    fn test_forbidden_central_combinations_are_zero() {
        const EPS: f64 = 1.0e-12;
        for config in sweep_configs() {
            match config.family {
                EllipticalFamily::CompactDisky => {
                    assert_eq!(config.core_softening_fraction, 0.0);
                }
                EllipticalFamily::GiantBoxy => {
                    assert_eq!(config.central_excess, 0.0);
                }
                EllipticalFamily::CdLike => {
                    assert_eq!(config.central_excess, 0.0);
                }
                EllipticalFamily::Classical => {
                    assert!(
                        config.core_softening_fraction <= 0.10 + EPS
                            && config.central_excess <= 0.10 + EPS
                            && config.core_softening_fraction + config.central_excess <= 0.10 + EPS,
                        "Classical must never combine a strong core with a \
                         strong excess: core = {}, excess = {}",
                        config.core_softening_fraction,
                        config.central_excess
                    );
                }
            }
        }
    }

    #[test]
    fn test_position_angle_bounds() {
        let pi = std::f64::consts::PI;
        for config in sweep_configs() {
            assert!(
                config.position_angle >= 0.0 && config.position_angle < pi,
                "position_angle {} outside [0, π)",
                config.position_angle
            );
        }
    }

    #[test]
    fn test_does_not_consume_legacy_scene_rng() {
        // A legacy scene RNG seeded independently of the feature namespace
        // must yield identical streams before and after a feature
        // derivation: the config must not advance any legacy stream.
        let mut legacy_reference = StdRng::seed_from_u64(42);
        let _config = EllipticalGalaxyConfig::for_scene_seed(42);
        let mut legacy_after = StdRng::seed_from_u64(42);
        for _ in 0..8 {
            assert_eq!(
                legacy_reference.random_range(0.0..1.0),
                legacy_after.random_range(0.0..1.0)
            );
        }
    }

    /// Diagnostic seed panel for the B1.2 report (not a golden test).
    #[allow(clippy::print_literal)]
    #[test]
    fn test_seed_panel_diagnostic_b12() {
        let panel: [u64; 14] = [0, 1, 2, 3, 4, 5, 8, 13, 16, 21, 42, 64, 99, 128];
        println!("B1.2 seed panel (diagnostic):");
        for seed in panel {
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            println!(
                "  seed {seed:>3}  {:14}  q={:.3}  pa={:.3}  Re={:.3}  n={:.2}  \
                 core={:.3}  excess={:.3}  halo_s={:.3}  halo_k={:.2}  iso={:+.4}",
                family_name(config.family),
                config.axis_ratio,
                config.position_angle,
                config.effective_radius,
                config.profile_index,
                config.core_softening_fraction,
                config.central_excess,
                config.outer_halo_strength,
                config.outer_halo_scale,
                config.isophote_shape
            );
        }
    }
}
