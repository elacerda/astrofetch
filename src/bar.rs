use crate::seed::{GenerationContext, SPIRAL_BAR_V1};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

const BAR_PROBABILITY: f64 = 0.45;
const MIN_HALF_LENGTH: f64 = 0.14;
const MAX_HALF_LENGTH: f64 = 0.28;
const MIN_AXIS_RATIO: f64 = 4.0;
const MAX_AXIS_RATIO: f64 = 7.0;
const MIN_STRENGTH: f64 = 0.12;
const MAX_STRENGTH: f64 = 0.24;

/// Parameters for the optional central bar of a spiral galaxy.
///
/// The bar lives in the intrinsic, deprojected disk plane. `angle_rad` only
/// needs the range `[0, pi)` because a bar is symmetric under a 180-degree
/// rotation. Coupling the spiral-arm roots to the bar ends belongs to the
/// density-integration step, not to random parameter generation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarConfig {
    pub half_length: f64,
    pub axis_ratio: f64,
    pub strength: f64,
    pub angle_rad: f64,
}

impl BarConfig {
    /// Draws the optional bar entirely from the `spiral/bar/v1` feature stream.
    ///
    /// No draw from this function can advance the legacy Spiral RNG. Returning
    /// `None` represents a genuinely unbarred spiral rather than a bar whose
    /// strength was forced to zero.
    pub fn from_context(context: GenerationContext) -> Option<Self> {
        let mut rng = StdRng::seed_from_u64(context.feature_seed(SPIRAL_BAR_V1));

        if rng.random::<f64>() >= BAR_PROBABILITY {
            return None;
        }

        Some(Self {
            half_length: rng.random_range(MIN_HALF_LENGTH..MAX_HALF_LENGTH),
            axis_ratio: rng.random_range(MIN_AXIS_RATIO..MAX_AXIS_RATIO),
            strength: rng.random_range(MIN_STRENGTH..MAX_STRENGTH),
            angle_rad: rng.random_range(0.0..std::f64::consts::PI),
        })
    }

    /// Half-width of the bar's minor axis in intrinsic disk coordinates.
    pub fn half_width(self) -> f64 {
        self.half_length / self.axis_ratio
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_config_is_deterministic_for_same_scene_seed() {
        let context = GenerationContext::new(42);

        assert_eq!(
            BarConfig::from_context(context),
            BarConfig::from_context(context)
        );
    }

    #[test]
    fn test_bar_config_population_contains_barred_and_unbarred_scenes() {
        let mut barred = 0usize;
        let mut unbarred = 0usize;

        for seed in 0..512_u64 {
            match BarConfig::from_context(GenerationContext::new(seed)) {
                Some(_) => barred += 1,
                None => unbarred += 1,
            }
        }

        assert!(barred > 0, "expected at least one barred scene");
        assert!(unbarred > 0, "expected at least one unbarred scene");
    }

    #[test]
    fn test_bar_config_values_stay_in_v1_ranges() {
        for seed in 0..512_u64 {
            let Some(bar) = BarConfig::from_context(GenerationContext::new(seed)) else {
                continue;
            };

            assert!((MIN_HALF_LENGTH..MAX_HALF_LENGTH).contains(&bar.half_length));
            assert!((MIN_AXIS_RATIO..MAX_AXIS_RATIO).contains(&bar.axis_ratio));
            assert!((MIN_STRENGTH..MAX_STRENGTH).contains(&bar.strength));
            assert!((0.0..std::f64::consts::PI).contains(&bar.angle_rad));
            assert!(bar.half_width() > 0.0);
            assert!(bar.half_width() < bar.half_length);
        }
    }

    #[test]
    fn test_bar_config_changes_across_scene_seeds() {
        let configs: Vec<_> = (0..64_u64)
            .filter_map(|seed| BarConfig::from_context(GenerationContext::new(seed)))
            .collect();

        assert!(configs.len() >= 2, "expected multiple barred seeds");
        assert!(
            configs.windows(2).any(|pair| pair[0] != pair[1]),
            "different scene seeds should not all produce the same bar"
        );
    }
}
