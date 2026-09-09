//! Deterministic dust-lane configuration for spiral galaxies.
//!
//! This module derives an optional [`DustLaneConfig`] from the
//! `spiral/dust/v1` feature stream. Phase 2B consumes the configuration in
//! Spiral generation as a deterministic multiplicative extinction of the
//! luminous disk; the dust stream remains isolated from the legacy scene RNG
//! and the `spiral/bar/v1` stream.

use crate::seed::{GenerationContext, SPIRAL_DUST_V1};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

/// Probability that a scene carries a dust-lane configuration.
///
/// Procedural calibration value, not an empirically validated
/// astrophysical distribution.
const DUST_PROBABILITY: f64 = 0.60;

/// Frozen `spiral/dust/v1` calibration ranges.
///
/// These are procedural calibration ranges chosen for visual plausibility,
/// not empirically validated astrophysical distributions.
const MIN_STRENGTH: f64 = 0.25;
const MAX_STRENGTH: f64 = 0.55;
const MIN_OFFSET: f64 = -0.30;
const MAX_OFFSET: f64 = 0.30;
const MIN_WIDTH_FACTOR: f64 = 0.5;
const MAX_WIDTH_FACTOR: f64 = 1.2;

/// Optional dust-lane parameters for a spiral galaxy.
///
/// All fields are procedural calibration values, not physically measured
/// quantities:
///
/// - `strength` is a dimensionless optical-depth (tau) amplitude for the
///   extinction relation `tau = strength * profile * radial_gate`,
///   `extinction = exp(-tau)`. It is **not** a fractional attenuation depth.
/// - `offset` is a signed arm-phase offset in radians. The current model has
///   no explicit chirality or rotation-direction semantics, so the offset is
///   not a leading/trailing statement.
/// - `width_factor` scales the local spiral-arm width for the dust lanes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DustLaneConfig {
    pub strength: f64,
    pub offset: f64,
    pub width_factor: f64,
}

impl DustLaneConfig {
    /// Draws the optional dust-lane configuration entirely from the
    /// `spiral/dust/v1` feature stream.
    ///
    /// The function creates its own `StdRng` seeded exclusively from
    /// `context.feature_seed(SPIRAL_DUST_V1)`. The draw order is frozen:
    ///
    /// 1. `rng.random::<f64>()` for presence; returns `None` when the draw is
    ///    greater than or equal to `DUST_PROBABILITY` (0.60).
    /// 2. `strength` from `0.25..0.55`.
    /// 3. `offset` from `-0.30..0.30` radians.
    /// 4. `width_factor` from `0.5..1.2`.
    ///
    /// No draw from this function can advance the legacy Spiral RNG or any
    /// other feature stream. Returning `None` represents a genuinely
    /// dustless scene rather than a dust lane whose strength was forced to
    /// zero.
    pub fn from_context(context: GenerationContext) -> Option<Self> {
        let mut rng = StdRng::seed_from_u64(context.feature_seed(SPIRAL_DUST_V1));

        if rng.random::<f64>() >= DUST_PROBABILITY {
            return None;
        }

        Some(Self {
            strength: rng.random_range(MIN_STRENGTH..MAX_STRENGTH),
            offset: rng.random_range(MIN_OFFSET..MAX_OFFSET),
            width_factor: rng.random_range(MIN_WIDTH_FACTOR..MAX_WIDTH_FACTOR),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::SpiralGalaxyConfig;

    #[test]
    fn test_dust_config_is_deterministic_for_same_scene_seed() {
        let context = GenerationContext::new(42);

        assert_eq!(
            DustLaneConfig::from_context(context),
            DustLaneConfig::from_context(context)
        );
    }

    #[test]
    fn test_dust_config_population_contains_dusty_and_dustless_scenes() {
        let mut dusty = 0usize;
        let mut dustless = 0usize;

        for seed in 0..512_u64 {
            match DustLaneConfig::from_context(GenerationContext::new(seed)) {
                Some(_) => dusty += 1,
                None => dustless += 1,
            }
        }

        assert!(dusty > 0, "expected at least one dusty scene");
        assert!(dustless > 0, "expected at least one dustless scene");
    }

    #[test]
    fn test_dust_config_values_stay_in_v1_ranges() {
        for seed in 0..512_u64 {
            let Some(dust) = DustLaneConfig::from_context(GenerationContext::new(seed)) else {
                continue;
            };

            assert!((MIN_STRENGTH..MAX_STRENGTH).contains(&dust.strength));
            assert!((MIN_OFFSET..MAX_OFFSET).contains(&dust.offset));
            assert!((MIN_WIDTH_FACTOR..MAX_WIDTH_FACTOR).contains(&dust.width_factor));
        }
    }

    #[test]
    fn test_dust_config_changes_across_scene_seeds() {
        let configs: Vec<_> = (0..64_u64)
            .filter_map(|seed| DustLaneConfig::from_context(GenerationContext::new(seed)))
            .collect();

        assert!(configs.len() >= 2, "expected multiple dusty seeds");
        assert!(
            configs.windows(2).any(|pair| pair[0] != pair[1]),
            "different scene seeds should not all produce the same dust config"
        );
    }

    #[test]
    fn test_dust_config_does_not_advance_legacy_spiral_rng() {
        let seed = 42;

        let mut baseline_rng = StdRng::seed_from_u64(seed);
        let baseline_config = SpiralGalaxyConfig::from_rng(&mut baseline_rng);
        let baseline_noise_seed = baseline_rng.random::<u32>();

        let mut isolated_rng = StdRng::seed_from_u64(seed);
        let isolated_config = SpiralGalaxyConfig::from_rng(&mut isolated_rng);

        let _ = DustLaneConfig::from_context(GenerationContext::new(seed));

        let isolated_noise_seed = isolated_rng.random::<u32>();

        assert_eq!(isolated_config, baseline_config);
        assert_eq!(isolated_noise_seed, baseline_noise_seed);
    }
}
