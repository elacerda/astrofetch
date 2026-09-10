use crate::bar::BarConfig;
use crate::density::DensityMap;
use crate::dust::DustLaneConfig;
use crate::render::topology::CellSamplingShape;
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
///
/// The sampling shape is fixed to `CellSamplingShape::HALF_BLOCK`, the
/// production topology.
#[cfg(test)]
pub fn generate_spiral_galaxy(
    terminal_width: usize,
    terminal_height: usize,
    rng: &mut StdRng,
) -> DensityMap {
    generate_spiral_galaxy_impl(
        terminal_width,
        terminal_height,
        rng,
        None,
        CellSamplingShape::HALF_BLOCK,
    )
}

/// Generates a spiral galaxy with explicit access to the base scene seed.
///
/// `GenerationContext` does not replace or advance the legacy RNG. It exists so
/// Phase 1 and later optional morphology can derive isolated feature streams
/// without perturbing the existing Spiral configuration or OpenSimplex seed.
///
/// The sampling shape is fixed to `CellSamplingShape::HALF_BLOCK`, the
/// production topology; shape-aware generation is available through
/// [`generate_spiral_galaxy_with_shape`].
pub fn generate_spiral_galaxy_with_context(
    terminal_width: usize,
    terminal_height: usize,
    rng: &mut StdRng,
    context: GenerationContext,
) -> DensityMap {
    generate_spiral_galaxy_impl(
        terminal_width,
        terminal_height,
        rng,
        Some(context),
        CellSamplingShape::HALF_BLOCK,
    )
}

/// Generates a spiral galaxy with an explicit cell sampling shape.
///
/// Internal shape-aware entry point (Phase 5A). The production path always
/// uses `CellSamplingShape::HALF_BLOCK`; this function exists so tests can
/// exercise `CellSamplingShape::QUADRANT` directly. The shape affects only
/// the sampling dimensions: for a fixed seed the RNG stream (Spiral
/// configuration, bar, dust, and noise draws) is identical regardless of
/// shape, and the output is deterministic for a given seed, terminal size,
/// and shape.
///
/// Currently exercised only by tests: the production scene path selects
/// `HALF_BLOCK` via [`generate_spiral_galaxy_with_context`]. The quadrant
/// renderer (Phase 5B) will select `QUADRANT` here.
#[cfg_attr(not(test), allow(dead_code))]
pub fn generate_spiral_galaxy_with_shape(
    terminal_width: usize,
    terminal_height: usize,
    rng: &mut StdRng,
    context: GenerationContext,
    shape: CellSamplingShape,
) -> DensityMap {
    generate_spiral_galaxy_impl(terminal_width, terminal_height, rng, Some(context), shape)
}

/// Fixed sampling geometry of the Spiral density pipeline.
///
/// The terminal requests `terminal_width × terminal_height` cells. The
/// logical density field is `max(W,1) × shape.columns()` by
/// `max(H,1) × shape.rows()`: the production `HALF_BLOCK` shape (1×2)
/// gives the legacy `W × 2H` field because the renderer consumes two
/// density rows per visible terminal row via half-block glyphs (1 logical
/// sample per terminal cell horizontally, 2 vertically), while the
/// `QUADRANT` shape (2×2) gives `2W × 2H` for the future quadrant
/// renderer. The density is evaluated on a supersampled grid of
/// `SUPERSAMPLE_FACTOR × SUPERSAMPLE_FACTOR` high-resolution samples per
/// logical cell and reduced back to the logical dimensions by averaging.
///
/// Phase 3 made this geometry explicit without changing it; Phase 5A makes
/// it shape-aware. The production path always uses
/// `CellSamplingShape::HALF_BLOCK`, so the derived integer dimensions and
/// the normalized-coordinate expression remain a bit-for-bit contract with
/// the pre-geometry pipeline. The supersample factor is deliberately modest
/// because this is a CLI visual effect, not a scientific image pipeline,
/// and the topology is intentionally non-configurable from the CLI.
struct SamplingGeometry {
    terminal_width: usize,
    terminal_height: usize,
    shape: CellSamplingShape,
}

impl SamplingGeometry {
    /// Isotropic supersampling factor applied to the logical field.
    const SUPERSAMPLE_FACTOR: usize = 3;

    /// Creates the geometry for a terminal of the given size and sampling
    /// shape.
    fn for_terminal(
        terminal_width: usize,
        terminal_height: usize,
        shape: CellSamplingShape,
    ) -> Self {
        Self {
            terminal_width,
            terminal_height,
            shape,
        }
    }

    /// Logical density width: `max(terminal_width, 1) * shape.columns()`.
    fn logical_width(&self) -> usize {
        self.terminal_width.max(1) * self.shape.columns()
    }

    /// Logical density height: `max(terminal_height, 1) * shape.rows()`.
    fn logical_height(&self) -> usize {
        self.terminal_height.max(1) * self.shape.rows()
    }

    /// High-resolution (supersampled) width: `logical_width * 3`.
    fn high_width(&self) -> usize {
        self.logical_width() * Self::SUPERSAMPLE_FACTOR
    }

    /// High-resolution (supersampled) height: `logical_height * 3`.
    fn high_height(&self) -> usize {
        self.logical_height() * Self::SUPERSAMPLE_FACTOR
    }

    /// Normalized x coordinate of high-resolution sample `sample_x`.
    ///
    /// Evaluates the exact legacy expression
    /// `2.0 * ((i as f64 + 0.5) / n as f64 - 0.5)` with
    /// `n = high_width()`. The expression must not be algebraically
    /// rewritten: anchored outputs are bit-for-bit contracts on this
    /// evaluation order.
    fn normalized_x(&self, sample_x: usize) -> f64 {
        normalized_coord(sample_x, self.high_width())
    }

    /// Normalized y coordinate of high-resolution sample `sample_y`.
    ///
    /// Evaluates the exact legacy expression
    /// `2.0 * ((i as f64 + 0.5) / n as f64 - 0.5)` with
    /// `n = high_height()`. The expression must not be algebraically
    /// rewritten: anchored outputs are bit-for-bit contracts on this
    /// evaluation order.
    fn normalized_y(&self, sample_y: usize) -> f64 {
        normalized_coord(sample_y, self.high_height())
    }
}

/// Generates a spiral galaxy as a density field at the shape's logical
/// dimensions.
///
/// `terminal_width`/`terminal_height` are the terminal cells requested by
/// the user. The returned map has `max(W,1) * shape.columns()` by
/// `max(H,1) * shape.rows()` dimensions: for the production `HALF_BLOCK`
/// shape that is the legacy `W × 2H` field (the renderer consumes two
/// density rows per visible terminal row via half-block glyphs).
fn generate_spiral_galaxy_impl(
    terminal_width: usize,
    terminal_height: usize,
    rng: &mut StdRng,
    context: Option<GenerationContext>,
    shape: CellSamplingShape,
) -> DensityMap {
    let config = SpiralGalaxyConfig::from_rng(rng);

    // Derive the optional central bar exactly once per scene from the isolated
    // feature stream. `BarConfig::from_context` never advances the legacy RNG,
    // so the existing `SpiralGalaxyConfig` and `noise_seed` draws are preserved
    // bit-for-bit.
    let bar = context.and_then(BarConfig::from_context);

    // Derive the optional dust lanes exactly once per scene from the isolated
    // dust feature stream, independently of the bar stream.
    // `DustLaneConfig::from_context` never advances the legacy RNG, so the
    // existing `SpiralGalaxyConfig` and `noise_seed` draws are preserved
    // bit-for-bit. When no context is supplied (the legacy test path) the dust
    // is absent and the density reduces exactly to the pre-dust Spiral model.
    let dust = context.and_then(DustLaneConfig::from_context);

    // The sampling geometry owns the dimension chain:
    //   terminal W×H
    //     -> logical max(W,1)*shape.columns() × max(H,1)*shape.rows()
    //     -> supersampled logical_width*3 × logical_height*3
    //     -> average reduction back to the logical dimensions.
    // The production shape is HALF_BLOCK (1×2), which reproduces the legacy
    // W×2H logical field and 3W×6H supersampled field bit-for-bit.
    let geometry = SamplingGeometry::for_terminal(terminal_width, terminal_height, shape);

    let noise_seed = rng.random::<u32>();
    let coarse_noise = OpenSimplex::new(noise_seed);
    let fine_noise = OpenSimplex::new(noise_seed.wrapping_add(1));

    let high = DensityMap::from_fn(geometry.high_width(), geometry.high_height(), |sx, sy| {
        let x = geometry.normalized_x(sx);
        let y = geometry.normalized_y(sy);

        spiral_density(x, y, &config, bar, dust, &coarse_noise, &fine_noise)
    });

    high.downsample_average(geometry.logical_width(), geometry.logical_height())
}

fn normalized_coord(i: usize, n: usize) -> f64 {
    2.0 * ((i as f64 + 0.5) / n as f64 - 0.5)
}

fn spiral_density(
    x: f64,
    y: f64,
    config: &SpiralGalaxyConfig,
    bar: Option<BarConfig>,
    dust: Option<DustLaneConfig>,
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

    let density = match dust {
        // Dustless scenes keep the exact Phase 2A evaluation order. Do not
        // replace this branch with the mathematically equivalent
        // `extinction = 1` formulation: the explicit branch preserves the
        // floating-point evaluation order and bit-identical output.
        None => {
            // Intended composition:
            //   bulge + disk + bar + gated_arms * clumpiness + gated_stellar_knots
            // `stellar_knots` is suppressed by exactly the same radial arm gate
            // as the main spiral-arm contribution, so spiral-associated knots
            // vanish inside the gated nuclear/bar region.
            bulge + disk + bar_term + arms * arm_gate_value * clumpiness + stellar_knots
        }
        Some(dust) => {
            // Dusty composition: dust attenuates only the luminous disk
            // (disk + gated arms * clumpiness). The bulge, bar term, and
            // stellar knots are NOT attenuated: this is a deliberate
            // first-version composition choice for procedural terminal art,
            // not physical radiative-transfer behavior.
            let arm_clump = arms * arm_gate_value * clumpiness;
            let luminous_disk = disk + arm_clump;
            let tau = dust.strength
                * dust_lane_profile(r, theta, config, bar, &dust)
                * dust_radial_gate(r, bar, config);
            let extinction = (-tau).exp();
            bulge + bar_term + luminous_disk * extinction + stellar_knots
        }
    };

    // Mathematically invalid negative values are clamped to zero.
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
    smoothstep_gate(r, inner, outer)
}

/// Cubic smoothstep radial gate.
///
/// Returns `0` for `r <= inner`, `1` for `r >= outer`, and the cubic
/// smoothstep `t * t * (3 - 2 * t)` between them, with
/// `t = clamp((r - inner) / (outer - inner), 0, 1)`. This is the exact
/// arithmetic sequence previously inlined in `arm_gate`; callers must not
/// algebraically rewrite it, because anchored outputs are bit-for-bit
/// contracts on this evaluation order.
///
/// Parameters
/// ----------
/// r : radius in the intrinsic disk plane (dimensionless scene units).
/// inner : inner boundary; the gate is exactly `0` at and below it.
/// outer : outer boundary; the gate is exactly `1` at and above it.
///
/// Returns
/// -------
/// Gate value in `[0, 1]`, monotonically increasing through the transition.
fn smoothstep_gate(r: f64, inner: f64, outer: f64) -> f64 {
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

/// Radial gate for the dust-lane extinction.
///
/// Barred scenes reuse the exact spiral-arm gate (`arm_gate`), so dust fades
/// in over the same radial transition as the arms. Unbarred scenes use the
/// fixed first-version morphology relation
/// `inner = bulge_sigma`, `outer = 2 * bulge_sigma` with the same cubic
/// smoothstep. These multiples are a fixed morphology relation, not an RNG
/// parameter and not an empirically calibrated astrophysical law.
///
/// Parameters
/// ----------
/// r : radius in the intrinsic disk plane (dimensionless scene units).
/// bar : optional bar configuration (only `half_length` is used).
/// config : spiral configuration (uses `bulge_sigma` when unbarred).
///
/// Returns
/// -------
/// Gate value in `[0, 1]`.
fn dust_radial_gate(r: f64, bar: Option<BarConfig>, config: &SpiralGalaxyConfig) -> f64 {
    match bar {
        Some(bar) => arm_gate(r, bar),
        None => smoothstep_gate(r, config.bulge_sigma, 2.0 * config.bulge_sigma),
    }
}

/// Max-over-arms Gaussian profile of the dust lanes at `(r, theta)`.
///
/// The dust lanes are a signed phase-offset copy of the stellar-arm geometry:
/// each dust ridge sits at
/// `spiral_base_theta(r, config, bar) + arm * (TAU / arms) + dust.offset`,
/// evaluated in the same intrinsic/deprojected disk frame as the stellar
/// arms (including the bar phase alignment when a bar is present). Each arm
/// contributes
/// `gaussian(r * |angular_distance(theta, dust_arm_theta)|, width)` with
/// `width = local_arm_width(r, config) * dust.width_factor`, and the arms are
/// combined with MAX (not SUM) so overlapping lanes do not stack.
///
/// `dust.offset` is applied exactly once, per arm ridge. There is no
/// independent spiral math, no per-arm random phase, no per-pixel RNG, and no
/// chirality semantics: the offset is a signed phase shift only. There is no
/// nuclear cutoff here; nuclear suppression belongs to `dust_radial_gate`.
///
/// Parameters
/// ----------
/// r : radius in the intrinsic disk plane (dimensionless scene units).
/// theta : angle in the intrinsic disk plane (radians, `atan2` convention).
/// config : spiral configuration (uses `arms`, `pitch`, `arm_width`).
/// bar : optional bar configuration (bar phase alignment is inherited).
/// dust : dust-lane configuration (uses `offset`, `width_factor`).
///
/// Returns
/// -------
/// Profile value in `[0, 1]`: finite, non-negative, at most 1 (it reaches 1
/// exactly on a dust lane ridge).
fn dust_lane_profile(
    r: f64,
    theta: f64,
    config: &SpiralGalaxyConfig,
    bar: Option<BarConfig>,
    dust: &DustLaneConfig,
) -> f64 {
    let base_theta = spiral_base_theta(r, config, bar);
    let arm_spacing = TAU / config.arms as f64;
    let width = local_arm_width(r, config) * dust.width_factor;

    let mut profile: f64 = 0.0;
    for arm in 0..config.arms {
        let dust_arm_theta = base_theta + arm as f64 * arm_spacing + dust.offset;
        let dtheta = angular_distance(theta, dust_arm_theta);

        // Approximate angular separation as a physical transverse distance.
        let distance = r * dtheta.abs();
        let profile_arm = gaussian(distance, width);

        profile = profile.max(profile_arm);
    }

    profile
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

/// Base angle of arm 0 of the logarithmic spiral at radius `r`.
///
/// The legacy spiral obeys `r = a * exp(b * theta)`, so the angle of arm 0 at
/// radius `r` is `ln(r / a) / b`, with `a = SPIRAL_LOG_A` and `b = pitch`.
/// When a bar is present, a single global phase offset
/// (`bar_phase_offset`) is added so arm 0 reaches the bar orientation near
/// `r = bar.half_length`; the offset is a no-op when the bar is absent.
///
/// This performs exactly the arithmetic previously inlined in
/// `spiral_arm_density`; the dust-lane profile reuses it so stellar-arm and
/// dust geometry cannot diverge.
///
/// Parameters
/// ----------
/// r : radius in the intrinsic disk plane (dimensionless scene units).
/// config : spiral configuration (uses `pitch`).
/// bar : optional bar configuration (uses `half_length` and `angle_rad`).
///
/// Returns
/// -------
/// Base angle of arm 0 in radians (unwrapped; arm `k` adds `k * TAU / arms`).
fn spiral_base_theta(r: f64, config: &SpiralGalaxyConfig, bar: Option<BarConfig>) -> f64 {
    let mut base_theta = (r / SPIRAL_LOG_A).max(1.0e-4).ln() / config.pitch;

    if let Some(bar) = bar {
        base_theta += bar_phase_offset(bar, config.pitch);
    }

    base_theta
}

/// Local transverse width of a spiral arm at radius `r`.
///
/// The width grows linearly with radius: `arm_width * (1 + 0.75 * r)`. This is
/// the exact expression previously inlined in `spiral_arm_density`; the
/// dust-lane profile reuses it (scaled by `DustLaneConfig::width_factor`) so
/// both geometries share one definition.
///
/// Parameters
/// ----------
/// r : radius in the intrinsic disk plane (dimensionless scene units).
/// config : spiral configuration (uses `arm_width`).
///
/// Returns
/// -------
/// Positive transverse width in the same dimensionless units as `r`.
fn local_arm_width(r: f64, config: &SpiralGalaxyConfig) -> f64 {
    config.arm_width * (1.0 + 0.75 * r)
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
    let base_theta = spiral_base_theta(r, config, bar);

    let arm_spacing = TAU / config.arms as f64;
    let radial_fade = (-r / config.disk_scale).exp();

    let mut density = 0.0;

    for arm in 0..config.arms {
        let arm_theta = base_theta + arm as f64 * arm_spacing;
        let dtheta = angular_distance(theta, arm_theta);

        // Approximate angular separation as a physical transverse distance.
        let distance = r * dtheta.abs();
        let width = local_arm_width(r, config);

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

        let barred = spiral_density(x, y, &config, Some(bar), None, &coarse_noise, &fine_noise);
        let legacy = spiral_density(x, y, &config, None, None, &coarse_noise, &fine_noise);
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

        let full = spiral_density(x, y, &config, Some(bar), None, &coarse_noise, &fine_noise);
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
        // Seed 1 is unbarred and dustless. The production (contextual)
        // density must match the legacy/context-free density bit-for-bit,
        // proving both the bar and dust integrations are pure no-ops when
        // absent.
        let seed = 1_u64;
        let mut legacy_rng = StdRng::seed_from_u64(seed);
        let legacy = generate_spiral_galaxy(40, 20, &mut legacy_rng);

        let mut ctx_rng = StdRng::seed_from_u64(seed);
        let contextual =
            generate_spiral_galaxy_with_context(40, 20, &mut ctx_rng, GenerationContext::new(seed));

        assert_eq!(contextual, legacy);
    }

    // ---- Phase 2B: dust-lane geometry and extinction ----

    /// Sampled dust-lane configuration used by the unit tests below.
    fn sample_dust() -> DustLaneConfig {
        DustLaneConfig {
            strength: 0.40,
            offset: 0.10,
            width_factor: 0.8,
        }
    }

    /// Sampled unbarred spiral configuration used by the unit tests below.
    /// Rotation and inclination are zero so sky-plane coordinates equal
    /// intrinsic disk coordinates.
    fn sample_spiral_config() -> SpiralGalaxyConfig {
        SpiralGalaxyConfig {
            arms: 2,
            pitch: 0.55,
            inclination_rad: 0.0,
            rotation_rad: 0.0,
            bulge_sigma: 0.06,
            disk_scale: 0.5,
            arm_width: 0.025,
            arm_strength: 2.5,
            noise_scale: 4.0,
        }
    }

    #[test]
    fn test_dust_lane_profile_is_finite_bounded_and_non_negative() {
        let config = sample_spiral_config();
        let dust = sample_dust();

        for bar in [None, Some(sample_bar())] {
            for r in [0.05, 0.1, 0.2, 0.5, 1.0] {
                for i in 0..360 {
                    let theta = TAU * (i as f64) / 360.0;
                    let p = dust_lane_profile(r, theta, &config, bar, &dust);
                    assert!(p.is_finite(), "profile not finite at r={r} theta={theta}");
                    assert!(p >= 0.0, "profile negative at r={r} theta={theta}");
                    assert!(p <= 1.0, "profile above 1 at r={r} theta={theta}");
                }
            }
        }
    }

    #[test]
    fn test_dust_lane_profile_reaches_one_on_offset_lane() {
        let config = sample_spiral_config();
        let dust = sample_dust();
        let r = 0.3;

        let lane_theta = spiral_base_theta(r, &config, None) + dust.offset;
        let p = dust_lane_profile(r, lane_theta, &config, None, &dust);
        assert!(
            (p - 1.0).abs() < 1.0e-12,
            "profile on the offset lane must be ~1: {p}"
        );
    }

    #[test]
    fn test_dust_lane_profile_off_lane_is_lower() {
        let config = sample_spiral_config();
        let dust = sample_dust();
        let r = 0.3;

        let lane_theta = spiral_base_theta(r, &config, None) + dust.offset;
        let on_lane = dust_lane_profile(r, lane_theta, &config, None, &dust);

        // Halfway between the two arm ridges (2 arms => spacing TAU/2).
        let off_lane_theta = lane_theta + TAU / (2.0 * config.arms as f64);
        let off_lane = dust_lane_profile(r, off_lane_theta, &config, None, &dust);

        assert!(on_lane > off_lane, "off-lane profile must be lower");
        assert!(
            off_lane < 0.5,
            "off-lane profile must be well below the ridge: {off_lane}"
        );
    }

    #[test]
    fn test_dust_lane_larger_width_factor_raises_off_lane_profile() {
        let config = sample_spiral_config();
        let r = 0.3;
        // A fixed non-zero off-lane point (0.05 rad from the lane ridge).
        let theta = spiral_base_theta(r, &config, None) + 0.10 + 0.05;

        let narrow = DustLaneConfig {
            strength: 0.40,
            offset: 0.10,
            width_factor: 0.5,
        };
        let wide = DustLaneConfig {
            strength: 0.40,
            offset: 0.10,
            width_factor: 1.2,
        };

        let p_narrow = dust_lane_profile(r, theta, &config, None, &narrow);
        let p_wide = dust_lane_profile(r, theta, &config, None, &wide);

        assert!(p_wide >= p_narrow, "wider lane must not lower the profile");
        assert!(
            p_wide > p_narrow,
            "wider lane must strictly raise the off-lane profile: narrow={p_narrow} wide={p_wide}"
        );
    }

    #[test]
    fn test_dust_offset_is_applied_exactly_once() {
        // With offset 0 the dust lane coincides with the stellar arm 0 ridge;
        // with a non-zero offset the stellar ridge must no longer be a dust
        // ridge (the lane is narrow), proving the offset shifts the dust
        // geometry exactly once instead of being applied per arm iteration or
        // twice.
        let config = sample_spiral_config();
        let r = 0.3;
        let base_theta = spiral_base_theta(r, &config, None);

        let zero_offset = DustLaneConfig {
            strength: 0.40,
            offset: 0.0,
            width_factor: 0.5,
        };
        let shifted = DustLaneConfig {
            strength: 0.40,
            offset: 0.20,
            width_factor: 0.5,
        };

        let p_zero_on_arm = dust_lane_profile(r, base_theta, &config, None, &zero_offset);
        let p_shifted_on_arm = dust_lane_profile(r, base_theta, &config, None, &shifted);

        assert!(
            (p_zero_on_arm - 1.0).abs() < 1.0e-12,
            "zero offset must sit on the stellar ridge: {p_zero_on_arm}"
        );
        assert!(
            p_shifted_on_arm < 0.5,
            "offset must move the lane off the stellar ridge: {p_shifted_on_arm}"
        );

        // The shifted lane must sit exactly at base_theta + offset.
        let p_shifted_on_lane = dust_lane_profile(r, base_theta + 0.20, &config, None, &shifted);
        assert!(
            (p_shifted_on_lane - 1.0).abs() < 1.0e-12,
            "shifted lane must be at base_theta + offset: {p_shifted_on_lane}"
        );
    }

    #[test]
    fn test_barred_dust_inherits_bar_phase_alignment() {
        // When a bar is present, the dust base phase must include the same
        // bar_phase_offset as the stellar arms: the dust ridge of arm 0 at the
        // bar-end radius must sit at the bar orientation plus the dust offset.
        let config = sample_spiral_config();
        let bar = sample_bar();
        let dust = sample_dust();
        let r = bar.half_length;

        let expected_lane = bar.angle_rad + dust.offset;
        let p = dust_lane_profile(r, expected_lane, &config, Some(bar), &dust);
        assert!(
            p > 0.99,
            "dust lane must inherit the bar phase alignment: {p}"
        );
    }

    #[test]
    fn test_smoothstep_gate_bounded_and_monotonic() {
        let inner = 0.1;
        let outer = 0.3;
        let steps = 64;

        let mut prev = 0.0;
        for i in 0..=steps {
            let r = inner + (outer - inner) * (i as f64) / (steps as f64);
            let g = smoothstep_gate(r, inner, outer);
            assert!((0.0..=1.0).contains(&g), "gate out of [0,1]: {g}");
            assert!(g >= prev - 1.0e-12, "gate not monotonic at r={r}");
            prev = g;
        }
        assert_eq!(smoothstep_gate(inner, inner, outer), 0.0);
        assert_eq!(smoothstep_gate(outer, inner, outer), 1.0);
        assert_eq!(smoothstep_gate(0.0, inner, outer), 0.0);
        assert_eq!(smoothstep_gate(1.0, inner, outer), 1.0);
    }

    #[test]
    fn test_barred_dust_radial_gate_equals_arm_gate() {
        let config = sample_spiral_config();
        let bar = sample_bar();

        for i in 0..128 {
            let r = 0.05 + 0.5 * (i as f64) / 127.0;
            assert_eq!(
                dust_radial_gate(r, Some(bar), &config),
                arm_gate(r, bar),
                "barred dust gate must equal arm_gate at r={r}"
            );
        }
    }

    #[test]
    fn test_unbarred_dust_radial_gate_follows_bulge_sigma_relation() {
        let config = sample_spiral_config();
        let inner = config.bulge_sigma;
        let outer = 2.0 * config.bulge_sigma;

        assert_eq!(dust_radial_gate(0.0, None, &config), 0.0);
        assert_eq!(dust_radial_gate(inner * 0.5, None, &config), 0.0);
        assert_eq!(dust_radial_gate(inner, None, &config), 0.0);
        assert_eq!(dust_radial_gate(outer, None, &config), 1.0);
        assert_eq!(dust_radial_gate(outer * 1.5, None, &config), 1.0);

        // Smooth transition strictly between the boundaries.
        let mid = (inner + outer) * 0.5;
        let g_mid = dust_radial_gate(mid, None, &config);
        assert!(
            g_mid > 0.0 && g_mid < 1.0,
            "mid-transition gate must be interior: {g_mid}"
        );

        // Monotonic across the transition.
        let mut prev = 0.0;
        for i in 0..=32 {
            let r = inner + (outer - inner) * (i as f64) / 32.0;
            let g = dust_radial_gate(r, None, &config);
            assert!((0.0..=1.0).contains(&g), "gate out of [0,1]: {g}");
            assert!(g >= prev - 1.0e-12, "gate not monotonic at r={r}");
            prev = g;
        }
    }

    #[test]
    fn test_dust_tau_and_extinction_are_bounded() {
        let config = sample_spiral_config();
        let dust = sample_dust();

        for bar in [None, Some(sample_bar())] {
            for r in [0.05, 0.1, 0.2, 0.5, 1.0] {
                for i in 0..120 {
                    let theta = TAU * (i as f64) / 120.0;
                    let tau = dust.strength
                        * dust_lane_profile(r, theta, &config, bar, &dust)
                        * dust_radial_gate(r, bar, &config);
                    let extinction = (-tau).exp();

                    assert!(tau.is_finite(), "tau not finite at r={r} theta={theta}");
                    assert!(tau >= 0.0, "tau negative at r={r} theta={theta}");
                    // tau = strength * profile * gate with profile <= 1 and
                    // gate <= 1, so tau can never exceed the v1 strength bound.
                    assert!(tau <= 0.55 + 1.0e-12, "tau above the v1 bound: {tau}");
                    assert!(extinction > 0.0, "extinction must stay positive");
                    assert!(extinction <= 1.0, "extinction above 1: {extinction}");
                    assert!(
                        extinction >= (-0.55_f64).exp() - 1.0e-12,
                        "extinction below the v1 lower bound: {extinction}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_dust_attenuates_only_the_luminous_disk() {
        // Prove, with the current private helpers/terms, that a dusty scene
        // differs from the dustless scene only in the luminous disk
        // contribution (disk + gated arms * clumpiness). The bulge and the
        // stellar knots must be untouched by the extinction.
        let config = sample_spiral_config();
        let dust = sample_dust();
        let noise_seed = 11_u32;
        let coarse_noise = OpenSimplex::new(noise_seed);
        let fine_noise = OpenSimplex::new(noise_seed.wrapping_add(1));

        // Rotation and inclination are zero, so sky-plane coordinates equal
        // intrinsic disk coordinates. r is beyond the unbarred dust gate outer
        // boundary (2 * bulge_sigma = 0.12) so the radial gate is exactly 1,
        // and theta sits on the dust lane so the profile is exactly 1.
        let r = 0.30;
        let theta = spiral_base_theta(r, &config, None) + dust.offset;
        let x = r * theta.cos();
        let y = r * theta.sin();

        let dusty = spiral_density(x, y, &config, None, Some(dust), &coarse_noise, &fine_noise);
        let dustless = spiral_density(x, y, &config, None, None, &coarse_noise, &fine_noise);

        let bulge = gaussian(r, config.bulge_sigma) * 0.30;
        let disk = (-r / config.disk_scale).exp() * 0.035;
        let arms = spiral_arm_density(r, theta, &config, None);
        let coarse =
            normalized_noise(coarse_noise.get([x * config.noise_scale, y * config.noise_scale]));
        let fine = normalized_noise(
            fine_noise.get([x * config.noise_scale * 5.0, y * config.noise_scale * 5.0]),
        );
        let clumpiness = 0.45 + 1.35 * coarse.powf(1.4);
        // Unbarred: arm_gate_value is exactly 1.0.
        let stellar_knots = fine.powf(8.0) * arms * 0.85;
        let arm_clump = arms * clumpiness;
        let luminous_disk = disk + arm_clump;
        let tau = dust.strength
            * dust_lane_profile(r, theta, &config, None, &dust)
            * dust_radial_gate(r, None, &config);
        let extinction = (-tau).exp();

        let expected_dusty = bulge + luminous_disk * extinction + stellar_knots;
        let expected_dustless = bulge + disk + arm_clump + stellar_knots;

        assert!(
            (dusty - expected_dusty).abs() < 1.0e-12,
            "dusty density must match the luminous-disk-extinction composition: dusty={dusty} expected={expected_dusty}"
        );
        assert!(
            (dustless - expected_dustless).abs() < 1.0e-12,
            "dustless density must match the pre-dust composition: dustless={dustless} expected={expected_dustless}"
        );

        // The extinction must actually attenuate at this point.
        assert!(
            extinction < 1.0,
            "test point must lie inside an attenuated lane"
        );
        assert!(dusty < dustless, "dust must reduce the density at the lane");

        // The difference is exactly the luminous-disk attenuation.
        let delta = dustless - dusty;
        let expected_delta = luminous_disk * (1.0 - extinction);
        assert!(
            (delta - expected_delta).abs() < 1.0e-12,
            "density difference must be exactly the luminous-disk attenuation: delta={delta} expected={expected_delta}"
        );
    }

    #[test]
    fn test_dusty_scene_is_deterministic_across_runs() {
        // Seed 4 is dusty (and unbarred) under spiral/dust/v1.
        let seed = 4_u64;
        let a = crate::engine::ArtModel::Spiral.generate_scene(40, 20, Some(seed));
        let b = crate::engine::ArtModel::Spiral.generate_scene(40, 20, Some(seed));
        assert_eq!(a.density, b.density);
    }

    #[test]
    fn test_dust_derivation_does_not_perturb_bar_or_legacy_streams() {
        // The bar and dust feature streams are independent: deriving dust must
        // not change the bar configuration, and neither may advance the
        // legacy RNG.
        let seed = 42_u64;
        let context = GenerationContext::new(seed);

        let bar_before = BarConfig::from_context(context);
        let _dust = DustLaneConfig::from_context(context);
        let bar_after = BarConfig::from_context(context);
        assert_eq!(
            bar_before, bar_after,
            "dust derivation must not change the bar config"
        );

        let mut baseline_rng = StdRng::seed_from_u64(seed);
        let baseline_config = SpiralGalaxyConfig::from_rng(&mut baseline_rng);
        let baseline_noise_seed = baseline_rng.random::<u32>();

        let mut isolated_rng = StdRng::seed_from_u64(seed);
        let isolated_config = SpiralGalaxyConfig::from_rng(&mut isolated_rng);
        let _ = BarConfig::from_context(context);
        let _ = DustLaneConfig::from_context(context);
        let isolated_noise_seed = isolated_rng.random::<u32>();

        assert_eq!(isolated_config, baseline_config);
        assert_eq!(isolated_noise_seed, baseline_noise_seed);
    }

    // ---- Phase 3: explicit sampling geometry ----

    #[test]
    fn test_sampling_geometry_dimensions_match_legacy_chain() {
        // Pin the production sampling topology: the HALF_BLOCK shape (1×2
        // logical samples per terminal cell) and an isotropic 3×
        // supersample. These are compile-time constants, not runtime
        // configuration.
        assert_eq!(CellSamplingShape::HALF_BLOCK.columns(), 1);
        assert_eq!(CellSamplingShape::HALF_BLOCK.rows(), 2);
        assert_eq!(SamplingGeometry::SUPERSAMPLE_FACTOR, 3);

        for (terminal_width, terminal_height) in
            [(0, 0), (1, 1), (0, 7), (5, 0), (40, 20), (101, 33)]
        {
            let geometry = SamplingGeometry::for_terminal(
                terminal_width,
                terminal_height,
                CellSamplingShape::HALF_BLOCK,
            );

            assert_eq!(
                geometry.logical_width(),
                terminal_width.max(1),
                "logical width for ({terminal_width},{terminal_height})"
            );
            assert_eq!(
                geometry.logical_height(),
                terminal_height.max(1) * 2,
                "logical height for ({terminal_width},{terminal_height})"
            );
            assert_eq!(
                geometry.high_width(),
                geometry.logical_width() * 3,
                "high width for ({terminal_width},{terminal_height})"
            );
            assert_eq!(
                geometry.high_height(),
                geometry.logical_height() * 3,
                "high height for ({terminal_width},{terminal_height})"
            );
        }
    }

    #[test]
    fn test_sampling_geometry_normalized_coordinates_match_legacy_expression() {
        // The geometry must invoke the exact legacy normalized-coordinate
        // expression, `2.0 * ((i as f64 + 0.5) / n as f64 - 0.5)`, with the
        // high-resolution extent of each axis. Bit-exact anchors are compared
        // via `to_bits`; no algebraically rearranged formula is used, and no
        // symmetry property is asserted (it is not bit-for-bit guaranteed).
        let geometry = SamplingGeometry::for_terminal(40, 20, CellSamplingShape::HALF_BLOCK);
        let high_width = geometry.high_width();
        let high_height = geometry.high_height();

        for i in [0usize, 1, high_width / 2, high_width - 2, high_width - 1] {
            let expected = 2.0 * ((i as f64 + 0.5) / high_width as f64 - 0.5);
            assert_eq!(
                geometry.normalized_x(i).to_bits(),
                expected.to_bits(),
                "normalized_x({i}) must match the legacy expression at n={high_width}"
            );
        }

        for j in [0usize, 1, high_height / 2, high_height - 2, high_height - 1] {
            let expected = 2.0 * ((j as f64 + 0.5) / high_height as f64 - 0.5);
            assert_eq!(
                geometry.normalized_y(j).to_bits(),
                expected.to_bits(),
                "normalized_y({j}) must match the legacy expression at n={high_height}"
            );
        }

        // A non-square case where the axis extents differ, so each axis is
        // proven to use its own high dimension.
        let non_square = SamplingGeometry::for_terminal(7, 3, CellSamplingShape::HALF_BLOCK);
        assert_ne!(
            non_square.high_width(),
            non_square.high_height(),
            "test case must have distinct axis extents"
        );

        for i in [
            0usize,
            non_square.high_width() / 2,
            non_square.high_width() - 1,
        ] {
            let expected = 2.0 * ((i as f64 + 0.5) / non_square.high_width() as f64 - 0.5);
            assert_eq!(
                non_square.normalized_x(i).to_bits(),
                expected.to_bits(),
                "normalized_x({i}) must use the x high dimension"
            );
        }

        for j in [
            0usize,
            non_square.high_height() / 2,
            non_square.high_height() - 1,
        ] {
            let expected = 2.0 * ((j as f64 + 0.5) / non_square.high_height() as f64 - 0.5);
            assert_eq!(
                non_square.normalized_y(j).to_bits(),
                expected.to_bits(),
                "normalized_y({j}) must use the y high dimension"
            );
        }
    }

    #[test]
    fn test_sampling_geometry_edge_dimensions_through_generation() {
        // The direct generator must keep the legacy output dimensions for
        // degenerate terminal sizes: logical width max(W,1), logical height
        // max(H,1) * 2.
        for (terminal_width, terminal_height, expected) in
            [(0usize, 0usize, (1, 2)), (0, 5, (1, 10)), (5, 0, (5, 2))]
        {
            let mut rng = StdRng::seed_from_u64(7);
            let map = generate_spiral_galaxy(terminal_width, terminal_height, &mut rng);
            assert_eq!(
                (map.width, map.height),
                expected,
                "output dimensions for ({terminal_width},{terminal_height})"
            );
        }
    }

    #[test]
    fn test_sampling_geometry_downsamples_exact_3x3_blocks() {
        // The high-resolution grid is exactly 3×3 samples per logical cell,
        // so each logical cell must average exactly one 3×3 block. Fill each
        // block with a distinct small integer constant (exact in f64) and
        // verify the downsampled value, proving the geometry chooses the same
        // 3×3 source bins for each logical cell.
        let geometry = SamplingGeometry::for_terminal(2, 2, CellSamplingShape::HALF_BLOCK);
        let high_width = geometry.high_width();
        let high_height = geometry.high_height();

        let mut high = DensityMap::new(high_width, high_height);
        for y in 0..high_height {
            for x in 0..high_width {
                let block = ((x / 3) + (y / 3) * 10) as f64;
                high.set(x, y, block);
            }
        }

        let logical = high.downsample_average(geometry.logical_width(), geometry.logical_height());
        assert_eq!(logical.width, geometry.logical_width());
        assert_eq!(logical.height, geometry.logical_height());

        for oy in 0..logical.height {
            for ox in 0..logical.width {
                let expected = (ox + oy * 10) as f64;
                assert_eq!(
                    logical.get(ox, oy),
                    expected,
                    "logical cell ({ox},{oy}) must average exactly one 3×3 block"
                );
            }
        }
    }

    // ---- Phase 4: terminal-cell topology cross-check ----

    #[test]
    fn test_half_block_topology_cross_checks_sampling_geometry() {
        // The production topology abstraction must agree with the Spiral
        // sampling geometry: the geometry's logical dimensions must derive
        // exactly from the HALF_BLOCK shape. This cross-check lives in the
        // galaxy test module on purpose: `SamplingGeometry` stays private
        // here, so the generator abstraction is not widened for testing.
        let shape = CellSamplingShape::HALF_BLOCK;
        let terminal_width = 40;
        let terminal_height = 20;
        let geometry = SamplingGeometry::for_terminal(terminal_width, terminal_height, shape);

        assert_eq!(
            geometry.logical_width(),
            terminal_width.max(1) * shape.columns()
        );
        assert_eq!(
            geometry.logical_height(),
            terminal_height.max(1) * shape.rows()
        );
    }

    // ---- Phase 5A: shape-aware Spiral sampling ----

    #[test]
    fn test_sampling_geometry_quadrant_dimensions() {
        // The QUADRANT shape (2×2) must produce the future quadrant
        // dimension chain: logical 2W×2H and supersampled 6W×6H, with the
        // same max(1) edge behavior as HALF_BLOCK.
        for (terminal_width, terminal_height) in
            [(0, 0), (1, 1), (0, 7), (5, 0), (40, 20), (101, 33)]
        {
            let geometry = SamplingGeometry::for_terminal(
                terminal_width,
                terminal_height,
                CellSamplingShape::QUADRANT,
            );

            assert_eq!(
                geometry.logical_width(),
                terminal_width.max(1) * 2,
                "logical width for ({terminal_width},{terminal_height})"
            );
            assert_eq!(
                geometry.logical_height(),
                terminal_height.max(1) * 2,
                "logical height for ({terminal_width},{terminal_height})"
            );
            assert_eq!(
                geometry.high_width(),
                geometry.logical_width() * 3,
                "high width for ({terminal_width},{terminal_height})"
            );
            assert_eq!(
                geometry.high_height(),
                geometry.logical_height() * 3,
                "high height for ({terminal_width},{terminal_height})"
            );
        }
    }

    #[test]
    fn test_sampling_geometry_quadrant_normalized_coordinates() {
        // QUADRANT normalized coordinates must use the exact same frozen
        // expression, `2.0 * ((i as f64 + 0.5) / n as f64 - 0.5)`, evaluated
        // with the QUADRANT high dimensions. Bit-exact via `to_bits`.
        let geometry = SamplingGeometry::for_terminal(40, 20, CellSamplingShape::QUADRANT);
        let high_width = geometry.high_width();
        let high_height = geometry.high_height();

        assert_eq!(high_width, 40 * 2 * 3);
        assert_eq!(high_height, 20 * 2 * 3);

        for i in [0usize, 1, high_width / 2, high_width - 2, high_width - 1] {
            let expected = 2.0 * ((i as f64 + 0.5) / high_width as f64 - 0.5);
            assert_eq!(
                geometry.normalized_x(i).to_bits(),
                expected.to_bits(),
                "normalized_x({i}) must match the frozen expression at n={high_width}"
            );
        }

        for j in [0usize, 1, high_height / 2, high_height - 2, high_height - 1] {
            let expected = 2.0 * ((j as f64 + 0.5) / high_height as f64 - 0.5);
            assert_eq!(
                geometry.normalized_y(j).to_bits(),
                expected.to_bits(),
                "normalized_y({j}) must match the frozen expression at n={high_height}"
            );
        }
    }

    #[test]
    fn test_spiral_quadrant_generation_dimensions() {
        // Shape-aware QUADRANT generation must return a 2W×2H DensityMap,
        // with the same max(1) edge behavior as the production HALF_BLOCK
        // path.
        for (terminal_width, terminal_height, expected) in [
            (0usize, 0usize, (2, 2)),
            (0, 5, (2, 10)),
            (5, 0, (10, 2)),
            (30, 15, (60, 30)),
        ] {
            let mut rng = StdRng::seed_from_u64(7);
            let map = generate_spiral_galaxy_with_shape(
                terminal_width,
                terminal_height,
                &mut rng,
                GenerationContext::new(7),
                CellSamplingShape::QUADRANT,
            );
            assert_eq!(
                (map.width, map.height),
                expected,
                "QUADRANT output dimensions for ({terminal_width},{terminal_height})"
            );
        }
    }

    #[test]
    fn test_spiral_quadrant_generation_is_deterministic() {
        // Same seed + W/H + QUADRANT must produce a bit-identical
        // DensityMap across repeated runs.
        for seed in [4_u64, 16, 42] {
            let a = generate_spiral_galaxy_with_shape(
                30,
                15,
                &mut StdRng::seed_from_u64(seed),
                GenerationContext::new(seed),
                CellSamplingShape::QUADRANT,
            );
            let b = generate_spiral_galaxy_with_shape(
                30,
                15,
                &mut StdRng::seed_from_u64(seed),
                GenerationContext::new(seed),
                CellSamplingShape::QUADRANT,
            );
            assert_eq!(
                a, b,
                "QUADRANT density must be deterministic for seed {seed}"
            );
        }
    }

    #[test]
    fn test_generate_scene_matches_shape_aware_half_block() {
        // The legacy generate_scene path (production HALF_BLOCK) must be
        // equivalent to the new shape-aware HALF_BLOCK path for
        // representative fixed seeds: unbarred (4), barred (16), and
        // barred+dusty (42).
        for seed in [4_u64, 16, 42] {
            let legacy = crate::engine::ArtModel::Spiral.generate_scene(30, 15, Some(seed));

            let mut rng = StdRng::seed_from_u64(seed);
            let shape_aware = generate_spiral_galaxy_with_shape(
                30,
                15,
                &mut rng,
                GenerationContext::new(seed),
                CellSamplingShape::HALF_BLOCK,
            );

            assert_eq!(
                legacy.density, shape_aware,
                "generate_scene (HALF_BLOCK) must equal the shape-aware HALF_BLOCK path for seed {seed}"
            );
        }
    }
}
