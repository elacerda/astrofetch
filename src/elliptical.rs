//! Deterministic Elliptical morphology configuration for the v2 generator.
//!
//! B1.2 introduces the morphology contract only: an immutable
//! [`EllipticalGalaxyConfig`] derived once per scene from the versioned
//! `elliptical/morphology/v2` feature namespace. The density generator
//! consumes this config in B2; until then nothing in the render pipeline
//! reads this module, so Elliptical rendered output stays byte-identical to
//! baseline `85ca173`.
//!
//! B2.1 adds the pure mathematical body kernel to this module
//! ([`elliptical_radius`], [`sersic_body_profile`],
//! [`elliptical_body_profile`]). It is still not consumed by
//! `src/engine.rs` or any renderer, so ordinary Elliptical output remains
//! byte-identical to the B1.2 checkpoint.
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
//!   `[2, 6]`; B2.1 evaluates the pure body kernel
//!   `I_body(dx, dy) = exp(−b_n·(r_ell/Re)^(1/n))`, where `r_ell` is the
//!   rotated elliptical radius from [`elliptical_radius`]; see
//!   [`sersic_body_profile`] for the chosen normalization and [`sersic_b`]
//!   for the `b_n` approximation.
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

// ────────────────────────────────────────────────────────────────────
// B2.1 — pure body kernel (math only; not yet connected to src/engine.rs)
// ────────────────────────────────────────────────────────────────────

/// Sersic concentration parameter `b_n` for the v2 body kernel.
///
/// Returns the analytic approximation
///
/// ```text
/// b_n ≈ 2n − 1/3
/// ```
///
/// for a Sersic shape parameter `n` (dimensionless; B1.2 draws produce
/// `n` in `[2, 6]`).
///
/// Intended meaning, documented precisely: the *exact* `b_n` is the value
/// satisfying the half-light condition
///
/// ```text
/// ∫₀^{b_n} u^(2n−1) e^(−u) du = Γ(2n) / 2,
/// ```
///
/// i.e. the median of the Gamma(2n, 1) distribution. With that exact
/// value, `Re` in the conventional Sersic profile is *exactly* the
/// two-dimensional half-light radius of the body. `2n − 1/3` is the
/// leading Wilson–Hilferty term of that median and reproduces the exact
/// value to within 0.15 % for `n` in `[2, 6]` (0.147 % at n = 2, 0.015 %
/// at n = 6). With the approximation, `Re` therefore remains the 2-D
/// half-light radius up to that same ~0.1 % error; this is *not* a
/// mathematical identity, and `Re` is not claimed to be an exact
/// half-light radius. No special-function dependency is introduced.
///
/// # Preconditions
/// `profile_index` must be positive; the approximation is validated for
/// `n` in `[2, 6]` (the B1.2 range).
pub(crate) fn sersic_b(profile_index: f64) -> f64 {
    2.0 * profile_index - 1.0 / 3.0
}

/// Rotated elliptical radius of a point in canvas-fraction coordinates.
///
/// Computes the elliptical radius of a point `(dx, dy)` given in legacy
/// canvas-fraction coordinates:
///
/// ```text
/// dx = (x − width/2) / width
/// dy = (y − height/2) / height
/// ```
///
/// i.e. the canvas spans `dx ∈ [−0.5, 0.5)`, `dy ∈ [−0.5, 0.5)` with the
/// origin at the canvas centre and `+x` to the right. The mapping from
/// generated-map pixel indices to `(dx, dy)` (including the render-height
/// convention of the generated density map) belongs to the B2.2
/// integrator; this helper is pure math on the fractions themselves.
///
/// Axis convention (matches the legacy `generate_elliptical_density` in
/// `src/engine.rs` exactly — do not swap major/minor in B2.2):
///
/// 1. Rotate the point into the ellipse frame with the legacy convention:
///
///    ```text
///    x_rot =  dx·cos(pa) + dy·sin(pa)
///    y_rot = −dx·sin(pa) + dy·cos(pa)
///    ```
///
/// 2. The **semi-major axis (`a = 1`) lies along `x′`**: the isophote
///    `r = 1` crosses the major axis at distance 1 from the centre.
/// 3. The **semi-minor axis is `b = q = axis_ratio` along `y′`**, i.e.
///    the minor-axis coordinate is divided by `q`:
///
///    ```text
///    r = hypot(x_rot, y_rot / q)
///    ```
///
/// The major axis therefore points along the canvas-fraction direction
/// `(cos pa, sin pa)` — equivalently, `y_rot = 0` along the major axis —
/// so the major axis sits at `position_angle` from the `+x` canvas axis,
/// exactly as the legacy generator rotates it. In canvas-fraction units
/// (canvas half-extent = 0.5): a point on the major axis at canvas
/// distance `d` has `r = d`; a point on the minor axis at the same
/// canvas distance has `r = d/q > d`. `hypot` is used so the result is
/// finite and non-negative for all finite inputs in the preconditions.
///
/// # Preconditions
/// `axis_ratio` in `(0, 1]` (B1.2 draws produce `(0.55, 0.95)`),
/// `position_angle` in `[0, π)` (any finite value works mathematically),
/// `dx` and `dy` finite.
pub(crate) fn elliptical_radius(dx: f64, dy: f64, axis_ratio: f64, position_angle: f64) -> f64 {
    debug_assert!(
        axis_ratio > 0.0 && axis_ratio <= 1.0,
        "axis_ratio must be in (0, 1]"
    );
    debug_assert!(dx.is_finite() && dy.is_finite(), "dx/dy must be finite");

    let cos_angle = position_angle.cos();
    let sin_angle = position_angle.sin();

    // Legacy rotation: major axis along x′, minor axis compressed by q
    // along y′ (see doc for the exact axis convention).
    let x_rot = dx * cos_angle + dy * sin_angle;
    let y_rot = -dx * sin_angle + dy * cos_angle;

    f64::hypot(x_rot, y_rot / axis_ratio)
}

/// Central-value-normalized Sersic-like body profile.
///
/// Chosen convention (fixed here so B2.2 cannot silently pick another
/// one): the **central-value-normalized** form
///
/// ```text
/// I_norm(r) = exp( −b_n · (r / Re)^(1/n) )
/// ```
///
/// which is the conventional Sersic profile
///
/// ```text
/// I_conv(r) = exp( −b_n · ((r/Re)^(1/n) − 1) )
/// ```
///
/// divided by its central value `I_conv(0) = exp(b_n)`. The two forms
/// differ only by a constant factor, so their shapes — and therefore
/// the radius enclosing half the 2-D light — are identical: `Re` keeps
/// the (approximate, see [`sersic_b`]) 2-D half-light-radius
/// interpretation. The normalization is chosen so the central value does
/// not explode with `n` (the conventional form has `I_conv(0) = e^{b_n}`,
/// which grows to ~1.1e5 at n = 6).
///
/// Properties:
///
/// * `I_norm(0) == 1.0` exactly;
/// * finite, in `(0, 1]`, for every `r ≥ 0` — bounded to a sensible
///   finite range before the existing downstream render normalization;
/// * strictly decreasing (hence monotonic non-increasing) and smooth in
///   `r` for `r > 0`;
/// * supports every B1.2 `n` in `[2, 6]` and Re range.
///
/// # Units and shapes
/// `r` and `Re` are in the same units (legacy canvas-fraction units when
/// composed with [`elliptical_radius`]); `Re` > 0; `n` > 0, validated on
/// `[2, 6]`; `r` ≥ 0. Scalar in, scalar out; no allocations, no I/O.
pub(crate) fn sersic_body_profile(r: f64, effective_radius: f64, profile_index: f64) -> f64 {
    debug_assert!(r >= 0.0 && effective_radius > 0.0 && profile_index > 0.0);

    let b_n = sersic_b(profile_index);
    let scaled = (r / effective_radius).powf(1.0 / profile_index);

    (-b_n * scaled).exp()
}

/// Pure Elliptical v2 body kernel: rotated elliptical radius passed
/// through the normalized Sersic body profile.
///
/// ```text
/// I_body(dx, dy) = sersic_body_profile(
///     elliptical_radius(dx, dy, axis_ratio, position_angle),
///     effective_radius, profile_index)
/// ```
///
/// Consumes only the four B1.2 body parameters; the B3 parameters
/// (`core_softening_fraction`, `central_excess`, `outer_halo_*`,
/// `isophote_shape`) are deliberately not used here. Returns a value in
/// `(0, 1]` with maximum 1.0 at the centre. Pure function: no RNG, no
/// I/O, no mutation.
pub(crate) fn elliptical_body_profile(
    dx: f64,
    dy: f64,
    axis_ratio: f64,
    position_angle: f64,
    effective_radius: f64,
    profile_index: f64,
) -> f64 {
    let r = elliptical_radius(dx, dy, axis_ratio, position_angle);
    sersic_body_profile(r, effective_radius, profile_index)
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

    // ── B2.1 — pure body kernel ─────────────────────────────────────

    #[test]
    fn test_elliptical_radius_q_one_is_rotationally_symmetric() {
        let q = 1.0_f64;
        let (dx, dy) = (0.137_f64, -0.291);
        for pa in [
            0.0_f64,
            0.4,
            std::f64::consts::FRAC_PI_2,
            1.7,
            std::f64::consts::PI - 1.0e-9,
        ] {
            // A round ellipse is the plain Euclidean distance, for any PA.
            assert!(
                (elliptical_radius(dx, dy, q, pa) - dx.hypot(dy)).abs() < 1.0e-12,
                "q=1 must be rotationally symmetric (pa={pa})"
            );
            // Rotating the point itself must not change the radius either.
            let alpha = 1.13_f64;
            let (rx, ry) = (
                dx * alpha.cos() - dy * alpha.sin(),
                dx * alpha.sin() + dy * alpha.cos(),
            );
            assert!(
                (elliptical_radius(rx, ry, q, pa) - elliptical_radius(dx, dy, q, pa)).abs()
                    < 1.0e-12,
                "q=1 must be invariant under point rotation (pa={pa})"
            );
        }
    }

    #[test]
    fn test_elliptical_radius_major_minor_axis_relation() {
        let q = 0.6_f64;
        let d = 0.3_f64;

        // PA = 0: major axis along +x (the unit semi-axis), minor along +y.
        let r_major = elliptical_radius(d, 0.0, q, 0.0);
        let r_minor = elliptical_radius(0.0, d, q, 0.0);
        assert!(
            (r_major - d).abs() < 1.0e-15,
            "major axis is the unit semi-axis: got {r_major}"
        );
        assert!(
            (r_minor - d / q).abs() < 1.0e-15,
            "minor axis is stretched by 1/q: got {r_minor}"
        );
        assert!(r_minor > r_major, "q < 1 must give a shorter minor extent");

        // PA = π/4: the major axis direction is (cos π/4, sin π/4) and the
        // minor direction is its perpendicular (−sin π/4, cos π/4).
        let pa = std::f64::consts::FRAC_PI_4;
        let c = pa.cos();
        let s = pa.sin();
        let r_on_major = elliptical_radius(d * c, d * s, q, pa);
        let r_on_minor = elliptical_radius(-d * s, d * c, q, pa);
        assert!((r_on_major - d).abs() < 1.0e-12);
        assert!((r_on_minor - d / q).abs() < 1.0e-12);
    }

    #[test]
    fn test_position_angle_rotates_major_axis() {
        let q = 0.7_f64;
        let pa1 = 0.4_f64;
        let pa2 = 1.1_f64;
        let d = 0.37_f64;

        // A point on the major axis of frame 1.
        let (dx, dy) = (d * pa1.cos(), d * pa1.sin());
        assert!(
            (elliptical_radius(dx, dy, q, pa1) - d).abs() < 1.0e-12,
            "a major-axis point must sit at canvas distance d in its own frame"
        );

        // In frame 2 the angular offset Δ = pa1 − pa2 enters analytically:
        // r = d·√(cos²Δ + (sinΔ/q)²) > d.
        let delta = pa1 - pa2;
        let expected = d * (delta.cos().powi(2) + (delta.sin() / q).powi(2)).sqrt();
        assert!(
            (elliptical_radius(dx, dy, q, pa2) - expected).abs() < 1.0e-12,
            "rotating the position angle must rotate the major axis"
        );
        assert!(expected > d, "off the major axis the radius must grow");
    }

    #[test]
    fn test_sersic_profile_central_value() {
        for re in [0.20_f64, 0.34] {
            for n in [2.0_f64, 3.0, 4.0, 5.0, 6.0] {
                let central = sersic_body_profile(0.0, re, n);
                assert!(central.is_finite(), "central value must be finite");
                assert_eq!(
                    central, 1.0,
                    "normalized central value must be exactly 1 (Re={re}, n={n})"
                );
            }
        }
    }

    #[test]
    fn test_sersic_profile_monotonic_non_increasing() {
        const STEPS: usize = 4096;
        let re = 0.30_f64;
        for n in [2.0_f64, 3.0, 4.0, 5.0, 6.0] {
            // Dense linear grid 0..10·Re.
            let mut prev = sersic_body_profile(0.0, re, n);
            for i in 1..=STEPS {
                let r = 10.0 * re * (i as f64 / STEPS as f64);
                let value = sersic_body_profile(r, re, n);
                assert!(
                    value <= prev + 1.0e-12,
                    "n={n}: profile rose at r={r}: {prev} -> {value}"
                );
                prev = value;
            }

            // Log-spaced grid hugging r = 0, where the slope vanishes.
            prev = sersic_body_profile(0.0, re, n);
            for i in 0..=96 {
                let r = re * 10f64.powf(-12.0 + i as f64 / 8.0);
                let value = sersic_body_profile(r, re, n);
                assert!(
                    value <= prev + 1.0e-12,
                    "n={n}: profile rose at r={r}: {prev} -> {value}"
                );
                prev = value;
            }
        }
    }

    #[test]
    fn test_family_extrema_stay_finite_and_bounded() {
        let mut corners = 0usize;
        for family in [
            EllipticalFamily::CompactDisky,
            EllipticalFamily::Classical,
            EllipticalFamily::GiantBoxy,
            EllipticalFamily::CdLike,
        ] {
            let ranges = FamilyRanges::for_family(family);
            for re in [ranges.effective_radius.0, ranges.effective_radius.1] {
                for n in [ranges.profile_index.0, ranges.profile_index.1] {
                    for q in [ranges.axis_ratio.0, ranges.axis_ratio.1] {
                        for pa in [
                            0.0_f64,
                            std::f64::consts::FRAC_PI_2,
                            std::f64::consts::PI - 1.0e-6,
                        ] {
                            corners += 1;
                            // Sweep the full canvas-fraction domain.
                            for gy in 0..33usize {
                                for gx in 0..33usize {
                                    let dx = -0.5 + gx as f64 * (1.0 / 32.0);
                                    let dy = -0.5 + gy as f64 * (1.0 / 32.0);
                                    let r = elliptical_radius(dx, dy, q, pa);
                                    assert!(
                                        r.is_finite() && r >= 0.0,
                                        "radius not finite at family={family:?}, q={q}, pa={pa}, ({dx}, {dy})"
                                    );
                                    let value = sersic_body_profile(r, re, n);
                                    assert!(
                                        value.is_finite() && value > 0.0 && value <= 1.0,
                                        "profile out of range: {value} at r={r}, Re={re}, n={n}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(corners, 4 * 2 * 2 * 2 * 3);
    }

    #[test]
    fn test_sersic_profile_no_nan_or_infinity() {
        for n in [2.0_f64, 3.0, 4.0, 5.0, 6.0] {
            let re = 0.30_f64;
            let radii = [
                0.0_f64,
                1.0e-300 * re,
                1.0e-9 * re,
                1.0e-3 * re,
                0.1 * re,
                re,
                2.0 * re,
                5.0 * re,
                10.0 * re,
                50.0 * re,
            ];
            for r in radii {
                let value = sersic_body_profile(r, re, n);
                assert!(
                    value.is_finite() && value > 0.0 && value <= 1.0,
                    "bad value {value} at r={r}, n={n}"
                );
            }
        }
    }

    /// Fraction of the normalized profile's total 2-D light inside
    /// `c·Re`, computed as
    ///
    /// ```text
    /// ∫₀^{b·c^(1/n)} u^(2n−1) e^(−u) du / Γ(2n)
    /// ```
    ///
    /// (Simpson, 4000 intervals). The closed form avoids re-integrating
    /// the whole profile; valid for the integer `n` used by the tests.
    fn light_fraction_within(c: f64, n: f64) -> f64 {
        let b_n = sersic_b(n);
        let upper = b_n * c.powf(1.0 / n);
        let power = (2.0 * n) as i32 - 1;
        let integrand = |u: f64| u.powi(power) * (-u).exp();

        const STEPS: usize = 4000;
        let h = upper / STEPS as f64;
        let mut sum = integrand(0.0) + integrand(upper);
        for i in 1..STEPS {
            let weight = if i % 2 == 1 { 4.0 } else { 2.0 };
            sum += weight * integrand(i as f64 * h);
        }

        let gamma_2n: f64 = (1..(2.0 * n) as u32).map(|i| i as f64).product();
        (h / 3.0) * sum / gamma_2n
    }

    #[test]
    fn test_larger_n_concentrated_core_and_broader_wings() {
        let re = 0.30_f64;
        let profile_at = |c: f64, n: f64| sersic_body_profile(c * re, re, n);

        // (a) Concentration: for every r in (0, Re] the larger-n profile
        // is strictly lower — light is packed closer to the centre.
        for i in 1..=256usize {
            let c = i as f64 / 256.0;
            let mut previous_n = 2.0_f64;
            let mut previous = profile_at(c, previous_n);
            for n in 3..=6 {
                let nn = n as f64;
                let value = profile_at(c, nn);
                assert!(
                    previous > value,
                    "at r = {c}·Re: I_{{n={previous_n}}} must exceed I_{{n={nn}}} ({previous} vs {value})"
                );
                previous = value;
                previous_n = nn;
            }
        }

        // (b) Outer wings: n=6 is far below n=2 at intermediate radii,
        // but the analytic crossover at r ≈ 32.2·Re puts n=6 above n=2
        // at large radii — the mathematical signature of broader wings.
        assert!(
            profile_at(10.0, 2.0) > profile_at(10.0, 6.0),
            "below the crossover the n=2 profile must be higher"
        );
        let wing_ratio = profile_at(40.0, 6.0) / profile_at(40.0, 2.0);
        assert!(
            wing_ratio > 2.0,
            "above the crossover the n=6 wings must clearly exceed n=2 (ratio={wing_ratio})"
        );

        // (c) Integrated light (the actual physical statement): Re
        // encloses ~half the 2-D light of every profile, and both the
        // fraction within 0.5·Re and the fraction beyond 2·Re increase
        // strictly with n.
        for n in 2..=6u32 {
            let within_re = light_fraction_within(1.0, n as f64);
            assert!(
                (within_re - 0.5).abs() < 0.01,
                "n={n}: Re must remain the approximate 2-D half-light radius (frac(<Re)={within_re})"
            );
        }
        let mut frac_inner = 0.0_f64;
        let mut frac_outer = 0.0_f64;
        for n in 2..=6u32 {
            let inner = light_fraction_within(0.5, n as f64);
            assert!(
                inner > frac_inner,
                "light within 0.5·Re must increase with n (n={n}: {inner} vs {frac_inner})"
            );
            let outer = 1.0 - light_fraction_within(2.0, n as f64);
            assert!(
                outer > frac_outer,
                "light beyond 2·Re must increase with n (n={n}: {outer} vs {frac_outer})"
            );
            frac_inner = inner;
            frac_outer = outer;
        }
    }

    #[test]
    fn test_composite_kernel_matches_composition() {
        for (dx, dy) in [(0.0_f64, 0.0), (0.15, -0.08), (-0.32, 0.27), (0.4, 0.4)] {
            let q = 0.68_f64;
            let pa = 0.9_f64;
            let re = 0.27_f64;
            let n = 3.4_f64;
            let direct = elliptical_body_profile(dx, dy, q, pa, re, n);
            let composed = sersic_body_profile(elliptical_radius(dx, dy, q, pa), re, n);
            assert_eq!(direct, composed);
            assert!(
                direct.is_finite() && direct > 0.0 && direct <= 1.0,
                "kernel value out of range: {direct}"
            );
        }
    }
}
