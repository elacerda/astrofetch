/// Render profile for a resolved model.
///
/// Encapsulates all presentation decisions: normalization, stretch, threshold,
/// and renderer selection.
use crate::density::DensityMap;
use crate::engine::ArtModel;
use crate::render::stretch::{apply_gamma_stretch, StretchType};

/// Normalization strategy for density maps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Normalization {
    /// No normalization - pass raw values through.
    None,
    /// Robust percentile normalization.
    Robust {
        low_percentile: f64,
        high_percentile: f64,
    },
}

/// Threshold strategy for determining visibility cutoff.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThresholdStrategy {
    /// Target a specific occupancy fraction for visible cells.
    TargetOccupancy(f64),
    /// Dedicated renderer handles its own thresholds (e.g., Starfield).
    Dedicated,
}

/// Renderer family selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererKind {
    /// Shade renderer with vertical supersampling for galaxy-like models.
    Shade,
    /// Sparse starfield renderer.
    Starfield,
}

/// Complete render profile for a model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderProfile {
    pub normalization: Normalization,
    pub stretch: StretchType,
    pub threshold: ThresholdStrategy,
    pub renderer: RendererKind,
}

impl RenderProfile {
    /// Returns the render profile for a resolved model.
    ///
    /// `ArtModel::Random` is invalid at this boundary because Patch 1 guarantees
    /// that the model has already been resolved to a concrete type.
    pub fn for_model(model: ArtModel) -> Self {
        match model {
            ArtModel::Spiral => RenderProfile {
                normalization: Normalization::Robust {
                    low_percentile: 0.02,
                    high_percentile: 0.98,
                },
                stretch: StretchType::Gamma(0.85),
                threshold: ThresholdStrategy::TargetOccupancy(0.26),
                renderer: RendererKind::Shade,
            },
            ArtModel::Elliptical => RenderProfile {
                normalization: Normalization::Robust {
                    low_percentile: 0.02,
                    high_percentile: 0.98,
                },
                stretch: StretchType::Gamma(0.7),
                threshold: ThresholdStrategy::TargetOccupancy(0.23),
                renderer: RendererKind::Shade,
            },
            ArtModel::Cluster => RenderProfile {
                normalization: Normalization::Robust {
                    low_percentile: 0.02,
                    high_percentile: 0.98,
                },
                stretch: StretchType::Gamma(0.65),
                threshold: ThresholdStrategy::TargetOccupancy(0.10),
                renderer: RendererKind::Shade,
            },
            ArtModel::Starfield => RenderProfile {
                normalization: Normalization::None,
                stretch: StretchType::None,
                threshold: ThresholdStrategy::Dedicated,
                renderer: RendererKind::Starfield,
            },
            ArtModel::Random => {
                // This should never happen - Random is resolved before this is called
                panic!("Random model should be resolved before getting render profile")
            }
        }
    }
}

/// Prepared density result that encodes the processing path at the type level.
///
/// This enum makes invalid states impossible by separating Starfield (no
/// normalization/stretch/threshold) from Shade models (robust normalization,
/// stretch, and target-occupancy threshold).
#[derive(Debug, Clone)]
pub enum PreparedDensity {
    /// Starfield: raw density values, no processing.
    Starfield { density: DensityMap },
    /// Shade models: normalized, stretched, with computed threshold.
    Shade { density: DensityMap, threshold: f64 },
}

/// Prepares density for rendering based on the profile.
///
/// * Starfield models: no normalization, no stretch, no threshold.
/// * Shade models: robust normalization, stretch, and target-occupancy threshold.
pub fn prepare_density(density: DensityMap, profile: RenderProfile) -> PreparedDensity {
    match profile.renderer {
        RendererKind::Starfield => {
            // Starfield: no processing, preserve raw values
            PreparedDensity::Starfield { density }
        }
        RendererKind::Shade => {
            // Shade models: apply robust normalization and stretch
            let normalized = normalize_robust_map(&density, profile.normalization);
            let stretched = apply_stretch_to_density(&normalized, profile.stretch);
            let threshold = compute_target_occupancy_threshold(&stretched, profile.threshold);
            PreparedDensity::Shade {
                density: stretched,
                threshold,
            }
        }
    }
}

/// Robust percentile normalization operating on DensityMap.
///
/// This function:
/// - Uses only finite positive values for percentile estimation
/// - Maps non-finite, negative, and zero values to 0.0
/// - Preserves dimensions
/// - Produces values in [0, 1]
fn normalize_robust_map(density: &DensityMap, normalization: Normalization) -> DensityMap {
    match normalization {
        Normalization::None => density.clone(),
        Normalization::Robust {
            low_percentile,
            high_percentile,
        } => {
            // Flatten finite positive values
            let mut values: Vec<f64> = density
                .data
                .iter()
                .copied()
                .filter(|v| v.is_finite() && *v > 0.0)
                .collect();

            if values.is_empty() {
                // Empty or zero map remains zero
                return DensityMap::new(density.width, density.height);
            }

            // Sort deterministically using total_cmp
            values.sort_by(f64::total_cmp);

            let n = values.len();
            let low_idx = ((n - 1) as f64 * low_percentile.clamp(0.0, 1.0)).floor() as usize;
            let high_idx = ((n - 1) as f64 * high_percentile.clamp(0.0, 1.0)).floor() as usize;

            // Ensure high_idx >= low_idx
            let high_idx = high_idx.max(low_idx);

            let low_val = values[low_idx.min(n - 1)];
            let high_val = values[high_idx.min(n - 1)];

            let range = high_val - low_val;

            if range.abs() < f64::EPSILON {
                // All finite positive values are equal
                return DensityMap {
                    width: density.width,
                    height: density.height,
                    data: density
                        .data
                        .iter()
                        .map(|v| if v.is_finite() && *v > 0.0 { 1.0 } else { 0.0 })
                        .collect(),
                };
            }

            DensityMap {
                width: density.width,
                height: density.height,
                data: density
                    .data
                    .iter()
                    .map(|v| {
                        if !v.is_finite() || *v <= 0.0 {
                            0.0
                        } else {
                            let clamped = (v - low_val) / range;
                            clamped.clamp(0.0, 1.0)
                        }
                    })
                    .collect(),
            }
        }
    }
}

/// Applies stretch to a DensityMap.
fn apply_stretch_to_density(density: &DensityMap, stretch: StretchType) -> DensityMap {
    match stretch {
        StretchType::None => density.clone(),
        StretchType::Gamma(gamma) => DensityMap {
            width: density.width,
            height: density.height,
            data: density
                .data
                .iter()
                .map(|v| apply_gamma_stretch(*v, gamma))
                .collect(),
        },
    }
}

/// Computes the target-occupancy threshold from vertical pair maxima.
///
/// Occupancy means terminal Shade cells before background-star injection.
fn compute_target_occupancy_threshold(density: &DensityMap, strategy: ThresholdStrategy) -> f64 {
    match strategy {
        ThresholdStrategy::Dedicated => 0.0, // Not used for Starfield
        ThresholdStrategy::TargetOccupancy(target) => {
            // Collect vertical pair maxima
            let mut pair_maxima: Vec<f64> = Vec::new();

            for y in (0..density.height).step_by(2) {
                for x in 0..density.width {
                    let top = density.get(x, y);
                    let bottom = if y + 1 < density.height {
                        density.get(x, y + 1)
                    } else {
                        0.0
                    }
                    .clamp(0.0, 1.0);
                    let pair_value = top.max(bottom);
                    // Sanitize non-finite or negative values to zero
                    let sanitized = if !pair_value.is_finite() || pair_value < 0.0 {
                        0.0
                    } else {
                        pair_value
                    };
                    pair_maxima.push(sanitized);
                }
            }

            if pair_maxima.is_empty() {
                return 0.0;
            }

            // Sort deterministically with f64::total_cmp
            pair_maxima.sort_by(f64::total_cmp);

            // Choose threshold near quantile (1.0 - target)
            let n = pair_maxima.len();
            let quantile = (1.0 - target).clamp(0.0, 1.0);
            let idx = ((n - 1) as f64 * quantile).round() as usize;

            pair_maxima[idx.min(n - 1)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::generate_spiral_galaxy;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_profile_for_spiral() {
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        assert_eq!(profile.renderer, RendererKind::Shade);
        assert!(matches!(
            profile.normalization,
            Normalization::Robust {
                low_percentile: 0.02,
                high_percentile: 0.98
            }
        ));
        assert_eq!(profile.stretch, StretchType::Gamma(0.85));
        assert!(matches!(
            profile.threshold,
            ThresholdStrategy::TargetOccupancy(0.26)
        ));
    }

    #[test]
    fn test_profile_for_elliptical() {
        let profile = RenderProfile::for_model(ArtModel::Elliptical);
        assert_eq!(profile.renderer, RendererKind::Shade);
        assert!(matches!(
            profile.normalization,
            Normalization::Robust {
                low_percentile: 0.02,
                high_percentile: 0.98
            }
        ));
        assert_eq!(profile.stretch, StretchType::Gamma(0.7));
        assert!(matches!(
            profile.threshold,
            ThresholdStrategy::TargetOccupancy(0.23)
        ));
    }

    #[test]
    fn test_profile_for_cluster() {
        let profile = RenderProfile::for_model(ArtModel::Cluster);
        assert_eq!(profile.renderer, RendererKind::Shade);
        assert!(matches!(
            profile.normalization,
            Normalization::Robust {
                low_percentile: 0.02,
                high_percentile: 0.98
            }
        ));
        assert_eq!(profile.stretch, StretchType::Gamma(0.65));
        assert!(matches!(
            profile.threshold,
            ThresholdStrategy::TargetOccupancy(0.10)
        ));
    }

    #[test]
    fn test_profile_for_starfield() {
        let profile = RenderProfile::for_model(ArtModel::Starfield);
        assert_eq!(profile.renderer, RendererKind::Starfield);
        assert_eq!(profile.normalization, Normalization::None);
        assert_eq!(profile.stretch, StretchType::None);
        assert!(matches!(profile.threshold, ThresholdStrategy::Dedicated));
    }

    #[test]
    fn test_prepare_density_shade_applies_normalization() {
        let density = DensityMap::new(10, 5);
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { density, threshold } => {
                assert_eq!(density.width, 10);
                assert_eq!(density.height, 5);
                // All zeros should remain zeros
                assert!(density.data.iter().all(|v| *v == 0.0));
                // Threshold should be 0.0 for all-zero map
                assert_eq!(threshold, 0.0);
            }
            PreparedDensity::Starfield { .. } => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_normalize_robust_empty_map() {
        let density = DensityMap::new(5, 5);
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { density, .. } => {
                assert!(density.data.iter().all(|v| *v == 0.0));
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_normalize_robust_with_positive_values() {
        let mut density = DensityMap::new(5, 5);
        for i in 0..25 {
            density.data[i] = (i as f64 + 1.0) / 25.0;
        }
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { density, .. } => {
                // Values should be in [0, 1]
                assert!(density.data.iter().all(|v| *v >= 0.0 && *v <= 1.0));
                // Some values should be non-zero
                assert!(density.data.iter().any(|v| *v > 0.0));
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_normalize_robust_with_nan() {
        let mut density = DensityMap::new(5, 5);
        density.data[12] = f64::NAN;
        for i in 0..25 {
            if i != 12 {
                density.data[i] = 0.5;
            }
        }
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { density, .. } => {
                // NaN should map to 0.0
                assert_eq!(density.data[12], 0.0);
                // Other values should be in [0, 1]
                assert!(density.data.iter().all(|v| *v >= 0.0 && *v <= 1.0));
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_normalize_robust_with_inf() {
        let mut density = DensityMap::new(5, 5);
        density.data[12] = f64::INFINITY;
        for i in 0..25 {
            if i != 12 {
                density.data[i] = 0.5;
            }
        }
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { density, .. } => {
                // Infinity should map to 0.0
                assert_eq!(density.data[12], 0.0);
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_normalize_robust_negative_values() {
        let mut density = DensityMap::new(5, 5);
        for i in 0..25 {
            density.data[i] = -((i as f64) / 25.0);
        }
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { density, .. } => {
                // All negative values should map to 0.0
                assert!(density.data.iter().all(|v| *v == 0.0));
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_normalize_robust_negative_infinity() {
        let mut density = DensityMap::new(5, 5);
        for i in 0..25 {
            density.data[i] = f64::NEG_INFINITY;
        }
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { density, .. } => {
                // Negative infinity should map to 0.0
                assert!(density.data.iter().all(|v| *v == 0.0));
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_normalize_robust_extreme_positive_outlier() {
        // Create a map with ordinary values having a real range plus one extreme outlier
        // Values: 0.10, 0.11, 0.12, ..., 0.33, 1000.0
        let mut density = DensityMap::new(5, 5);
        for i in 0..24 {
            density.data[i] = 0.10 + (i as f64) * 0.01; // 0.10, 0.11, ..., 0.33
        }
        density.data[24] = 1000.0; // Extreme outlier
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { density, .. } => {
                // Values should be in [0, 1]
                assert!(density.data.iter().all(|v| *v >= 0.0 && *v <= 1.0));
                // The outlier should be mapped to 1.0
                assert!((density.data[24] - 1.0).abs() < f64::EPSILON);
                // At least two ordinary positive values should remain different after normalization
                assert!(
                    density.data[0] < density.data[1],
                    "0.10 should map to a lower value than 0.11"
                );
                assert!(
                    density.data[1] < density.data[2],
                    "0.11 should map to a lower value than 0.12"
                );
                // At least one ordinary value should remain strictly between 0.0 and 1.0
                // After robust normalization with 2nd percentile low, 0.10 (the lowest) maps to 0.0
                // and 0.11 should map to a small positive value
                assert!(
                    density.data[1] > 0.0 && density.data[1] < 1.0,
                    "0.11 should map to a value between 0 and 1"
                );
                // All outputs should be finite
                assert!(density.data.iter().all(|v| v.is_finite()));
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_normalize_robust_collapsed_positive_range() {
        // All finite positive values are equal
        let mut density = DensityMap::new(5, 5);
        for i in 0..25 {
            density.data[i] = 0.5;
        }
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { density, .. } => {
                // All finite positive values should map to 1.0 when range is collapsed
                assert!(density.data.iter().all(|v| *v == 1.0));
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_normalize_robust_mixed_values() {
        // Create a map with: negative, zero, finite positive, NaN, positive infinity, negative infinity
        let mut density = DensityMap::new(6, 1);
        density.data[0] = -1.0; // negative
        density.data[1] = 0.0; // zero
        density.data[2] = 0.5; // finite positive
        density.data[3] = f64::NAN; // NaN
        density.data[4] = f64::INFINITY; // positive infinity
        density.data[5] = f64::NEG_INFINITY; // negative infinity
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { density, .. } => {
                // Negative, zero, NaN, and infinities should map to exactly 0.0
                assert_eq!(density.data[0], 0.0, "negative should map to 0.0");
                assert_eq!(density.data[1], 0.0, "zero should remain 0.0");
                assert_eq!(density.data[3], 0.0, "NaN should map to 0.0");
                assert_eq!(density.data[4], 0.0, "positive infinity should map to 0.0");
                assert_eq!(density.data[5], 0.0, "negative infinity should map to 0.0");
                // Finite positive should be normalized to some value in (0, 1]
                assert!(
                    density.data[2] > 0.0 && density.data[2] <= 1.0,
                    "finite positive should be in (0, 1]"
                );
                // All outputs should be finite
                assert!(density.data.iter().all(|v| v.is_finite()));
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_normalize_robust_preserves_dimensions() {
        let mut density = DensityMap::new(10, 8);
        for i in 0..80 {
            density.data[i] = (i as f64 + 1.0) / 80.0;
        }
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { density, .. } => {
                assert_eq!(density.width, 10);
                assert_eq!(density.height, 8);
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_prepare_density_starfield_preserves_values() {
        // Starfield should preserve raw density values without normalization or modification
        let original = DensityMap {
            width: 3,
            height: 2,
            data: vec![0.0, 0.04, 0.18, 0.001, 0.08, 0.0],
        };
        let profile = RenderProfile::for_model(ArtModel::Starfield);
        let prepared = prepare_density(original.clone(), profile);
        match prepared {
            PreparedDensity::Starfield { density } => {
                assert_eq!(density, original);
            }
            _ => panic!("Expected Starfield variant"),
        }
    }

    /// Creates a density map with known vertical pair maxima for testing.
    /// Each pair (row 0, row 1) has max value = value at row 0 since row 1 is 0.
    fn make_density_with_known_pairs() -> DensityMap {
        // 5 rows, 4 columns
        // Row 0: [0.1, 0.2, 0.3, 0.4]
        // Row 1: [0.0, 0.0, 0.0, 0.0] -> pairs: max(0.1,0)=0.1, max(0.2,0)=0.2, etc.
        // Row 2: [0.5, 0.6, 0.7, 0.8]
        // Row 3: [0.0, 0.0, 0.0, 0.0] -> pairs: max(0.5,0)=0.5, max(0.6,0)=0.6, etc.
        // Row 4: [0.9, 1.0, 0.0, 0.0] -> pairs: max(0.9,0)=0.9, max(1.0,0)=1.0 (row 5 doesn't exist)
        let mut density = DensityMap::new(4, 5);
        density.set(0, 0, 0.1);
        density.set(1, 0, 0.2);
        density.set(2, 0, 0.3);
        density.set(3, 0, 0.4);
        density.set(0, 2, 0.5);
        density.set(1, 2, 0.6);
        density.set(2, 2, 0.7);
        density.set(3, 2, 0.8);
        density.set(0, 4, 0.9);
        density.set(1, 4, 1.0);
        density
    }

    /// Computes the expected threshold after normalization and stretch.
    /// This is the same algorithm used in prepare_density -> compute_target_occupancy_threshold.
    fn compute_expected_threshold(density: &DensityMap, profile: RenderProfile) -> f64 {
        // Apply normalization
        let normalized = normalize_robust_map(density, profile.normalization);
        // Apply stretch
        let stretched = apply_stretch_to_density(&normalized, profile.stretch);
        // Compute threshold on stretched values
        compute_target_occupancy_threshold(&stretched, profile.threshold)
    }

    #[test]
    fn test_target_occupancy_exact_threshold_for_known_distribution() {
        // With 4 columns and 5 rows, we have 2 full pairs + 1 partial pair = 9 pairs total
        // After normalization: [0.0, 0.0102, 0.0204, ..., 0.9796, 1.0] (25 values)
        // After gamma stretch with gamma=0.85, values change non-linearly
        // The threshold is computed on the stretched values
        let density = make_density_with_known_pairs();
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let expected_threshold = compute_expected_threshold(&density, profile);
        let prepared = prepare_density(density.clone(), profile);
        match prepared {
            PreparedDensity::Shade { threshold, .. } => {
                // Exact threshold value expected
                assert!(
                    (threshold - expected_threshold).abs() < f64::EPSILON,
                    "Expected threshold {}, got {}",
                    expected_threshold,
                    threshold
                );
                // Verify threshold is deterministic
                let prepared2 = prepare_density(density, profile);
                match prepared2 {
                    PreparedDensity::Shade { threshold: t2, .. } => {
                        assert!(
                            (threshold - t2).abs() < f64::EPSILON,
                            "Threshold should be deterministic"
                        );
                    }
                    _ => panic!("Expected Shade variant"),
                }
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_target_occupancy_uses_vertical_pairs() {
        // Create a map where vertical pairs have different maxima than individual cells
        // Row 0: [0.1, 0.9]
        // Row 1: [0.8, 0.2]
        // Vertical pairs: max(0.1, 0.8)=0.8, max(0.9, 0.2)=0.9
        // Sorted: [0.8, 0.9]
        // For 2 pairs, quantile 0.74: index = 1 * 0.74 = 0.74 -> round to 1
        // Threshold = 0.9 (after normalization and stretch)
        let mut density = DensityMap::new(2, 2);
        density.set(0, 0, 0.1);
        density.set(1, 0, 0.9);
        density.set(0, 1, 0.8);
        density.set(1, 1, 0.2);
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let expected_threshold = compute_expected_threshold(&density, profile);
        let prepared = prepare_density(density.clone(), profile);
        match prepared {
            PreparedDensity::Shade { threshold, .. } => {
                // Exact threshold value expected
                assert!(
                    (threshold - expected_threshold).abs() < f64::EPSILON,
                    "Expected threshold {}, got {}",
                    expected_threshold,
                    threshold
                );
                // Verify threshold is deterministic
                let prepared2 = prepare_density(density, profile);
                match prepared2 {
                    PreparedDensity::Shade { threshold: t2, .. } => {
                        assert!(
                            (threshold - t2).abs() < f64::EPSILON,
                            "Threshold should be deterministic"
                        );
                    }
                    _ => panic!("Expected Shade variant"),
                }
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_target_occupancy_odd_number_of_rows() {
        // 3 rows, 2 columns -> 2 pairs (one partial)
        // Row 0: [0.3, 0.4]
        // Row 1: [0.1, 0.2]
        // Row 2: [0.5, 0.6] (partial pair with 0.0)
        // Pairs: max(0.3, 0.1)=0.3, max(0.4, 0.2)=0.4, max(0.5, 0)=0.5
        // Sorted: [0.3, 0.4, 0.5]
        // For 3 pairs, quantile 0.74: index = 2 * 0.74 = 1.48 -> round to 1
        // Threshold = 0.4 (after normalization and stretch)
        let mut density = DensityMap::new(2, 3);
        density.set(0, 0, 0.3);
        density.set(1, 0, 0.4);
        density.set(0, 1, 0.1);
        density.set(1, 1, 0.2);
        density.set(0, 2, 0.5);
        density.set(1, 2, 0.6);
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let expected_threshold = compute_expected_threshold(&density, profile);
        let prepared = prepare_density(density.clone(), profile);
        match prepared {
            PreparedDensity::Shade { threshold, .. } => {
                // Exact threshold value expected
                assert!(
                    (threshold - expected_threshold).abs() < f64::EPSILON,
                    "Expected threshold {}, got {}",
                    expected_threshold,
                    threshold
                );
                // Verify threshold is deterministic
                let prepared2 = prepare_density(density, profile);
                match prepared2 {
                    PreparedDensity::Shade { threshold: t2, .. } => {
                        assert!(
                            (threshold - t2).abs() < f64::EPSILON,
                            "Threshold should be deterministic"
                        );
                    }
                    _ => panic!("Expected Shade variant"),
                }
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_target_occupancy_all_zero_map() {
        let density = DensityMap::new(10, 10);
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared = prepare_density(density, profile);
        match prepared {
            PreparedDensity::Shade { threshold, .. } => {
                // All-zero map should not panic, threshold should be 0.0
                assert_eq!(threshold, 0.0);
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    #[test]
    fn test_target_occupancy_repeated_calculation_same_result() {
        let density = make_density_with_known_pairs();
        let profile = RenderProfile::for_model(ArtModel::Spiral);
        let prepared1 = prepare_density(density.clone(), profile);
        let prepared2 = prepare_density(density, profile);
        match (prepared1, prepared2) {
            (
                PreparedDensity::Shade { threshold: t1, .. },
                PreparedDensity::Shade { threshold: t2, .. },
            ) => {
                assert_eq!(t1, t2, "Repeated calculation should give same threshold");
            }
            _ => panic!("Expected Shade variant"),
        }
    }

    /// Spiral generation tests (focused on density support, not rendering)

    #[test]
    fn test_spiral_generation_deterministic() {
        let mut rng1 = StdRng::seed_from_u64(7);
        let mut rng2 = StdRng::seed_from_u64(7);

        let map1 = generate_spiral_galaxy(40, 20, &mut rng1);
        let map2 = generate_spiral_galaxy(40, 20, &mut rng2);

        assert_eq!(map1, map2);
    }

    #[test]
    fn test_spiral_generation_dimensions() {
        let mut rng = StdRng::seed_from_u64(42);
        let map = generate_spiral_galaxy(40, 20, &mut rng);

        assert_eq!(map.width, 40);
        assert_eq!(map.height, 40); // 20 * 2
    }

    #[test]
    fn test_spiral_has_sufficient_positive_support() {
        // Test canonical seeds that previously had low raw positive support
        let canonical_seeds = [7, 137, 2026];

        for seed in &canonical_seeds {
            let mut rng = StdRng::seed_from_u64(*seed);
            let map = generate_spiral_galaxy(40, 20, &mut rng);

            // Count vertical pair maxima that are finite and > 0
            let mut positive_count = 0;
            let mut total_pairs = 0;

            for y in (0..map.height).step_by(2) {
                for x in 0..map.width {
                    let top = map.get(x, y);
                    let bottom = if y + 1 < map.height {
                        map.get(x, y + 1)
                    } else {
                        0.0
                    };
                    let pair_max = top.max(bottom);
                    if pair_max.is_finite() && pair_max > 0.0 {
                        positive_count += 1;
                    }
                    total_pairs += 1;
                }
            }

            let positive_support = (positive_count as f64) / (total_pairs as f64);
            // After cutoff removal, all canonical seeds should have sufficient support
            assert!(
                positive_support >= 0.20,
                "Seed {} has insufficient positive support: {:.4}",
                seed,
                positive_support
            );
        }
    }

    /// Cluster generation tests (focused on density support, not rendering)

    #[test]
    fn test_cluster_generation_deterministic() {
        let _rng1 = StdRng::seed_from_u64(42);
        let _rng2 = StdRng::seed_from_u64(42);

        let scene1 = ArtModel::Cluster.generate_scene(40, 20, Some(42));
        let scene2 = ArtModel::Cluster.generate_scene(40, 20, Some(42));

        assert_eq!(scene1.density, scene2.density);
    }

    #[test]
    fn test_cluster_generation_dimensions() {
        let scene = ArtModel::Cluster.generate_scene(40, 20, Some(42));

        assert_eq!(scene.density.width, 40);
        assert_eq!(scene.density.height, 40); // 20 * 2
    }

    #[test]
    fn test_cluster_has_sufficient_positive_support() {
        // Test canonical seeds that previously had low raw positive support
        let canonical_seeds = [42];

        for seed in &canonical_seeds {
            let scene = ArtModel::Cluster.generate_scene(40, 20, Some(*seed));

            // Count vertical pair maxima that are finite and > 0
            let mut positive_count = 0;
            let mut total_pairs = 0;

            for y in (0..scene.density.height).step_by(2) {
                for x in 0..scene.density.width {
                    let top = scene.density.get(x, y);
                    let bottom = if y + 1 < scene.density.height {
                        scene.density.get(x, y + 1)
                    } else {
                        0.0
                    };
                    let pair_max = top.max(bottom);
                    if pair_max.is_finite() && pair_max > 0.0 {
                        positive_count += 1;
                    }
                    total_pairs += 1;
                }
            }

            let positive_support = (positive_count as f64) / (total_pairs as f64);
            // After cutoff removal, cluster should have sufficient support (minimum 5%)
            assert!(
                positive_support >= 0.05,
                "Seed {} has insufficient positive support: {:.4}",
                seed,
                positive_support
            );
        }
    }

    /// Calculates visible terminal-cell occupancy using vertical pair maxima.
    ///
    /// This is the same calculation used by the renderer to determine which
    /// cells are visible. It counts cells where the pair maximum exceeds the threshold.
    ///
    /// Note: Background stars are NOT counted as visible cells for occupancy.
    /// The renderer only counts cells with galaxy structure (non-space glyphs).
    fn calculate_occupancy(density: &DensityMap, threshold: f64) -> f64 {
        let mut visible_count = 0;
        let mut total_pairs = 0;

        for y in (0..density.height).step_by(2) {
            for x in 0..density.width {
                let top = density.get(x, y);
                let bottom = if y + 1 < density.height {
                    density.get(x, y + 1)
                } else {
                    0.0
                };
                let pair_max = top.max(bottom);
                // Only count cells where pair_max is finite, > 0, and >= threshold
                // This matches the renderer's glyph_for_density_pair condition
                if pair_max.is_finite() && pair_max > 0.0 && pair_max >= threshold {
                    visible_count += 1;
                }
                total_pairs += 1;
            }
        }

        if total_pairs == 0 {
            return 0.0;
        }

        (visible_count as f64) / (total_pairs as f64)
    }

    /// Test canonical occupancy for all models and seeds.
    ///
    /// This test verifies that the density processing produces reasonable
    /// terminal-cell occupancy for each galaxy model type.
    ///
    /// This diagnostic version collects all measurements before asserting,
    /// printing a complete table for all model/seed combinations.
    #[allow(clippy::print_literal)]
    #[test]
    fn test_canonical_occupancy() {
        let models = [
            (ArtModel::Spiral, 0.20, 0.32, 0.26), // Spiral: 20-32%, target 26%
            (ArtModel::Elliptical, 0.18, 0.28, 0.23), // Elliptical: 18-28%, target 23%
            (ArtModel::Cluster, 0.05, 0.15, 0.10), // Cluster: 5-15%, target 10%
        ];
        let seeds = [0, 1, 7, 42, 137, 2026];

        // Store all measurements
        struct Measurement {
            model: ArtModel,
            seed: u64,
            target_occupancy: f64,
            raw_positive_support: f64,
            processed_positive_support: f64,
            threshold: f64,
            visible_occupancy: f64,
            out_of_range: bool,
        }

        let mut measurements: Vec<Measurement> = Vec::new();

        for &(model, min_occ, max_occ, target_occ) in &models {
            for seed in &seeds {
                // Generate scene
                let scene = model.generate_scene(40, 20, Some(*seed));

                // Get profile for model
                let profile = RenderProfile::for_model(model);

                // ===== Calculate raw_positive_support =====
                // Fraction of terminal cells whose raw vertical-pair maximum is finite and > 0
                let mut raw_positive_count = 0;
                let mut raw_total_pairs = 0;
                for y in (0..scene.density.height).step_by(2) {
                    for x in 0..scene.density.width {
                        let top = scene.density.get(x, y);
                        let bottom = if y + 1 < scene.density.height {
                            scene.density.get(x, y + 1)
                        } else {
                            0.0
                        };
                        let pair_max = top.max(bottom);
                        if pair_max.is_finite() && pair_max > 0.0 {
                            raw_positive_count += 1;
                        }
                        raw_total_pairs += 1;
                    }
                }
                let raw_positive_support = if raw_total_pairs == 0 {
                    0.0
                } else {
                    (raw_positive_count as f64) / (raw_total_pairs as f64)
                };

                // ===== Prepare density using production function =====
                let prepared = prepare_density(scene.density.clone(), profile);

                // Extract processed density and threshold
                let (density, threshold) = match prepared {
                    PreparedDensity::Shade { density, threshold } => (density, threshold),
                    PreparedDensity::Starfield { .. } => continue, // Skip Starfield
                };

                // ===== Calculate processed_positive_support =====
                // Fraction of terminal cells whose post-normalization/post-stretch pair max is finite and > 0
                let mut processed_positive_count = 0;
                let mut processed_total_pairs = 0;
                for y in (0..density.height).step_by(2) {
                    for x in 0..density.width {
                        let top = density.get(x, y);
                        let bottom = if y + 1 < density.height {
                            density.get(x, y + 1)
                        } else {
                            0.0
                        };
                        let pair_max = top.max(bottom);
                        if pair_max.is_finite() && pair_max > 0.0 {
                            processed_positive_count += 1;
                        }
                        processed_total_pairs += 1;
                    }
                }
                let processed_positive_support = if processed_total_pairs == 0 {
                    0.0
                } else {
                    (processed_positive_count as f64) / (processed_total_pairs as f64)
                };

                // ===== Calculate visible_occupancy =====
                // Fraction satisfying: finite; > 0; >= threshold
                let occupancy = calculate_occupancy(&density, threshold);

                // Determine if out of range
                let out_of_range = occupancy < min_occ || occupancy > max_occ;

                // Store measurement
                measurements.push(Measurement {
                    model,
                    seed: *seed,
                    target_occupancy: target_occ,
                    raw_positive_support,
                    processed_positive_support,
                    threshold,
                    visible_occupancy: occupancy,
                    out_of_range,
                });
            }
        }

        // Print complete diagnostic table
        println!("\n=== Canonical Occupancy Diagnostic Table ===");
        println!(
            "{:<10} {:<6} {:<10} {:<16} {:<20} {:<12} {:<16} {:<12} {}",
            "Model",
            "Seed",
            "Target",
            "Raw Support",
            "Processed Support",
            "Threshold",
            "Visible",
            "Status",
            "Notes"
        );
        println!(
            "{:<10} {:<6} {:<10} {:<16} {:<20} {:<12} {:<16} {:<12} {}",
            "", "", "(%)", "(%)", "(%)", "", "(%)", "", ""
        );
        println!("{}", "-".repeat(110));

        let mut out_of_range_count = 0;
        let mut zero_threshold_count = 0;
        let mut low_processed_support_count = 0;

        for m in &measurements {
            let status = if m.out_of_range {
                out_of_range_count += 1;
                "OUT_OF_RANGE"
            } else {
                "OK"
            };

            let mut notes = Vec::new();

            // Check for zero threshold
            if (m.threshold - 0.0).abs() < f64::EPSILON {
                zero_threshold_count += 1;
                notes.push("ZERO_THRESHOLD");
            }

            // Check if processed positive support is below approved minimum (20% for Spiral)
            // The approved minimum is the minimum expected occupancy for each model
            let approved_min = match m.model {
                ArtModel::Spiral => 0.20,
                ArtModel::Elliptical => 0.18,
                ArtModel::Cluster => 0.05,
                _ => 0.0,
            };

            if m.processed_positive_support < approved_min {
                low_processed_support_count += 1;
                notes.push("LOW_PROCESSED_SUPPORT");
            }

            // Check if robust normalization reduces positive support
            if m.processed_positive_support < m.raw_positive_support {
                notes.push("REDUCED_BY_NORMALIZATION");
            }

            let notes_str = if notes.is_empty() {
                "-".to_string()
            } else {
                notes.join(", ")
            };

            println!(
                "{:<10} {:<6} {:<10.2} {:<16.4} {:<20.4} {:<12.6} {:<16.4} {:<12} {}",
                format!("{:?}", m.model),
                m.seed,
                m.target_occupancy * 100.0,
                m.raw_positive_support * 100.0,
                m.processed_positive_support * 100.0,
                m.threshold,
                m.visible_occupancy * 100.0,
                status,
                notes_str
            );
        }

        println!("{}", "-".repeat(110));
        println!("\nSummary:");
        println!("  Total measurements: {}", measurements.len());
        println!("  Out of range: {}", out_of_range_count);
        println!("  Zero threshold: {}", zero_threshold_count);
        println!(
            "  Processed support below approved minimum: {}",
            low_processed_support_count
        );

        // Report models that can be tuned through threshold alone vs those requiring other changes
        println!("\nAnalysis:");
        println!("  Models that can be tuned through threshold alone:");
        println!("    (These have processed_positive_support >= approved minimum)");

        println!("  Models requiring generation-support, normalization, or requirements decision:");
        println!("    (These have processed_positive_support < approved minimum)");

        // Final aggregate assertion after all data is printed
        assert!(
            out_of_range_count == 0,
            "{} model/seed combinations produced occupancy outside expected ranges. See table above for details.",
            out_of_range_count
        );
    }
}
