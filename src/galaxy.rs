use crate::bar::BarConfig;
use crate::density::DensityMap;
use crate::seed::GenerationContext;
use noise::{NoiseFn, OpenSimplex};
use rand::rngs::StdRng;
use rand::RngExt;

const TAU: f64 = std::f64::consts::PI * 2.0;

/// Scale constant of the legacy logarithmic spiral: `r = a * exp(b * theta)`.
///
/// Kept as a named constant so the bar phase-alignment formula references the
/// same value as [`spiral_arm_density`]. Changing this would alter both the
/// legacy arm geometry and the bar-end alignment.
const SPIRAL_LOG_A: f64 = 0.075;

/// Fixed Ferrers exponent of the bar density profile: `density = strength * (1 - m2)^n`.
///
/// This is a SHAPE constant, not an RNG parameter. `n = 2` keeps the center
/// density, finite elliptical boundary, length, and width fixed; because
/// `n > 1`, the density and its first derivative both vanish continuously at
/// the boundary `m2 = 1`. Do not randomize this value.
const BAR_FERRERS_EXPONENT: f64 = 2.0;

/// Tunable parameters for the analytic spiral galaxy model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpiralGalaxyConfig {
    pub arms: usize,
    pub pitch: f64,
    pub inclination_rad: f64,
    pub rotation_rad: f64,
    pub bulge_sigma: f64,
    pub disk_scale: f64,
    pub arm_width: f64,
    pub arm_strength: f64,
    pub noise_scale: f64,
}

impl SpiralGalaxyConfig {
    /// Draws deterministic-looking physical parameters from the seeded RNG.
    pub fn from_rng(rng: &mut StdRng) -> Self {
        Self {
            arms: rng.random_range(2..=5),
            pitch: rng.random_range(0.42..0.70),
            inclination_rad: rng.random_range(0.70..1.05),
            rotation_rad: rng.random_range(0.0..TAU),
            bulge_sigma: rng.random_range(0.045..0.075),
            disk_scale: rng.random_range(0.45..0.62),
            arm_width: rng.random_range(0.018..0.034),
            arm_strength: rng.random_range(2.0..3.4),
            noise_scale: rng.random_range(3.5..6.0),
        }
    }
}

/// Generates a spiral galaxy using the legacy RNG-only entry point.
///
/// Production scene generation uses [`generate_spiral_galaxy_with_context`].
/// This wrapper remains for focused density tests that predate feature-specific
/// seed namespaces; no optional feature should depend on this context-free path.
#[cfg(test)]
pub fn generate_spiral_galaxy(
    terminal_width: usize,
    terminal_height: usize,
    rng: &mut StdRng,
) -> DensityMap {
    generate_spiral_galaxy_impl(terminal_width, terminal_height, rng, None)
}

/// Generates a spiral galaxy with explicit access to the base scene seed.
///
/// `GenerationContext` does not replace or advance the legacy RNG. It exists so
/// Phase 1 and later optional morphology can derive isolated feature streams
/// without perturbing the existing Spiral configuration or OpenSimplex seed.
pub fn generate_spiral_galaxy_with_context(
    terminal_width: usize,
    terminal_height: usize,
    rng: &mut StdRng,
    context: GenerationContext,
) -> DensityMap {
    generate_spiral_galaxy_impl(terminal_width, terminal_height, rng, Some(context))
}

/// Generates a spiral galaxy as a high-resolution density field.
///
/// `terminal_height` is the number of terminal text rows requested by the user.
/// The returned map has twice that height because the renderer consumes two
/// density rows per visible terminal row via half-block glyphs.
fn generate_spiral_galaxy_impl(
    terminal_width: usize,
    terminal_height: usize,
    rng: &mut StdRng,
    context: Option<GenerationContext>,
) -> DensityMap {
    let config = SpiralGalaxyConfig::from_rng(rng);

    // Derive the optional central bar exactly once per scene from the isolated
    // feature stream. `BarConfig::from_context` never advances the legacy RNG,
    // so the existing `SpiralGalaxyConfig` and `noise_seed` draws are preserved
    // bit-for-bit. When no context is supplied (the legacy test path) the bar is
    // absent and the density reduces to the original Spiral model.
    let bar = context.and_then(BarConfig::from_context);

    let out_width = terminal_width.max(1);
    let out_height = terminal_height.max(1) * 2;

    // Supersampling before binning. This is deliberately modest because this is
    // a CLI visual effect, not a scientific image pipeline.
    let sample = 3;
    let high_width = out_width * sample;
    let high_height = out_height * sample;

    let noise_seed = rng.random::<u32>();
    let coarse_noise = OpenSimplex::new(noise_seed);
    let fine_noise = OpenSimplex::new(noise_seed.wrapping_add(1));

    let high = DensityMap::from_fn(high_width, high_height, |sx, sy| {
        let x = normalized_coord(sx, high_width);
        let y = normalized_coord(sy, high_height);

        spiral_density(x, y, &config, bar, &coarse_noise, &fine_noise)
    });

    high.downsample_average(out_width, out_height)
}

fn normalized_coord(i: usize, n: usize) -> f64 {
    2.0 * ((i as f64 + 0.5) / n as f64 - 0.5)
}

fn spiral_density(
    x: f64,
    y: f64,
    config: &SpiralGalaxyConfig,
    bar: Option<BarConfig>,
    coarse_noise: &OpenSimplex,
    fine_noise: &OpenSimplex,
) -> f64 {
    // Half-block glyphs already double the vertical sampling. If this model is
    // later rendered in pure ASCII, increase this factor toward ~2.0.
    let y_aspect_corrected = y;

    // Sky-plane rotation.
    let cos_r = config.rotation_rad.cos();
    let sin_r = config.rotation_rad.sin();
    let xr = x * cos_r + y_aspect_corrected * sin_r;
    let yr = -x * sin_r + y_aspect_corrected * cos_r;

    // Simple inclined disk deprojection. After this point (xr, yd) are
    // intrinsic/deprojected disk coordinates, which is the frame in which the
    // bar is defined.
    let cos_i = config.inclination_rad.cos().abs().max(0.30);
    let yd = yr / cos_i;

    let r = (xr * xr + yd * yd).sqrt();
    if r > 1.20 {
        return 0.0;
    }

    let theta = yd.atan2(xr);

    let bulge = gaussian(r, config.bulge_sigma) * 0.30;
    let disk = (-r / config.disk_scale).exp() * 0.035;

    // The bar is evaluated in the same intrinsic disk plane as the arms. When
    // absent, the composition reduces exactly to the legacy Spiral model.
    let bar_term = bar.map(|b| bar_density(xr, yd, b)).unwrap_or(0.0);

    // When a bar is present, the arms are gated by a smooth radial transition
    // centered near the bar end so arm 0 does not run at full strength through
    // the nucleus. The phase of the logarithmic spiral is also shifted so arm 0
    // reaches the bar orientation at r = bar.half_length. Both adjustments are
    // no-ops when the bar is absent.
    let (arms, arm_gate_value) = if let Some(b) = bar {
        let gate = arm_gate(r, b);
        let arms = spiral_arm_density(r, theta, config, Some(b));
        (arms, gate)
    } else {
        let arms = spiral_arm_density(r, theta, config, None);
        (arms, 1.0)
    };

    let coarse =
        normalized_noise(coarse_noise.get([xr * config.noise_scale, yd * config.noise_scale]));

    let fine = normalized_noise(
        fine_noise.get([xr * config.noise_scale * 5.0, yd * config.noise_scale * 5.0]),
    );

    let clumpiness = 0.45 + 1.35 * coarse.powf(1.4);
    let stellar_knots = fine.powf(8.0) * arms * arm_gate_value * 0.85;

    // Intended composition:
    //   bulge + disk + bar + gated_arms * clumpiness + gated_stellar_knots
    // `stellar_knots` is suppressed by exactly the same radial arm gate as the
    // main spiral-arm contribution, so spiral-associated knots vanish inside
    // the gated nuclear/bar region. Mathematically invalid negative values are
    // clamped to zero.
    let density = bulge + disk + bar_term + arms * arm_gate_value * clumpiness + stellar_knots;
    density.max(0.0)
}

/// Ferrers-style bar density evaluated in the intrinsic disk plane.
///
/// The intrinsic disk coordinates are rotated into the bar frame
/// (`xb = xd * cos(angle) + yd * sin(angle)`,
/// `yb = -xd * sin(angle) + yd * cos(angle)`), then evaluated with the fixed
/// exponent `n = BAR_FERRERS_EXPONENT` (2.0) profile:
/// `density = strength * (1 - m2)^n` for `m2 < 1`, else `0`. The profile has
/// finite support: it is exactly zero at and beyond the elliptical boundary
/// `m2 = 1`, where `m2 = (xb/a)^2 + (yb/b)^2`, with `a = half_length` and
/// `b = half_width()`. Because `n > 1`, both `(1 - m2)^n` and its derivative
/// approach zero continuously as `m2 -> 1`, so the boundary is smooth rather
/// than a hard rectangular clip. The exponent is a shape constant, not an RNG
/// parameter: do not randomize it.
///
/// Parameters
/// ----------
/// xd : intrinsic disk x-coordinate (deprojected).
/// yd : intrinsic disk y-coordinate (deprojected, inclination-corrected).
/// bar : bar configuration carrying half-length, axis ratio, strength, angle.
///
/// Returns
/// -------
/// Non-negative bar density. Equals `bar.strength` at the center and `0` for
/// `m2 >= 1`.
fn bar_density(xd: f64, yd: f64, bar: BarConfig) -> f64 {
    let a = bar.half_length;
    let b = bar.half_width();

    // Guard against degenerate configurations. The v1 ranges keep both axes
    // positive, but the function must remain total and non-negative.
    if a <= 0.0 || b <= 0.0 {
        return 0.0;
    }

    let cos_a = bar.angle_rad.cos();
    let sin_a = bar.angle_rad.sin();
    let xb = xd * cos_a + yd * sin_a;
    let yb = -xd * sin_a + yd * cos_a;

    let m2 = (xb / a).powi(2) + (yb / b).powi(2);
    if m2 >= 1.0 {
        return 0.0;
    }

    let one_minus = 1.0 - m2;
    // Fixed Ferrers exponent (BAR_FERRERS_EXPONENT = 2.0) for v1. This is a
    // shape constant, not an RNG parameter: do not randomize it.
    bar.strength * one_minus.powf(BAR_FERRERS_EXPONENT)
}

/// Smooth radial gate for the spiral arms when a bar is present.
///
/// Returns `0` for `r <= inner`, `1` for `r >= outer`, and a smooth monotonic
/// cubic smoothstep between them. The boundaries are fixed morphology
/// constants derived from `bar.half_length`, not new RNG parameters.
///
/// Parameters
/// ----------
/// r : radius in the intrinsic disk plane.
/// bar : bar configuration (only `half_length` is used).
///
/// Returns
/// -------
/// Gate value in `[0, 1]`. Zero at/below the inner boundary, one at/above the
/// outer boundary, monotonically increasing through the transition.
fn arm_gate(r: f64, bar: BarConfig) -> f64 {
    let inner = 0.65 * bar.half_length;
    let outer = 1.05 * bar.half_length;

    if r <= inner {
        0.0
    } else if r >= outer {
        1.0
    } else {
        // Cubic smoothstep: 3t^2 - 2t^3, monotonic on [0, 1].
        let t = (r - inner) / (outer - inner);
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

/// Global phase offset for the logarithmic spiral so that, when a bar is
/// present, arm 0 reaches the bar orientation near `r = bar.half_length`.
///
/// The legacy spiral obeys `r = a * exp(b * theta)`, so the angle of arm 0 at
/// radius `r` is `theta_0(r) = ln(r / a) / b`. Aligning `theta_0(half_length)`
/// with `bar.angle_rad` requires shifting `base_theta` by
/// `bar.angle_rad - ln(half_length / a) / b`. The offset is a single
/// deterministic global value; no per-arm offsets or new RNG streams are
/// introduced.
fn bar_phase_offset(bar: BarConfig, pitch: f64) -> f64 {
    let a = SPIRAL_LOG_A;
    let base_theta_at_bar_end = (bar.half_length / a).max(1.0e-4).ln() / pitch;
    bar.angle_rad - base_theta_at_bar_end
}

fn spiral_arm_density(
    r: f64,
    theta: f64,
    config: &SpiralGalaxyConfig,
    bar: Option<BarConfig>,
) -> f64 {
    if r < 0.045 {
        return 0.0;
    }

    // Logarithmic spiral: r = a * exp(b * theta).
    // We invert it to compare the observed angle against the nearest arm angle.
    let a = SPIRAL_LOG_A;
    let b = config.pitch;
    let mut base_theta = (r / a).max(1.0e-4).ln() / b;

    // Apply a single global phase offset so arm 0 reaches the bar orientation
    // near r = bar.half_length. No-op when the bar is absent.
    if let Some(b_cfg) = bar {
        base_theta += bar_phase_offset(b_cfg, config.pitch);
    }

    let arm_spacing = TAU / config.arms as f64;
    let radial_fade = (-r / config.disk_scale).exp();

    let mut density = 0.0;

    for arm in 0..config.arms {
        let arm_theta = base_theta + arm as f64 * arm_spacing;
        let dtheta = angular_distance(theta, arm_theta);

        // Approximate angular separation as a physical transverse distance.
        let distance = r * dtheta.abs();
        let width = config.arm_width * (1.0 + 0.75 * r);

        density += gaussian(distance, width);
    }

    density * radial_fade * config.arm_strength
}

fn gaussian(x: f64, sigma: f64) -> f64 {
    (-0.5 * (x / sigma).powi(2)).exp()
}

fn angular_distance(a: f64, b: f64) -> f64 {
    let mut d = (a - b + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI;
    if d < -std::f64::consts::PI {
        d += TAU;
    }
    d
}

fn normalized_noise(value: f64) -> f64 {
    ((value + 1.0) * 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::{derive_feature_seed, SPIRAL_BAR_V1};
    use rand::{RngExt, SeedableRng};

    fn legacy_rng_checkpoint_signature(seed: u64) -> u64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let config = SpiralGalaxyConfig::from_rng(&mut rng);
        let noise_seed = rng.random::<u32>();

        let words = [
            config.arms as u64,
            config.pitch.to_bits(),
            config.inclination_rad.to_bits(),
            config.rotation_rad.to_bits(),
            config.bulge_sigma.to_bits(),
            config.disk_scale.to_bits(),
            config.arm_width.to_bits(),
            config.arm_strength.to_bits(),
            config.noise_scale.to_bits(),
            u64::from(noise_seed),
        ];

        let mut hash = 0xcbf29ce484222325_u64;
        for word in words {
            for byte in word.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
        hash
    }

    #[test]
    fn test_spiral_galaxy_is_deterministic() {
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);

        let map1 = generate_spiral_galaxy(30, 12, &mut rng1);
        let map2 = generate_spiral_galaxy(30, 12, &mut rng2);

        assert_eq!(map1, map2);
    }

    #[test]
    fn test_spiral_context_path_matches_legacy_path_for_unbarred_seed() {
        // Seed 1 is unbarred under `spiral/bar/v1`. For an unbarred scene the
        // contextual path must reduce bit-for-bit to the legacy/context-free
        // Spiral density, proving the bar integration is a pure no-op when no
        // bar is present.
        let seed = 1_u64;
        let mut legacy_rng = StdRng::seed_from_u64(seed);
        let mut contextual_rng = StdRng::seed_from_u64(seed);

        let legacy = generate_spiral_galaxy(30, 12, &mut legacy_rng);
        let contextual = generate_spiral_galaxy_with_context(
            30,
            12,
            &mut contextual_rng,
            GenerationContext::new(seed),
        );

        assert_eq!(contextual, legacy);
    }

    #[test]
    fn test_spiral_galaxy_uses_half_block_height() {
        let mut rng = StdRng::seed_from_u64(42);
        let map = generate_spiral_galaxy(30, 12, &mut rng);

        assert_eq!(map.width, 30);
        assert_eq!(map.height, 24);
        assert!(map.data.iter().any(|v| *v > 0.1));
    }

    #[test]
    fn test_spiral_arm_count_range_is_2_to_5() {
        let mut counts = [0usize; 6];
        for seed in 0..2000u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let config = SpiralGalaxyConfig::from_rng(&mut rng);
            assert!(
                (2..=5).contains(&config.arms),
                "seed {seed}: arms out of range"
            );
            counts[config.arms] += 1;
        }
        for (arms, &count) in counts.iter().enumerate().skip(2).take(4) {
            assert!(count > 0, "expected at least one seed with {arms} arms");
        }
    }

    #[test]
    fn test_spiral_fixed_seed_arm_count_anchors() {
        // Seed anchors for the current RNG stream: seed 4 has 4 arms,
        // seed 0 has 5 arms.
        let mut rng = StdRng::seed_from_u64(4);
        assert_eq!(SpiralGalaxyConfig::from_rng(&mut rng).arms, 4);
        let mut rng = StdRng::seed_from_u64(0);
        assert_eq!(SpiralGalaxyConfig::from_rng(&mut rng).arms, 5);
    }

    #[test]
    fn test_spiral_legacy_rng_checkpoint_anchors() {
        // Baseline captured from main at f036c2b230dc5a1faf6f9dcb2614b12d0e7726e8.
        // These signatures cover every legacy SpiralGalaxyConfig draw plus the
        // subsequent OpenSimplex seed draw. New feature RNGs must not perturb
        // this checkpoint.
        let actual = [
            (4_u64, legacy_rng_checkpoint_signature(4)),
            (16_u64, legacy_rng_checkpoint_signature(16)),
            (42_u64, legacy_rng_checkpoint_signature(42)),
        ];
        let expected = [
            (4_u64, 11016964189280910970_u64),
            (16_u64, 15594627238422693320_u64),
            (42_u64, 13280321653872101795_u64),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_spiral_feature_rng_does_not_advance_legacy_stream() {
        let seed = 42;

        let mut baseline_rng = StdRng::seed_from_u64(seed);
        let baseline_config = SpiralGalaxyConfig::from_rng(&mut baseline_rng);
        let baseline_noise_seed = baseline_rng.random::<u32>();

        let mut isolated_rng = StdRng::seed_from_u64(seed);
        let isolated_config = SpiralGalaxyConfig::from_rng(&mut isolated_rng);

        let bar_seed = derive_feature_seed(seed, SPIRAL_BAR_V1);
        let mut bar_rng = StdRng::seed_from_u64(bar_seed);
        for _ in 0..32 {
            let _ = bar_rng.random::<u64>();
        }

        let isolated_noise_seed = isolated_rng.random::<u32>();

        assert_eq!(isolated_config, baseline_config);
        assert_eq!(isolated_noise_seed, baseline_noise_seed);
    }

    #[test]
    fn test_spiral_4_and_5_arm_scenes_are_deterministic() {
        for seed in [0u64, 4] {
            let scene1 = crate::engine::ArtModel::Spiral.generate_scene(30, 15, Some(seed));
            let scene2 = crate::engine::ArtModel::Spiral.generate_scene(30, 15, Some(seed));
            assert_eq!(scene1.density, scene2.density);
        }
    }

    // ---- Phase 1B: barred-spiral morphology ----

    /// Sampled Ferrers bar density used by the unit tests below. The bar is
    /// aligned with the x-axis (`angle_rad = 0`) so the major/minor axes map
    /// cleanly onto `xd`/`yd`.
    fn sample_bar() -> BarConfig {
        BarConfig {
            half_length: 0.20,
            axis_ratio: 5.0,
            strength: 0.18,
            angle_rad: 0.0,
        }
    }

    #[test]
    fn test_ferrers_bar_center_reaches_strength() {
        let bar = sample_bar();
        // The origin is the bar center in intrinsic disk coordinates.
        let d = bar_density(0.0, 0.0, bar);
        assert!((d - bar.strength).abs() < 1.0e-12);
    }

    #[test]
    fn test_ferrers_bar_zero_outside_boundary() {
        let bar = sample_bar();
        let a = bar.half_length;
        let b = bar.half_width();

        // Along the major axis, exactly at and beyond the boundary.
        assert_eq!(bar_density(a, 0.0, bar), 0.0);
        assert_eq!(bar_density(a * 1.001, 0.0, bar), 0.0);
        assert_eq!(bar_density(-a, 0.0, bar), 0.0);

        // Along the minor axis, exactly at and beyond the boundary.
        assert_eq!(bar_density(0.0, b, bar), 0.0);
        assert_eq!(bar_density(0.0, b * 1.001, bar), 0.0);
        assert_eq!(bar_density(0.0, -b, bar), 0.0);

        // A corner point well outside the elliptical boundary.
        assert_eq!(bar_density(a, b, bar), 0.0);
    }

    #[test]
    fn test_ferrers_bar_symmetric_about_both_axes() {
        let bar = sample_bar();
        let a = bar.half_length * 0.5;
        let b = bar.half_width() * 0.5;

        let pp = bar_density(a, b, bar);
        let pn = bar_density(a, -b, bar);
        let np = bar_density(-a, b, bar);
        let nn = bar_density(-a, -b, bar);

        assert!((pp - pn).abs() < 1.0e-12);
        assert!((pp - np).abs() < 1.0e-12);
        assert!((pp - nn).abs() < 1.0e-12);
        assert!(pp > 0.0);
    }

    #[test]
    fn test_ferrers_bar_approaches_zero_at_boundary() {
        let bar = sample_bar();
        let a = bar.half_length;

        // Just inside the boundary along the major axis: density should be
        // small and positive, and tend to zero as we approach `a`.
        let just_inside = a * (1.0 - 1.0e-3);
        let d = bar_density(just_inside, 0.0, bar);
        assert!(d > 0.0);
        assert!(d < bar.strength * 1.0e-2);

        // Monotonic decrease along the major axis from center to boundary.
        let mut prev = bar.strength;
        let steps = 32;
        for i in 1..=steps {
            let r = a * (i as f64) / (steps as f64);
            let d = bar_density(r, 0.0, bar);
            assert!(d <= prev + 1.0e-12, "expected monotonic decrease");
            prev = d;
        }
        assert_eq!(prev, 0.0);
    }

    #[test]
    fn test_ferrers_bar_never_negative() {
        let bar = sample_bar();
        let a = bar.half_length;
        let b = bar.half_width();

        for i in -10..=10_i32 {
            for j in -10..=10_i32 {
                let xd = (i as f64) * a * 1.5 / 10.0;
                let yd = (j as f64) * b * 1.5 / 10.0;
                let d = bar_density(xd, yd, bar);
                assert!(d >= 0.0, "negative bar density at ({xd}, {yd}): {d}");
            }
        }
    }

    #[test]
    fn test_ferrers_bar_boundary_is_c1_smooth() {
        // The Ferrers profile (n = 2) must remain C^1-smooth at the finite
        // elliptical boundary: both the density value and its radial slope
        // must approach zero as m2 -> 1 from below. This is what keeps the
        // boundary visually smooth (no hard rectangular clip).
        let bar = sample_bar();
        let a = bar.half_length;

        // Radii approaching the boundary from below along the major axis.
        let radii: Vec<f64> = [1.0e-1, 5.0e-2, 2.5e-2, 1.0e-2, 5.0e-3, 2.0e-3, 1.0e-3]
            .iter()
            .map(|eps| a * (1.0 - eps))
            .collect();

        // (1) Value continuity: density decreases monotonically toward zero.
        let mut prev_d = f64::INFINITY;
        for &r in &radii {
            let d = bar_density(r, 0.0, bar);
            assert!(d >= 0.0);
            assert!(d < prev_d, "density must decrease toward the boundary");
            prev_d = d;
        }
        assert!(
            prev_d < bar.strength * 1.0e-2,
            "density must be near zero at the boundary: {prev_d}"
        );

        // (2) Slope continuity: the magnitude of the radial slope must shrink
        // as we approach the boundary (finite-difference estimate of
        // d(density)/dr). For a C^1 profile whose derivative vanishes at the
        // boundary, these magnitudes decrease monotonically here.
        let mut prev_slope_mag = f64::INFINITY;
        for w in radii.windows(2) {
            let (r0, r1) = (w[0], w[1]);
            let d0 = bar_density(r0, 0.0, bar);
            let d1 = bar_density(r1, 0.0, bar);
            let slope = (d1 - d0) / (r1 - r0);
            assert!(slope < 0.0, "density must decrease outward");
            assert!(
                slope.abs() < prev_slope_mag,
                "slope magnitude must shrink toward the boundary (C^1 smoothness)"
            );
            prev_slope_mag = slope.abs();
        }
    }

    #[test]
    fn test_arm_gate_zero_at_and_below_inner() {
        let bar = sample_bar();
        let inner = 0.65 * bar.half_length;

        assert_eq!(arm_gate(0.0, bar), 0.0);
        assert_eq!(arm_gate(inner, bar), 0.0);
        assert_eq!(arm_gate(inner * 0.5, bar), 0.0);
    }

    #[test]
    fn test_arm_gate_one_at_and_above_outer() {
        let bar = sample_bar();
        let outer = 1.05 * bar.half_length;

        assert_eq!(arm_gate(outer, bar), 1.0);
        assert_eq!(arm_gate(outer * 1.5, bar), 1.0);
    }

    #[test]
    fn test_arm_gate_bounded_and_monotonic() {
        let bar = sample_bar();
        let inner = 0.65 * bar.half_length;
        let outer = 1.05 * bar.half_length;
        let steps = 64;

        let mut prev = 0.0;
        let mut prev_r = inner;
        for i in 0..=steps {
            let r = inner + (outer - inner) * (i as f64) / (steps as f64);
            let g = arm_gate(r, bar);
            assert!((0.0..=1.0).contains(&g), "gate out of [0,1]: {g}");
            assert!(g >= prev - 1.0e-12, "gate not monotonic at r={r}");
            prev = g;
            prev_r = r;
        }
        assert_eq!(arm_gate(inner, bar), 0.0);
        assert_eq!(arm_gate(outer, bar), 1.0);
        // Exercise the strict interior so the smoothstep branch is covered.
        let mid = (inner + outer) * 0.5;
        let g_mid = arm_gate(mid, bar);
        assert!(g_mid > 0.0 && g_mid < 1.0);
        let _ = prev_r;
    }

    #[test]
    fn test_barred_scene_is_deterministic_across_runs() {
        // Seed 42 is barred under spiral/bar/v1.
        let seed = 42_u64;
        let a = crate::engine::ArtModel::Spiral.generate_scene(40, 20, Some(seed));
        let b = crate::engine::ArtModel::Spiral.generate_scene(40, 20, Some(seed));
        assert_eq!(a.density, b.density);
    }

    #[test]
    fn test_barred_scene_legacy_rng_checkpoint_unchanged() {
        // The legacy RNG checkpoint must remain unchanged for a barred seed.
        assert_eq!(
            legacy_rng_checkpoint_signature(42),
            13280321653872101795_u64
        );
        assert_eq!(
            legacy_rng_checkpoint_signature(16),
            15594627238422693320_u64
        );
    }

    #[test]
    fn test_barred_seed_differs_from_context_free_density() {
        // Seed 42 is barred: the contextual (production) density must differ
        // from the legacy/context-free density.
        let seed = 42_u64;
        let mut legacy_rng = StdRng::seed_from_u64(seed);
        let legacy = generate_spiral_galaxy(40, 20, &mut legacy_rng);

        let mut ctx_rng = StdRng::seed_from_u64(seed);
        let contextual =
            generate_spiral_galaxy_with_context(40, 20, &mut ctx_rng, GenerationContext::new(seed));

        assert_ne!(contextual, legacy);
    }

    #[test]
    fn test_barred_seed_central_support_is_present() {
        // The bulge alone guarantees positive central density, so asserting a
        // positive central value is tautological. Instead, compare the barred
        // and legacy/context-free density for the same known barred seed (42)
        // at a point on the bar major axis inside the arm inner cutoff
        // (r < 0.045): there the arms and knots are exactly zero in both
        // densities, the bulge and disk terms are identical, and the only
        // difference is the bar term. The assertions below fail if the bar
        // were accidentally removed.
        let seed = 42_u64;
        let mut rng = StdRng::seed_from_u64(seed);
        let config = SpiralGalaxyConfig::from_rng(&mut rng);
        let noise_seed = rng.random::<u32>();
        let coarse_noise = OpenSimplex::new(noise_seed);
        let fine_noise = OpenSimplex::new(noise_seed.wrapping_add(1));

        let bar = BarConfig::from_context(GenerationContext::new(seed))
            .expect("seed 42 is barred under spiral/bar/v1");

        // A point on the bar major axis, inside the arm inner cutoff (0.045)
        // and well inside the bar support (half_length >= 0.14).
        let r0 = 0.04;
        let xr = r0 * bar.angle_rad.cos();
        let yd = r0 * bar.angle_rad.sin();

        // Map the intrinsic disk point back to the sky plane (inverse of the
        // rotation + inclination deprojection applied in `spiral_density`).
        let cos_i = config.inclination_rad.cos().abs().max(0.30);
        let yr = yd * cos_i;
        let cos_r = config.rotation_rad.cos();
        let sin_r = config.rotation_rad.sin();
        let x = xr * cos_r - yr * sin_r;
        let y = xr * sin_r + yr * cos_r;

        let barred = spiral_density(x, y, &config, Some(bar), &coarse_noise, &fine_noise);
        let legacy = spiral_density(x, y, &config, None, &coarse_noise, &fine_noise);
        let bar_term = bar_density(xr, yd, bar);

        assert!(bar_term > 0.0, "test point must lie inside the bar support");
        assert!(
            barred > legacy,
            "bar must add central density: barred={barred} legacy={legacy}"
        );
        assert!(
            (barred - legacy - bar_term).abs() < 1.0e-12,
            "barred-legacy difference must be exactly the bar term: barred={barred} legacy={legacy} bar_term={bar_term}"
        );
    }

    #[test]
    fn test_zero_arm_gate_removes_stellar_knots() {
        // A point inside the gated nuclear region (r <= 0.65 * half_length)
        // must carry no spiral-associated structure at all: the gated arm
        // contribution and the stellar knots both vanish, leaving only bulge,
        // disk, and bar. This test fails if the knots are computed from the
        // ungated arm value.
        //
        // Rotation and inclination are zero so sky-plane coordinates equal
        // intrinsic disk coordinates.
        let config = SpiralGalaxyConfig {
            arms: 2,
            pitch: 0.55,
            inclination_rad: 0.0,
            rotation_rad: 0.0,
            bulge_sigma: 0.06,
            disk_scale: 0.5,
            arm_width: 0.025,
            arm_strength: 2.5,
            noise_scale: 4.0,
        };
        let bar = sample_bar();
        let noise_seed = 7_u32;
        let coarse_noise = OpenSimplex::new(noise_seed);
        let fine_noise = OpenSimplex::new(noise_seed.wrapping_add(1));

        // r0 is above the arm inner cutoff (0.045) so the ungated arms are
        // non-zero near the arm centerlines, and below the gate inner boundary
        // (0.65 * 0.20 = 0.13) so the arm gate is exactly zero here.
        let r0 = 0.10;

        // Find a sample where the ungated arms and the fine noise are both
        // non-negligible, so the ungated knot term would be observable.
        let mut best = None;
        for i in 0..720 {
            let theta = TAU * (i as f64) / 720.0;
            let x = r0 * theta.cos();
            let y = r0 * theta.sin();
            let arms_ungated = spiral_arm_density(r0, theta, &config, Some(bar));
            let fine = normalized_noise(
                fine_noise.get([x * config.noise_scale * 5.0, y * config.noise_scale * 5.0]),
            );
            let knot = fine.powf(8.0) * arms_ungated * 0.85;
            if arms_ungated > 0.1 && knot > 1.0e-3 && best.is_none_or(|(bk, _, _)| knot > bk) {
                best = Some((knot, x, y));
            }
        }
        let (knot, x, y) = best.expect("no sample with ungated arms and non-zero fine noise");

        let full = spiral_density(x, y, &config, Some(bar), &coarse_noise, &fine_noise);
        let r = (x * x + y * y).sqrt();
        let reference = gaussian(r, config.bulge_sigma) * 0.30
            + (-r / config.disk_scale).exp() * 0.035
            + bar_density(x, y, bar);

        assert!(
            knot > 1.0e-3,
            "knot term too small to guard the gate: {knot}"
        );
        assert!(
            (full - reference).abs() < 1.0e-12,
            "zero arm gate must remove both arms and knots: full={full} reference={reference}"
        );
    }

    #[test]
    fn test_unbarred_seed_matches_legacy_density_exactly() {
        // Seed 1 is unbarred. The production (contextual) density must match
        // the legacy/context-free density bit-for-bit.
        let seed = 1_u64;
        let mut legacy_rng = StdRng::seed_from_u64(seed);
        let legacy = generate_spiral_galaxy(40, 20, &mut legacy_rng);

        let mut ctx_rng = StdRng::seed_from_u64(seed);
        let contextual =
            generate_spiral_galaxy_with_context(40, 20, &mut ctx_rng, GenerationContext::new(seed));

        assert_eq!(contextual, legacy);
    }
}
