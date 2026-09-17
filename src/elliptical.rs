//! Deterministic Elliptical morphology for the v2 generator.
//!
//! The v2 Elliptical body is a central-value-normalized Sersic-like
//! profile. This module owns the whole generation path: an immutable
//! [`EllipticalGalaxyConfig`] derived once per scene from the versioned
//! `elliptical/morphology/v2` feature namespace; the pure body kernel
//! ([`elliptical_radius`], [`elliptical_shaped_radius`],
//! [`sersic_body_profile`], [`elliptical_body_profile`]); and the scene
//! generation that
//! `src/engine.rs` dispatches the Elliptical model to
//! ([`generate_elliptical_density`]).
//!
//! [`elliptical_cell`] is the single per-cell seam that establishes the
//! elliptical radius, the geometric support decision and the
//! central-structure-applied Sersic intensity from one radius evaluation
//! (body → [`apply_central_structure`] → grain);
//! [`elliptical_density_profile`] maps it to the post-support,
//! pre-render-normalization body, and [`generate_elliptical_density`]
//! applies the multiplicative local grain on supported cells only. The
//! legacy fixed double-Gaussian body and the legacy `0.018` brightness
//! cutoff are both retired; the config is derived exactly once per scene
//! from the versioned feature namespace and is never drawn from the
//! legacy scene RNG; only the per-supported-cell grain consumes that RNG
//! (see [`generate_elliptical_density`]).
//!
//! Geometric support contract:
//!
//! * A cell is visible **iff** `r_shape <= support_re_multiplier(family) ·
//!   Re` (boundary inclusive), with `r_shape` from
//!   [`elliptical_shaped_radius`] — the B2 elliptical radius
//!   [`elliptical_radius`] modulated by the fourth-harmonic
//!   `isophote_shape` — and the multiplier from
//!   [`EllipticalFamily::support_re_multiplier`] — a deterministic
//!   family contract constant, not a measurement and not an RNG draw. It is
//!   deliberately not stored in [`EllipticalGalaxyConfig`]. The same
//!   shaped radius also feeds the Sersic intensity, so support and
//!   profile can never be evaluated from inconsistent radii.
//! * Why the legacy `0.018` cutoff was retired: as an absolute brightness
//!   threshold on the peak-normalized Sersic body it coupled visibility to
//!   the profile amplitude — the body crosses 0.018 at ≈1.20·Re for
//!   `n = 2` but ≈0.075·Re for `n = 4` and ≈0.002·Re for `n = 6` — so the
//!   profile index silently *defined* the support. Geometric support
//!   decouples visibility from amplitude: Sersic `n` changes concentration
//!   without moving the support boundary.
//! * Support multipliers are family-specific presentation-contract
//!   constants expressed as multiples of `Re` (CompactDisky 1.75,
//!   Classical 1.40, GiantBoxy 1.00, CdLike 1.30).
//! * Inside support: Sersic intensity evaluated at the same shaped
//!   radius, passed through the central structure — the softened core
//!   (`core_softening_fraction`) and the central excess
//!   (`central_excess`), intensity only and never support — and then
//!   perturbed by the multiplicative local grain
//!   (`factor = 1 + U(−g, +g)`, clamped to [0, 1]; one row-major unit
//!   draw per supported cell from the legacy scene RNG). Outside support:
//!   density is exactly 0.0 and no grain draw is consumed.
//! * Shaped support can add or remove cells relative to the pure B2
//!   ellipse, so the row-major grain stream resynchronizes after the first
//!   changed support cell (accepted consequence, not a bug): the contract
//!   remains exactly one legacy scene-RNG draw per supported cell, zero
//!   draws for unsupported cells, and bit-for-bit determinism for a given
//!   seed. No new grain RNG namespace is introduced.
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
//!   `[2, 6]`; the v2 body kernel is
//!   `I_body(dx, dy) = exp(−b_n·(r_ell/Re)^(1/n))`, where `r_ell` is the
//!   rotated elliptical radius from [`elliptical_radius`]; see
//!   [`sersic_body_profile`] for the chosen normalization and [`sersic_b`]
//!   for the `b_n` approximation.
//! * `core_softening_fraction` is the core-softening length as a fraction
//!   of Re (`rc = f·Re`); the core replaces the radius by
//!   `hypot(r, rc)` and re-centres the ratio so the centre stays at 1
//!   while the inner profile flattens — 0.0 means no core (bit-for-bit
//!   B3.1 body).
//! * `central_excess` is the amplitude (as a fraction of the peak
//!   density) of an additive radial bump with the fixed e-folding scale
//!   `0.20·Re` and exact kernel `exp(−(r / (0.20·Re))²)`, applied before
//!   peak normalization; 0.0 means none.
//! * `outer_halo_strength` is the halo amplitude relative to the body's
//!   characteristic surface brightness; `outer_halo_scale` is the halo radial
//!   scale as a multiple of Re.
//! * `isophote_shape` is the dimensionless amplitude `c` of the
//!   fourth-harmonic isophote shape
//!   `r_shape = r_ell / (1 + c·cos 4θ)`, where `θ = atan2(v, u)` is
//!   the intrinsic (deprojected) angular phase of
//!   [`elliptical_coordinates`]. Sign convention: positive = disky,
//!   negative = boxy, zero = pure B2 ellipse (bit-for-bit identical
//!   radius). Valid range `[-0.045, +0.045]`, so the denominator stays
//!   in `[0.955, 1.045]`.

use crate::density::DensityMap;
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

    /// Geometric-support multiplier of the family presentation contract.
    ///
    /// The v2 body is visible **iff** `r_ell <= k · Re`, where `r_ell` is
    /// [`elliptical_radius`] and `Re` is
    /// [`EllipticalGalaxyConfig::effective_radius`].
    ///
    /// `k` is a deterministic family contract constant — a presentation
    /// contract, not a measurement and not an RNG draw. It is intentionally never stored in
    /// [`EllipticalGalaxyConfig`]: the family alone determines it, and a
    /// per-scene draw would add a latent the seven-draw contract does not
    /// have.
    pub(crate) fn support_re_multiplier(self) -> f64 {
        match self {
            Self::CompactDisky => 1.75,
            Self::Classical => 1.40,
            Self::GiantBoxy => 1.00,
            Self::CdLike => 1.30,
        }
    }
}

/// Per-family parameter ranges of the v2 morphology contract.
///
/// With geometric family support (module docs) the visible extent is
/// `support_re_multiplier(family) · Re`, so the body ranges
/// (`axis_ratio`, `effective_radius`, `profile_index`) do not have to
/// keep a brightness isophote inside any occupancy band. `Re` remains the
/// (approximate) 2-D half-light radius per [`sersic_b`].
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

/// Immutable morphology prepared once per Elliptical scene.
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
    /// Core-softening length as a fraction of Re (`rc = f · Re`);
    /// 0.0 = no core (bit-for-bit B3.1 body).
    pub(crate) core_softening_fraction: f64,
    /// Central enhancement amplitude as a fraction of the peak density,
    /// added with the fixed 0.20·Re e-folding kernel before peak
    /// normalization; 0.0 = none.
    pub(crate) central_excess: f64,
    /// Outer halo amplitude relative to the body's characteristic brightness.
    pub(crate) outer_halo_strength: f64,
    /// Outer halo radial scale as a multiple of Re.
    pub(crate) outer_halo_scale: f64,
    /// Fourth-harmonic isophote-shape amplitude `c` (dimensionless):
    /// `r_shape = r_ell / (1 + c·cos 4θ)` in intrinsic (deprojected)
    /// coordinates. Positive = disky, negative = boxy, zero = pure B2
    /// ellipse. Valid range `[-0.045, +0.045]`.
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
// Pure body kernel (math only)
// ────────────────────────────────────────────────────────────────────

/// Sersic concentration parameter `b_n` for the v2 body kernel.
///
/// Returns the analytic approximation
///
/// ```text
/// b_n ≈ 2n − 1/3
/// ```
///
/// for a Sersic shape parameter `n` (dimensionless; v2 draws produce `n`
/// in `[2, 6]`).
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
/// `n` in `[2, 6]` (the v2 range).
pub(crate) fn sersic_b(profile_index: f64) -> f64 {
    2.0 * profile_index - 1.0 / 3.0
}

/// Intrinsic (deprojected) elliptical coordinates of a canvas point.
///
/// Single owner of the legacy rotation/deprojection math, shared by
/// [`elliptical_radius`] and [`elliptical_shaped_radius`]:
///
/// ```text
/// x_rot =  dx·cos(pa) + dy·sin(pa)
/// y_rot = −dx·sin(pa) + dy·cos(pa)
/// u = x_rot
/// v = y_rot / q
/// r_ell = hypot(u, v)
/// ```
///
/// `u` lies along the intrinsic major axis (unit semi-axis), `v` is the
/// deprojected minor-axis coordinate (the minor axis stretched to the
/// unit semi-axis by dividing by `q`), and `r_ell` is the B2 elliptical
/// radius. `r_ell = 0` iff the point is the exact canvas centre.
///
/// # Preconditions
/// `axis_ratio` in `(0, 1]`, `position_angle` in `[0, π)` (any finite
/// value works mathematically), `dx` and `dy` finite.
fn elliptical_coordinates(
    dx: f64,
    dy: f64,
    axis_ratio: f64,
    position_angle: f64,
) -> (f64, f64, f64) {
    debug_assert!(
        axis_ratio > 0.0 && axis_ratio <= 1.0,
        "axis_ratio must be in (0, 1]"
    );
    debug_assert!(dx.is_finite() && dy.is_finite(), "dx/dy must be finite");

    let cos_angle = position_angle.cos();
    let sin_angle = position_angle.sin();

    // Legacy rotation: major axis along x′, minor axis compressed by q
    // along y′ (see [`elliptical_radius`] doc for the exact axis
    // convention).
    let x_rot = dx * cos_angle + dy * sin_angle;
    let y_rot = -dx * sin_angle + dy * cos_angle;

    let u = x_rot;
    let v = y_rot / axis_ratio;
    (u, v, f64::hypot(u, v))
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
/// convention of the generated density map) belongs to
/// [`generate_elliptical_density`]; this helper is pure math on the
/// fractions themselves.
///
/// Axis convention (matches the legacy v1 generator in `src/engine.rs`
/// exactly — do not swap major/minor). The rotation/deprojection is
/// factored into [`elliptical_coordinates`], which is the single owner of
/// that math; [`elliptical_shaped_radius`] consumes the same intrinsic
/// coordinates, so the two radii can never disagree about the frame.
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
/// `axis_ratio` in `(0, 1]` (v2 draws produce `(0.55, 0.95)`),
/// `position_angle` in `[0, π)` (any finite value works mathematically),
/// `dx` and `dy` finite.
pub(crate) fn elliptical_radius(dx: f64, dy: f64, axis_ratio: f64, position_angle: f64) -> f64 {
    let (_, _, r_ell) = elliptical_coordinates(dx, dy, axis_ratio, position_angle);
    r_ell
}

/// Shaped elliptical radius: the B2 elliptical radius modulated by the
/// accepted fourth-harmonic isophote shape.
///
/// Definition (intrinsic/deprojected coordinates of
/// [`elliptical_coordinates`], `u` along the major axis, `v = y_rot/q`
/// with the minor axis stretched to the unit semi-axis):
///
/// ```text
/// r_shape = r_ell / (1 + c · cos 4θ),    θ = atan2(v, u)
/// ```
///
/// where `c = isophote_shape`. Production uses the branch-free identity
/// for `r_ell > 0`:
///
/// ```text
/// cos 4θ = (u⁴ − 6·u²·v² + v⁴) / (u² + v²)²
/// ```
///
/// At the exact centre (`r_ell = 0`): `r_shape = 0.0` exactly (the
/// angular phase is undefined there).
///
/// Sign convention:
///
/// * `c > 0`  => disky: at fixed `r_shape`, the contour extends farther
///   along the intrinsic axes (`r_ell = R·(1+c)` on the axes) and is
///   pinched along the intrinsic diagonals (`r_ell = R·(1−c)`);
/// * `c = 0`  => pure ellipse, bit-for-bit equal to
///   [`elliptical_radius`] (the denominator is exactly 1.0);
/// * `c < 0`  => boxy: the axis/diagonal relations are reversed.
///
/// The modulation is defined in the intrinsic (deprojected) frame; the
/// on-sky pattern inherits the galaxy rotation from `position_angle`
/// (rotating the point and the position angle by the same delta leaves
/// `r_shape` unchanged) and must not be read as on-sky 45° diagonals.
///
/// # Units and conventions
/// `dx`/`dy` in legacy canvas-fraction units (canvas half-extent = 0.5);
/// `r_shape` in the same units; `axis_ratio` in `(0, 1]`;
/// `position_angle` in radians `[0, π)`; `isophote_shape` in
/// `[-0.045, +0.045]`, so the denominator lies in `[0.955, 1.045]` and
/// `r_shape` is finite and non-negative for all finite inputs.
///
/// # Preconditions
/// Same as [`elliptical_radius`], plus `|isophote_shape| <= 0.045`.
/// Scalar in, scalar out; no RNG, no I/O, no mutation.
pub(crate) fn elliptical_shaped_radius(
    dx: f64,
    dy: f64,
    axis_ratio: f64,
    position_angle: f64,
    isophote_shape: f64,
) -> f64 {
    debug_assert!(
        isophote_shape.abs() <= 0.045,
        "isophote_shape must be in [-0.045, 0.045]"
    );

    let (u, v, r_ell) = elliptical_coordinates(dx, dy, axis_ratio, position_angle);
    if r_ell == 0.0 {
        return 0.0;
    }

    let u2 = u * u;
    let v2 = v * v;
    let r2 = u2 + v2;
    let cos4_theta = (u2 * u2 - 6.0 * u2 * v2 + v2 * v2) / (r2 * r2);
    let denominator = 1.0 + isophote_shape * cos4_theta;
    r_ell / denominator
}

/// Central-value-normalized Sersic-like body profile.
///
/// Chosen convention (fixed here; do not change it silently): the
/// **central-value-normalized** form
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
/// * supports every v2 `n` in `[2, 6]` and Re range.
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
/// Consumes only the four body parameters (`axis_ratio`,
/// `position_angle`, `effective_radius`, `profile_index`); the remaining
/// fields (`core_softening_fraction`, `central_excess`, `outer_halo_*`,
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

// ────────────────────────────────────────────────────────────────────
// Central structure (softened core + central excess, intensity only)
// ────────────────────────────────────────────────────────────────────

/// Radial e-folding scale of the central-excess kernel, as a fraction
/// of `Re`.
///
/// The accepted B3.2A central-excess kernel is
///
/// ```text
/// K(r) = exp(−(r / scale)²),   scale = CENTRAL_EXCESS_SCALE_FRACTION · Re
/// ```
///
/// `scale` is the kernel's e-folding scale, **not** a statistical
/// Gaussian sigma: `K(scale) = exp(−1)`, the half-maximum radius is
/// `scale·sqrt(ln 2) ≈ 0.16651·Re`, and the full width at half maximum
/// is `2·scale·sqrt(ln 2) ≈ 0.33302·Re`. (The 0.471·Re half-width
/// belongs to the different convention `exp(−r²/(2σ²))` and is not used
/// here.) It is a presentation-contract constant — never an RNG draw and
/// never stored in [`EllipticalGalaxyConfig`].
const CENTRAL_EXCESS_SCALE_FRACTION: f64 = 0.20;

/// Central-excess kernel, a pure function of the scaled radius
/// `x = r / scale`.
///
/// ```text
/// K(x) = exp(−x·x),    scale = CENTRAL_EXCESS_SCALE_FRACTION · Re
/// ```
///
/// Pinned contract (see `test_b32_excess_kernel_contract`):
///
/// * `K(0) == 1.0` exactly — the kernel peaks at the centre;
/// * `K(1) == exp(−1)` — the value at the e-folding scale `r = scale`;
/// * `K(sqrt(ln 2)) == 0.5` (to 1 ulp) — the half-maximum radius is
///   `scale·sqrt(ln 2) ≈ 0.16651·Re`;
/// * `0 < K(x)` and monotone non-increasing for `x ≥ 0` — no detached
///   component, no ring;
/// * full half-maximum width `2·scale·sqrt(ln 2) ≈ 0.33302·Re`.
///
/// `x` is dimensionless. Pure: no RNG, no I/O, no mutation.
fn central_excess_kernel(x: f64) -> f64 {
    (-x * x).exp()
}

/// Central structure: softened core plus central excess, applied to the
/// already-computed Sersic body intensity.
///
/// Consumes the shaped radius `r_shape` (already computed by
/// [`elliptical_shaped_radius`]) and changes **intensity only** — it never
/// rederives coordinates, never recalculates the isophote shape, and never
/// affects the geometric support decision.
///
/// ## Core (`f = core_softening_fraction`, `rc = f · Re`)
///
/// For `f == 0`: `I_core == body_intensity` bit-for-bit. For `f > 0`:
///
/// ```text
/// r_soft = hypot(r_shape, rc)
/// I_core = Sersic(r_soft, Re, n) / Sersic(rc, Re, n)
/// ```
///
/// (centre-normalized softened-radius form, [`sersic_body_profile`]
/// throughout):
///
/// * `I_core(0) == 1.0` exactly (`hypot(0, rc) == rc`, so the ratio is
///   1);
/// * `0 < I_core <= 1`, monotone non-increasing in `r_shape`;
/// * no hole, no ring; stronger `f` flattens the inner profile — the
///   profile value at every fixed `r > 0` rises toward the (unchanged)
///   centre value, so the central concentration decreases.
///
/// The division by `Sersic(rc, Re, n)` *is* the centre normalization;
/// no further normalization pass is added.
///
/// ## Excess (`e = central_excess`)
///
/// With `scale = CENTRAL_EXCESS_SCALE_FRACTION · Re` and
/// `K = central_excess_kernel(r_shape / scale)`:
///
/// ```text
/// I_final = (I_core + e · K) / (1 + e)
/// ```
///
/// * `e == 0`: returns `I_core` without the additive arithmetic;
/// * the centre stays exactly 1.0 (both terms are 1.0 at
///   `r_shape = 0`);
/// * `(0, 1]` inside support; the kernel peaks at the centre and decays
///   monotonically — no detached component, no ring;
/// * peak normalization by `(1 + e)` keeps the centre at 1 while the
///   outer profile is gently pulled down, sharpening the central
///   concentration.
///
/// # Units and conventions
/// `r_shape` and `effective_radius` in the same units (legacy
/// canvas-fraction units when composed with [`elliptical_shaped_radius`]);
/// `profile_index` the Sersic-like shape index; `f` a fraction of `Re`
/// (`f = 0` = no core); `e` a fraction of the peak density
/// (`e = 0` = none). Scalar in, scalar out; no RNG, no I/O, no mutation,
/// no support decision.
pub(crate) fn apply_central_structure(
    body_intensity: f64,
    r_shape: f64,
    effective_radius: f64,
    profile_index: f64,
    core_softening_fraction: f64,
    central_excess: f64,
) -> f64 {
    debug_assert!(
        r_shape >= 0.0
            && effective_radius > 0.0
            && profile_index > 0.0
            && core_softening_fraction >= 0.0
            && central_excess >= 0.0,
        "apply_central_structure preconditions"
    );

    let i_core = if core_softening_fraction == 0.0 {
        body_intensity
    } else {
        let rc = core_softening_fraction * effective_radius;
        let r_soft = f64::hypot(r_shape, rc);
        sersic_body_profile(r_soft, effective_radius, profile_index)
            / sersic_body_profile(rc, effective_radius, profile_index)
    };

    if central_excess == 0.0 {
        i_core
    } else {
        let scale = CENTRAL_EXCESS_SCALE_FRACTION * effective_radius;
        let x = r_shape / scale;
        (i_core + central_excess * central_excess_kernel(x)) / (1.0 + central_excess)
    }
}

// ────────────────────────────────────────────────────────────────────
// Elliptical scene generation (geometric support + grain)
// ────────────────────────────────────────────────────────────────────

/// Multiplicative local grain fraction of the Elliptical v2 grain
/// equation.
///
/// Elliptical v2 uses a multiplicative grain with a fixed 5% local
/// modulation: every geometrically supported cell is scaled by
/// `1 + U(−g, +g)` and the result is clamped to [0, 1]. The factor is
/// relative to the local Sersic intensity, so the grain amplitude stays
/// bounded relative to the local signal at every cell (it replaces the
/// legacy absolute additive ±0.012 grain). `g` is a
/// presentation-contract constant — never an RNG draw and never stored in
/// [`EllipticalGalaxyConfig`].
const ELLIPTICAL_GRAIN_FRACTION: f64 = 0.05;

/// Multiplicative grain equation, as a pure helper that keeps the RNG
/// mapping separate from the math.
///
/// One legacy unit draw `unit_draw ∈ [0, 1)` (a single
/// `StdRng::random::<f64>()` sample) is mapped to the local factor
///
/// ```text
/// factor = 1 + (2·unit_draw − 1) · fraction   ∈   1 + U(−fraction, +fraction)
/// value' = clamp(value · factor, 0, 1)
/// ```
///
/// `value` is the supported Sersic intensity in `(0, 1]`; `fraction` ≥ 0
/// is the grain amplitude in *local* (relative) units. For `fraction < 1`
/// and `value > 0` the factor is always positive, so the lower clamp is
/// inert in production and clamping can only bite at the upper bound
/// 1.0, when `value · factor > 1.0` (which implies `value ≥ 1/(1+g)`).
///
/// # Properties
/// * `unit_draw = 0.5` reproduces `value` exactly (factor 1.0);
/// * `abs(value' − value) ≤ fraction · value` — the grain can never
///   dominate the local signal, also when the upper clamp is active;
/// * `value' > 0` for `value > 0` (grain can never zero out a supported
///   cell).
///
/// Pure: no RNG, no I/O, no mutation; the caller owns draw order and
/// count (row-major, supported cells only).
pub(crate) fn apply_multiplicative_grain(value: f64, unit_draw: f64, fraction: f64) -> f64 {
    let factor = 1.0 + (2.0 * unit_draw - 1.0) * fraction;
    (value * factor).clamp(0.0_f64, 1.0_f64)
}

/// One Elliptical cell through the single per-cell seam.
///
/// Combines the three per-cell facts — rotated elliptical radius, geometric
/// support decision, Sersic intensity — from ONE [`elliptical_radius`]
/// evaluation, so the support boundary and the body profile can never be
/// computed from inconsistent radii.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EllipticalCell {
    /// `true` iff the cell is inside the family's geometric support:
    /// `r_shape <= support_re_multiplier(family) · Re` (boundary
    /// inclusive), with `r_shape` from [`elliptical_shaped_radius`].
    /// Independent of the central-structure fields.
    pub(crate) inside_support: bool,
    /// Central-structure-applied Sersic body intensity in `(0, 1]` when
    /// `inside_support` (exactly 1.0 at the canvas centre), exactly 0.0
    /// otherwise.
    pub(crate) intensity: f64,
}

/// Per-cell seam: shaped elliptical radius → geometric support
/// decision → Sersic body intensity → central structure, all from a
/// single shaped-radius evaluation.
///
/// The support is **geometric** and independent of the Sersic amplitude:
///
/// ```text
/// r_shape        =  elliptical_shaped_radius(dx, dy, q, pa, c)
/// inside_support  iff  r_shape <= k(family) · Re
/// intensity       =  apply_central_structure(              (inside support)
///                        sersic_body_profile(r_shape, Re, n),
///                        r_shape, Re, n, f, e)
///                   0.0                                     (outside support)
/// ```
///
/// with `k(family) = [`EllipticalFamily::support_re_multiplier`]` and the
/// boundary inclusive (`<=`) per the geometric support contract. The
/// shaped radius is computed exactly once and feeds **both** the support
/// decision and the Sersic intensity (and, through
/// [`apply_central_structure`], the central structure), so the three can
/// never be evaluated from inconsistent radii. The central structure
/// changes intensity only — the support decision is made from `r_shape`
/// alone and is independent of `f` and `e`. Because the Sersic body is
/// strictly positive for every finite radius, the softened core keeps
/// `I_core` in `(0, 1]` (ratio of positives), the excess kernel is
/// strictly positive, and `I_final = (I_core + e·K)/(1 + e)` stays in
/// `(0, 1]` — so the map built from this seam satisfies the invariant
///
/// ```text
/// cell intensity > 0.0  iff  inside_support
/// ```
///
/// which [`generate_elliptical_density`] relies on to spend exactly one
/// grain draw, in row-major order, per supported cell.
///
/// # Units and conventions
/// `dx`/`dy` in legacy canvas-fraction units (canvas half-extent = 0.5);
/// `Re` in the same units; `k` dimensionless. Scalar in, scalar out; no
/// RNG, no I/O, no mutation.
pub(crate) fn elliptical_cell(dx: f64, dy: f64, config: EllipticalGalaxyConfig) -> EllipticalCell {
    let r_shape = elliptical_shaped_radius(
        dx,
        dy,
        config.axis_ratio,
        config.position_angle,
        config.isophote_shape,
    );
    let support_limit = config.family.support_re_multiplier() * config.effective_radius;

    if r_shape <= support_limit {
        let body_intensity =
            sersic_body_profile(r_shape, config.effective_radius, config.profile_index);
        let intensity = apply_central_structure(
            body_intensity,
            r_shape,
            config.effective_radius,
            config.profile_index,
            config.core_softening_fraction,
            config.central_excess,
        );
        EllipticalCell {
            inside_support: true,
            intensity,
        }
    } else {
        EllipticalCell {
            inside_support: false,
            intensity: 0.0,
        }
    }
}

/// Pure per-scene Sersic body map after geometric support: the per-cell
/// seam [`elliptical_cell`] evaluated at every cell of a `width` ×
/// `height` canvas.
///
/// Coordinate convention (identical to the legacy v1 generator):
///
/// ```text
/// dx = (x − width/2) / width
/// dy = (y − height/2) / height
/// ```
///
/// where `height` is the generated density-map height (2× terminal height
/// in the ordinary half-block path). The canvas therefore spans
/// `dx, dy ∈ [−0.5, 0.5)` with the origin at the centre, `+x` to the right,
/// and no width/height aspect distortion is introduced.
///
/// `config` is consumed by value: the caller derives it exactly once per
/// scene (see [`EllipticalGalaxyConfig::from_context`]) and this function
/// performs no re-derivation and touches no RNG. This is the
/// **post-geometric-support, pre-render-normalization** body: cells inside
/// family support carry the central-structure-applied Sersic intensity in
/// `(0, 1]` (exactly 1.0 at the canvas centre), cells outside support
/// carry exactly 0.0 — the raw-positive-support diagnostic is the
/// positive-cell fraction of this map.
///
/// # Side effects
/// None (pure allocation of the returned [`DensityMap`]).
pub(crate) fn elliptical_density_profile(
    width: usize,
    height: usize,
    config: EllipticalGalaxyConfig,
) -> DensityMap {
    DensityMap::from_fn(width, height, |x, y| {
        let dx = (x as f64 - width as f64 / 2.0) / width as f64;
        let dy = (y as f64 - height as f64 / 2.0) / height as f64;
        elliptical_cell(dx, dy, config).intensity
    })
}

/// Generates the Elliptical v2 density map for one scene.
///
/// The Sersic body with geometric family support replaces the legacy
/// fixed double-Gaussian body:
///
/// * **Morphology**: [`EllipticalGalaxyConfig::from_context`] is called
///   exactly once per scene, from the versioned
///   `elliptical/morphology/v2` feature namespace. The body equation
///   consumes `axis_ratio`, `position_angle`, `effective_radius`,
///   `profile_index` and `isophote_shape` (the latter through
///   [`elliptical_shaped_radius`], which also sets the geometric support);
///   `core_softening_fraction` and `central_excess` are applied by the
///   per-cell seam through [`apply_central_structure`] (intensity only,
///   never support); the halo fields (`outer_halo_*`) stay frozen and are
///   deliberately unused here.
/// * **Per-pixel math**: [`elliptical_density_profile`] (the per-cell
///   seam [`elliptical_cell`] with the legacy canvas-fraction convention);
///   `height` is the render height (2× terminal height in the half-block
///   path).
/// * **Post-profile**: the multiplicative local grain
///   ([`apply_multiplicative_grain`] at the fixed 5% local-modulation
///   fraction [`ELLIPTICAL_GRAIN_FRACTION`], one unit draw in row-major
///   order per supported cell) replaces the legacy absolute additive
///   ±0.012 grain;
///   the legacy `0.018` brightness cutoff is retired and replaced by the
///   geometric family support (`r_shape <= k(family) · Re`, see
///   [`elliptical_cell`]). The result stays bounded in `[0, 1]` because
///   the central-value-normalized body profile is in `(0, 1]` and the
///   helper clamps to `[0, 1]`.
///
/// # RNG contract
/// The morphology config is **never** drawn from the legacy scene RNG:
/// it comes from `context.feature_seed(ELLIPTICAL_MORPHOLOGY_V2)`. The
/// only consumer of `rng` (the legacy scene RNG created in
/// `ArtModel::generate_density`) is the per-supported-cell grain unit
/// draw (`rng.random::<f64>()`), in row-major order. Grain draws occur
/// **only** for cells inside geometric support; outside support the
/// density is exactly 0.0 and no draw is consumed — no dummy or
/// discarded draws are inserted.
///
/// # Side effects
/// Returns the filled [`DensityMap`]; mutates only `rng`'s internal state
/// via the grain draws.
pub(crate) fn generate_elliptical_density(
    width: usize,
    height: usize,
    context: GenerationContext,
    rng: &mut StdRng,
) -> DensityMap {
    let config = EllipticalGalaxyConfig::from_context(context);
    let mut map = elliptical_density_profile(width, height, config);

    for y in 0..height {
        for x in 0..width {
            let value = map.get(x, y);

            // Positive iff inside geometric support (invariant of
            // `elliptical_cell`): supported cells draw one grain in
            // row-major order; unsupported cells stay exactly 0.0 and
            // consume no RNG.
            let value = if value > 0.0 {
                apply_multiplicative_grain(value, rng.random::<f64>(), ELLIPTICAL_GRAIN_FRACTION)
            } else {
                0.0
            };

            map.set(x, y, value);
        }
    }

    map
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

    /// Diagnostic seed panel (not a golden test).
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

    // ── Pure body kernel ─────────────────────────────────────

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

    // ── Scene generation integration ─────────────────────────

    use crate::engine::ArtModel;
    use crate::render::{prepare_density, PreparedDensity, RenderProfile};

    /// Elliptical generation is deterministic per seed, different seeds
    /// still differ, and the density-map geometry is unchanged
    /// (W × 2·H), checked on the canonical 40×20 scene.
    #[test]
    fn test_b22_generation_deterministic_and_seeded() {
        for seed in [0_u64, 1, 7, 42, 137, 2026] {
            let a = ArtModel::Elliptical.generate_scene(40, 20, Some(seed));
            let b = ArtModel::Elliptical.generate_scene(40, 20, Some(seed));
            assert_eq!(a.density, b.density, "seed {seed} must be deterministic");
            assert_eq!(
                (a.density.width, a.density.height),
                (40, 40),
                "seed {seed}: density geometry changed"
            );
        }

        let d42 = ArtModel::Elliptical
            .generate_scene(40, 20, Some(42))
            .density;
        let d43 = ArtModel::Elliptical
            .generate_scene(40, 20, Some(43))
            .density;
        assert_ne!(d42, d43, "different seeds must still differ");
    }

    /// Every generated density value is finite, non-negative and bounded
    /// by 1.0.
    #[test]
    fn test_b22_all_values_finite_nonnegative_bounded() {
        for seed in [0_u64, 1, 7, 42, 137, 2026] {
            let scene = ArtModel::Elliptical.generate_scene(40, 20, Some(seed));
            for (idx, &value) in scene.density.data.iter().enumerate() {
                assert!(
                    value.is_finite() && (0.0_f64..=1.0).contains(&value),
                    "seed {seed} cell {idx}: {value}"
                );
            }
        }
    }

    /// The body has positive (but not full) support and the
    /// central-value-normalized kernel keeps the canvas centre as the
    /// strictly brightest cell.
    #[test]
    fn test_b22_body_positive_support_and_central_structure() {
        for seed in [0_u64, 1, 7, 42, 137, 2026] {
            let scene = ArtModel::Elliptical.generate_scene(40, 20, Some(seed));
            let map = &scene.density;
            let total = map.width * map.height;
            let visible = map.data.iter().filter(|&&v| v > 0.0).count();
            assert!(visible > 0, "seed {seed}: body collapsed to empty");
            assert!(visible < total, "seed {seed}: canvas fully filled");

            // Centre pixel: dx = dy = 0 → profile exactly 1.0 (≥ 1 − g
            // after the multiplicative grain at the fixed grain
            // fraction). The nearest neighbour stays ≤ ~0.137 in the
            // most diffuse corner case (q = 0.55, Re = 0.20, n = 2 on the
            // canonical 40×40 grid), far below the centre, so the centre
            // remains the unique global maximum.
            let (cx, cy) = (map.width / 2, map.height / 2);
            let center = map.get(cx, cy);
            assert!(
                center >= 1.0 - ELLIPTICAL_GRAIN_FRACTION - 1.0e-12,
                "seed {seed}: centre {center} lost its peak"
            );
            assert!(
                map.data.iter().all(|&v| v <= center + 1.0e-12),
                "seed {seed}: a cell is brighter than the centre"
            );
        }
    }

    /// The pure profile stage equals the per-cell seam
    /// [`elliptical_cell`] (geometric support + Sersic kernel) evaluated
    /// with ONE config derived per scene (via the same `for_scene_seed`
    /// path), recomputed for several seeds and several canvas sizes (odd
    /// included) — so neither per-pixel nor size-dependent re-derivation
    /// is possible.
    #[test]
    fn test_b22_profile_is_scene_config_times_kernel() {
        for seed in [0_u64, 42, 137, 2026] {
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            for (width, render_height) in [(40usize, 40usize), (33, 34), (50, 42)] {
                let profile = elliptical_density_profile(width, render_height, config);
                for y in 0..render_height {
                    for x in 0..width {
                        let dx = (x as f64 - width as f64 / 2.0) / width as f64;
                        let dy = (y as f64 - render_height as f64 / 2.0) / render_height as f64;
                        let expected = elliptical_cell(dx, dy, config).intensity;
                        assert_eq!(profile.get(x, y), expected, "seed {seed} ({x},{y})");
                    }
                }
            }
        }
    }

    /// Cells inside geometric support equal the pre-grain intensity
    /// (the Sersic body at the *shaped* radius, passed through the
    /// central structure — the `elliptical_cell` intensity) times a
    /// multiplicative grain factor in `[1 − g, 1 + g]`, clamped to [0,
    /// 1]; cells outside support stay exactly 0.0 (asserted by
    /// `test_b22f_cells_outside_geometric_support_are_exactly_zero`).
    #[test]
    fn test_b22h_cells_inside_support_are_sersic_times_bounded_factor() {
        let g = ELLIPTICAL_GRAIN_FRACTION;
        for seed in [0_u64, 7, 42, 2026] {
            let (width, terminal_height) = (40usize, 20usize);
            let render_height = terminal_height * 2;
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            let k = config.family.support_re_multiplier();
            let scene = ArtModel::Elliptical.generate_scene(width, terminal_height, Some(seed));
            assert_eq!(scene.density.width, width);
            assert_eq!(scene.density.height, render_height);
            let mut inside = 0usize;
            for y in 0..render_height {
                for x in 0..width {
                    let dx = (x as f64 - width as f64 / 2.0) / width as f64;
                    let dy = (y as f64 - render_height as f64 / 2.0) / render_height as f64;
                    let r_shape = elliptical_shaped_radius(
                        dx,
                        dy,
                        config.axis_ratio,
                        config.position_angle,
                        config.isophote_shape,
                    );
                    if r_shape <= k * config.effective_radius {
                        inside += 1;
                        let p = elliptical_cell(dx, dy, config).intensity;
                        let v = scene.density.get(x, y);
                        assert!(
                            v >= p * (1.0 - g) - 1.0e-12
                                && v <= (p * (1.0 + g)).min(1.0) + 1.0e-12,
                            "seed {seed} ({x},{y}): multiplicative grain out of bounds (profile {p}, got {v})"
                        );
                    }
                }
            }
            assert!(inside > 0, "seed {seed}: support collapsed to empty");
        }
    }

    // ── Multiplicative local grain ───────────────────────────

    /// Grain equation on the pure helper: unit-draw → factor mapping,
    /// exact factor application, and clamping only at the upper bound
    /// where necessary.
    #[test]
    fn test_b22h_helper_equation_and_upper_clamp() {
        for g in [0.03_f64, 0.05, 0.08] {
            // unit_draw 0.5 → factor exactly 1 → value reproduced exactly.
            for v in [1.0_f64, 0.5, 1.0e-6] {
                assert_eq!(apply_multiplicative_grain(v, 0.5, g), v);
            }
            // factor endpoints: unit 0 → 1 − g, unit 1 → 1 + g.
            for v in [1.0_f64, 0.4, 0.02] {
                assert_eq!(apply_multiplicative_grain(v, 0.0, g), v * (1.0 - g));
                assert_eq!(
                    apply_multiplicative_grain(v, 1.0, g),
                    (v * (1.0 + g)).min(1.0)
                );
            }
            // Upper clamp bites only when v · (1 + g) > 1.
            assert_eq!(apply_multiplicative_grain(1.0, 1.0, g), 1.0);
            assert_eq!(apply_multiplicative_grain(1.0, 0.5, g), 1.0);
            assert!(apply_multiplicative_grain(0.9, 1.0, g) < 1.0);
            // Lower clamp is inert for positive values with g < 1.
            assert!(apply_multiplicative_grain(1.0e-12, 0.0, g) > 0.0);
        }
    }

    /// The grain can never dominate the local signal —
    /// `abs(value' − value) ≤ g · value` at every supported cell, also
    /// where the upper clamp is active (clamping at 1.0 implies
    /// `value ≥ 1/(1+g)`, which makes `1 − value ≤ g · value` hold).
    #[test]
    fn test_b22h_grain_never_dominates_local_signal() {
        let g = ELLIPTICAL_GRAIN_FRACTION;
        for seed in [0_u64, 1, 7, 42, 137, 2026] {
            let (width, terminal_height) = (40usize, 20usize);
            let render_height = terminal_height * 2;
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            let profile = elliptical_density_profile(width, render_height, config);
            let scene = ArtModel::Elliptical
                .generate_scene(width, terminal_height, Some(seed))
                .density;
            let mut checked = 0usize;
            for y in 0..render_height {
                for x in 0..width {
                    let p = profile.get(x, y);
                    if p > 0.0 {
                        checked += 1;
                        let v = scene.get(x, y);
                        assert!(
                            (v - p).abs() <= g * p + 1.0e-12,
                            "seed {seed} ({x},{y}): grain dominated the local signal (profile {p}, got {v})"
                        );
                    }
                }
            }
            assert!(checked > 0, "seed {seed}: no supported cells");
        }
    }

    /// Grain can never zero out a supported cell (all supported cells
    /// stay strictly positive) and the geometric (shaped) support is
    /// unchanged by the grain (scene support == config shaped support,
    /// unsupported cells exactly 0.0).
    #[test]
    fn test_b22h_supported_cells_stay_positive_and_support_unchanged() {
        for seed in [0_u64, 1, 2, 5, 7, 13, 42, 64, 99, 137, 2026] {
            let (width, terminal_height) = (40usize, 20usize);
            let render_height = terminal_height * 2;
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            let k = config.family.support_re_multiplier();
            let scene = ArtModel::Elliptical.generate_scene(width, terminal_height, Some(seed));
            let mut inside = 0usize;
            for y in 0..render_height {
                for x in 0..width {
                    let dx = (x as f64 - width as f64 / 2.0) / width as f64;
                    let dy = (y as f64 - render_height as f64 / 2.0) / render_height as f64;
                    let r_shape = elliptical_shaped_radius(
                        dx,
                        dy,
                        config.axis_ratio,
                        config.position_angle,
                        config.isophote_shape,
                    );
                    let v = scene.density.get(x, y);
                    if r_shape <= k * config.effective_radius {
                        inside += 1;
                        assert!(
                            v > 0.0,
                            "seed {seed} ({x},{y}): supported cell zeroed by grain"
                        );
                    } else {
                        assert_eq!(
                            v, 0.0,
                            "seed {seed} ({x},{y}): unsupported cell must stay 0"
                        );
                    }
                }
            }
            assert!(inside > 0, "seed {seed}: support collapsed to empty");
        }
    }

    /// Same seed stays deterministic; exactly ONE unit draw is consumed
    /// from the legacy scene RNG per supported cell, in row-major order;
    /// unsupported cells consume no draw; the config never draws from
    /// the scene RNG.
    ///
    /// Proof strategy: rebuild the scene from the pure profile plus a
    /// fresh `StdRng::seed_from_u64(seed)` — the exact legacy scene-RNG
    /// construction of `ArtModel::generate_density` — drawing one unit
    /// sample per supported cell in row-major order; every scene cell
    /// must then be bit-equal. Any extra (dummy) draw, skipped draw, or
    /// config draw on the scene RNG would desynchronize the draw sequence
    /// and break the bit-equality.
    #[test]
    fn test_b22h_exactly_one_draw_per_supported_cell_row_major() {
        for seed in [0_u64, 1, 2, 5, 13, 42, 64, 99, 137, 2026] {
            let (width, terminal_height) = (40usize, 20usize);
            let render_height = terminal_height * 2;
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            let profile = elliptical_density_profile(width, render_height, config);
            let scene = ArtModel::Elliptical
                .generate_scene(width, terminal_height, Some(seed))
                .density;

            let mut rng = StdRng::seed_from_u64(seed);
            let mut draws = 0usize;
            for y in 0..render_height {
                for x in 0..width {
                    let p = profile.get(x, y);
                    let v = scene.get(x, y);
                    if p > 0.0 {
                        draws += 1;
                        let expected = apply_multiplicative_grain(
                            p,
                            rng.random::<f64>(),
                            ELLIPTICAL_GRAIN_FRACTION,
                        );
                        assert_eq!(
                            v, expected,
                            "seed {seed} ({x},{y}): grain draw stream desynchronized (got {v}, expected {expected})"
                        );
                    } else {
                        assert_eq!(
                            v, 0.0,
                            "seed {seed} ({x},{y}): unsupported cell must stay 0"
                        );
                    }
                }
            }
            assert!(draws > 0, "seed {seed}: no supported cells");

            // Determinism: a second generation from the same seed
            // matches bit-for-bit.
            let scene_again = ArtModel::Elliptical
                .generate_scene(width, terminal_height, Some(seed))
                .density;
            assert_eq!(scene, scene_again, "seed {seed} must be deterministic");
        }
    }

    /// The Elliptical `RenderProfile` is frozen — it must not tune
    /// normalization/stretch/threshold to the new morphology
    fn test_b22_elliptical_render_profile_frozen() {
        let profile = RenderProfile::for_model(ArtModel::Elliptical);
        assert_eq!(
            format!("{profile:?}"),
            "RenderProfile { normalization: Robust { low_percentile: 0.02, high_percentile: 0.98 }, stretch: Gamma(0.7), threshold: TargetOccupancy(0.23), preparation: Galaxy }"
        );
    }

    // ── Geometric support ─────────────────────────

    /// All four family support multipliers are pinned.
    #[test]
    fn test_b22f_support_re_multiplier_pinned_per_family() {
        assert_eq!(EllipticalFamily::CompactDisky.support_re_multiplier(), 1.75);
        assert_eq!(EllipticalFamily::Classical.support_re_multiplier(), 1.40);
        assert_eq!(EllipticalFamily::GiantBoxy.support_re_multiplier(), 1.00);
        assert_eq!(EllipticalFamily::CdLike.support_re_multiplier(), 1.30);
    }

    /// The seven-draw config order and its deterministic anchors: the full
    /// config of pinned seeds is bit-for-bit reproducible. The
    /// feature-seed anchors themselves remain pinned in `src/seed.rs`.
    #[test]
    fn test_b22f_config_draw_order_and_anchors_unchanged() {
        const ANCHORS: [(u64, &str); 7] = [
            (
                0,
                "EllipticalGalaxyConfig { family: Classical, axis_ratio: 0.7922141726904416, position_angle: 2.7349944778684883, effective_radius: 0.31714324884824063, profile_index: 3.7857208141373446, core_softening_fraction: 0.07963755080049664, central_excess: 0.020362449199503363, outer_halo_strength: 0.06804772495101992, outer_halo_scale: 2.5670643745918325, isophote_shape: 0.01952848621465633 }",
            ),
            (
                1,
                "EllipticalGalaxyConfig { family: CdLike, axis_ratio: 0.7187286785842384, position_angle: 0.3612497561493311, effective_radius: 0.2690225770773009, profile_index: 3.080451541546018, core_softening_fraction: 0.014506915655063079, central_excess: 0.0, outer_halo_strength: 0.26480373577801164, outer_halo_scale: 4.296074715560233, isophote_shape: -0.00229782971640426 }",
            ),
            (
                7,
                "EllipticalGalaxyConfig { family: CdLike, axis_ratio: 0.8716400230786168, position_angle: 1.8732514003598573, effective_radius: 0.292127286822845, profile_index: 3.5425457364569, core_softening_fraction: 0.07196528044769114, central_excess: 0.0, outer_halo_strength: 0.3908099722988442, outer_halo_scale: 6.816199445976883, isophote_shape: -0.0004682081698133972 }",
            ),
            (
                42,
                "EllipticalGalaxyConfig { family: CompactDisky, axis_ratio: 0.6979070049426946, position_angle: 2.6080678795592602, effective_radius: 0.2480912889700056, profile_index: 2.721369334550084, core_softening_fraction: 0.0, central_excess: 0.11320637748671084, outer_halo_strength: 0.021174744082507982, outer_halo_scale: 1.7646843010313498, isophote_shape: 0.020024323438672137 }",
            ),
            (
                137,
                "EllipticalGalaxyConfig { family: CompactDisky, axis_ratio: 0.6089154479330066, position_angle: 1.647669306451919, effective_radius: 0.2716019400455035, profile_index: 3.0740291006825524, core_softening_fraction: 0.0, central_excess: 0.03926587968957436, outer_halo_strength: 0.0013602622613769634, outer_halo_scale: 1.517003278267212, isophote_shape: 0.013310737681865161 }",
            ),
            (
                2026,
                "EllipticalGalaxyConfig { family: CdLike, axis_ratio: 0.8758507651335193, position_angle: 0.22797724168566566, effective_radius: 0.3044651119524691, profile_index: 3.7893022390493822, core_softening_fraction: 0.019105624377492202, central_excess: 0.0, outer_halo_strength: 0.3840641724222741, outer_halo_scale: 6.681283448445481, isophote_shape: -0.019336383537374803 }",
            ),
            (
                4095,
                "EllipticalGalaxyConfig { family: CompactDisky, axis_ratio: 0.6300084683581262, position_angle: 0.789710790860876, effective_radius: 0.23182756909420327, profile_index: 2.477413536413049, core_softening_fraction: 0.0, central_excess: 0.053620597323756194, outer_halo_strength: 0.0446990875181028, outer_halo_scale: 2.058738593976285, isophote_shape: 0.03920249948055765 }",
            ),
        ];
        for (seed, expected) in ANCHORS {
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            assert_eq!(
                format!("{config:?}"),
                expected,
                "seed {seed} config drifted from the B1.2 anchor"
            );
        }
    }

    /// The v2 body ranges (q, Re, n) are frozen exactly.
    #[test]
    fn test_b22f_b12_body_ranges_restored_exactly() {
        fn expected_ranges(family: EllipticalFamily) -> ((f64, f64), (f64, f64), (f64, f64)) {
            match family {
                EllipticalFamily::CompactDisky => ((0.55, 0.80), (0.20, 0.30), (2.0, 3.5)),
                EllipticalFamily::Classical => ((0.62, 0.88), (0.24, 0.36), (2.5, 4.5)),
                EllipticalFamily::GiantBoxy => ((0.72, 0.95), (0.30, 0.44), (4.0, 6.0)),
                EllipticalFamily::CdLike => ((0.65, 0.90), (0.24, 0.34), (2.5, 4.5)),
            }
        }
        for family in [
            EllipticalFamily::CompactDisky,
            EllipticalFamily::Classical,
            EllipticalFamily::GiantBoxy,
            EllipticalFamily::CdLike,
        ] {
            let ranges = FamilyRanges::for_family(family);
            let (q, re, n) = expected_ranges(family);
            assert_eq!(ranges.axis_ratio, q, "{family:?} axis_ratio range changed");
            assert_eq!(
                ranges.effective_radius, re,
                "{family:?} effective_radius range changed"
            );
            assert_eq!(
                ranges.profile_index, n,
                "{family:?} profile_index range changed"
            );
        }
    }

    /// Every cell outside the shaped support `k(family) · Re` is
    /// exactly 0.0 in both the pure profile map and the generated scene.
    /// (B3.1: the support boundary is the *shaped* radius, so the
    /// outside test must use `r_shape`, not the pure `r_ell`.)
    #[test]
    fn test_b22f_cells_outside_geometric_support_are_exactly_zero() {
        for seed in [0_u64, 7, 42, 137, 2026, 4095] {
            let (width, render_height) = (40usize, 40usize);
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            let k = config.family.support_re_multiplier();
            let profile = elliptical_density_profile(width, render_height, config);
            let scene = ArtModel::Elliptical
                .generate_scene(width, render_height / 2, Some(seed))
                .density;
            let mut outside = 0usize;
            for y in 0..render_height {
                for x in 0..width {
                    let dx = (x as f64 - width as f64 / 2.0) / width as f64;
                    let dy = (y as f64 - render_height as f64 / 2.0) / render_height as f64;
                    let r_shape = elliptical_shaped_radius(
                        dx,
                        dy,
                        config.axis_ratio,
                        config.position_angle,
                        config.isophote_shape,
                    );
                    if r_shape > k * config.effective_radius {
                        outside += 1;
                        assert_eq!(
                            profile.get(x, y),
                            0.0,
                            "seed {seed} ({x},{y}): profile outside support must be 0"
                        );
                        assert_eq!(
                            scene.get(x, y),
                            0.0,
                            "seed {seed} ({x},{y}): scene outside support must be 0"
                        );
                    }
                }
            }
            // The 40×40 canvas corners sit at canvas distance ≥
            // 0.475·√2, so r_shape(corner) ≥ 0.475·√2/1.045 ≈ 0.64
            // while k·Re < 0.525: at least the corners are always outside.
            assert!(outside > 0, "seed {seed}: expected outside-support cells");
        }
    }

    /// The support boundary depends on q, PA, Re, the family multiplier
    /// and the isophote shape — never on the Sersic amplitude
    /// (profile index `n`), which changes concentration without defining
    /// support. (B3.1: the boundary is the *shaped* radius, so with
    /// `isophote_shape ≠ 0` it is `r_shape <= k·Re`, not `r_ell`.)
    #[test]
    fn test_b22f_support_boundary_independent_of_profile_index() {
        let base = EllipticalGalaxyConfig {
            family: EllipticalFamily::CompactDisky,
            axis_ratio: 0.63,
            position_angle: 1.07,
            effective_radius: 0.24,
            profile_index: 2.0,
            core_softening_fraction: 0.0,
            central_excess: 0.0,
            outer_halo_strength: 0.0,
            outer_halo_scale: 1.0,
            isophote_shape: 0.0,
        };
        // B2 corner: with isophote_shape = 0 the shaped radius is
        // bit-for-bit the pure elliptical radius.
        for n_hi in [3.0_f64, 4.5, 6.0] {
            let hi = EllipticalGalaxyConfig {
                profile_index: n_hi,
                ..base
            };
            let k = base.family.support_re_multiplier();
            for gy in 0..41usize {
                for gx in 0..41usize {
                    let dx = -0.5 + gx as f64 * 0.025;
                    let dy = -0.5 + gy as f64 * 0.025;
                    let lo = elliptical_cell(dx, dy, base);
                    let hi_cell = elliptical_cell(dx, dy, hi);
                    assert_eq!(
                        lo.inside_support, hi_cell.inside_support,
                        "support differs at ({dx},{dy}) between n=2.0 and n={n_hi}"
                    );
                    let r_shape = elliptical_shaped_radius(
                        dx,
                        dy,
                        base.axis_ratio,
                        base.position_angle,
                        base.isophote_shape,
                    );
                    assert_eq!(
                        lo.inside_support,
                        r_shape <= k * base.effective_radius,
                        "boundary must be r_shape <= k·Re at ({dx},{dy})"
                    );
                }
            }
        }

        // B3.1 corners: with a nonzero isophote shape the same
        // n-independence must hold for the shaped boundary.
        for c in [0.03_f64, -0.045] {
            let shaped_base = EllipticalGalaxyConfig {
                isophote_shape: c,
                ..base
            };
            for n_hi in [3.0_f64, 4.5, 6.0] {
                let hi = EllipticalGalaxyConfig {
                    profile_index: n_hi,
                    ..shaped_base
                };
                let k = shaped_base.family.support_re_multiplier();
                for gy in 0..41usize {
                    for gx in 0..41usize {
                        let dx = -0.5 + gx as f64 * 0.025;
                        let dy = -0.5 + gy as f64 * 0.025;
                        let lo = elliptical_cell(dx, dy, shaped_base);
                        let hi_cell = elliptical_cell(dx, dy, hi);
                        assert_eq!(
                            lo.inside_support, hi_cell.inside_support,
                            "c={c}: support differs at ({dx},{dy}) between n=2.0 and n={n_hi}"
                        );
                        let r_shape = elliptical_shaped_radius(
                            dx,
                            dy,
                            shaped_base.axis_ratio,
                            shaped_base.position_angle,
                            c,
                        );
                        assert_eq!(
                            lo.inside_support,
                            r_shape <= k * shaped_base.effective_radius,
                            "c={c}: boundary must be r_shape <= k·Re at ({dx},{dy})"
                        );
                    }
                }
            }
        }
    }

    /// The positive-iff-supported invariant the grain pass relies on,
    /// verified at every family range corner — including the
    /// `isophote_shape` corners, which B3.1 made part of the support —
    /// on a 40×40 canvas. (B3.1: the support boundary is the *shaped*
    /// radius `r_shape <= k·Re`, not the pure `r_ell`.)
    #[test]
    fn test_b22f_profile_positive_iff_inside_support() {
        for family in [
            EllipticalFamily::CompactDisky,
            EllipticalFamily::Classical,
            EllipticalFamily::GiantBoxy,
            EllipticalFamily::CdLike,
        ] {
            let ranges = FamilyRanges::for_family(family);
            for q in [ranges.axis_ratio.0, ranges.axis_ratio.1] {
                for re in [ranges.effective_radius.0, ranges.effective_radius.1] {
                    for n in [ranges.profile_index.0, ranges.profile_index.1] {
                        for pa in [0.0_f64, 1.1, std::f64::consts::PI - 1.0e-9] {
                            for c in [0.0_f64, ranges.isophote_shape.0, ranges.isophote_shape.1] {
                                let config = EllipticalGalaxyConfig {
                                    family,
                                    axis_ratio: q,
                                    position_angle: pa,
                                    effective_radius: re,
                                    profile_index: n,
                                    core_softening_fraction: 0.0,
                                    central_excess: 0.0,
                                    outer_halo_strength: 0.0,
                                    outer_halo_scale: 1.0,
                                    isophote_shape: c,
                                };
                                let profile = elliptical_density_profile(40, 40, config);
                                let k = family.support_re_multiplier();
                                let mut inside = 0usize;
                                let mut outside = 0usize;
                                for y in 0..40usize {
                                    for x in 0..40usize {
                                        let dx = (x as f64 - 20.0) / 40.0;
                                        let dy = (y as f64 - 20.0) / 40.0;
                                        let r_shape = elliptical_shaped_radius(dx, dy, q, pa, c);
                                        let v = profile.get(x, y);
                                        assert!(
                                            v.is_finite() && (0.0_f64..=1.0).contains(&v),
                                            "corner value {v} at ({x},{y})"
                                        );
                                        if r_shape <= k * re {
                                            inside += 1;
                                            assert!(v > 0.0, "inside support must be positive");
                                        } else {
                                            outside += 1;
                                            assert_eq!(v, 0.0, "outside support must be exactly 0");
                                        }
                                    }
                                }
                                assert!(inside > 0 && outside > 0, "degenerate corner");
                            }
                        }
                    }
                }
            }
        }
    }

    /// Raw-support population invariant (morphology diagnostic): over a
    /// broad deterministic population (4096 sequential base seeds)
    /// the *raw positive support* — post-geometric-support,
    /// pre-render-normalization positive-cell fraction of the 40×40 body
    /// map — stays far above pathological near-empty values.
    ///
    /// This is a documented lower bound on the discrete grid, not an
    /// occupancy objective: no scene is required to sit near any specific
    /// fraction (the renderer's 23% target is NOT a morphology goal), and
    /// the bound is deliberately broad — it only catches a near-empty
    /// morphology collapse.
    #[allow(clippy::print_literal)]
    #[test]
    fn test_b22f_raw_support_population_bounded_below() {
        const GRID: usize = 40;
        const SEEDS: u64 = 4096;
        const MIN_RAW_SUPPORT: f64 = 0.15;

        let mut min = (f64::INFINITY, 0_u64, EllipticalFamily::Classical);
        let mut max = (0.0_f64, 0_u64, EllipticalFamily::Classical);
        for seed in 0..SEEDS {
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            let profile = elliptical_density_profile(GRID, GRID, config);
            let positive = profile.data.iter().filter(|&&v| v > 0.0).count() as f64;
            let fraction = positive / (GRID * GRID) as f64;
            assert!(
                fraction >= MIN_RAW_SUPPORT,
                "seed {seed} ({}): raw positive support {fraction:.4} < {MIN_RAW_SUPPORT}",
                family_name(config.family)
            );
            if fraction < min.0 {
                min = (fraction, seed, config.family);
            }
            if fraction > max.0 {
                max = (fraction, seed, config.family);
            }
        }
        println!(
            "B2.2F raw support population (40×40, {SEEDS} seeds):              min {:.4} (seed {} = {}), max {:.4} (seed {} = {}), bound ≥ {MIN_RAW_SUPPORT}",
            min.0,
            min.1,
            family_name(min.2),
            max.0,
            max.1,
            family_name(max.2)
        );
    }

    /// Diagnostic seed panel (not a golden test): the 14-seed panel with
    /// per-seed morphology parameters, the geometric support multiplier,
    /// the raw post-geometric-support positive fraction (pre-render,
    /// 40×40 body map) and the post-render visible terminal-cell
    /// occupancy at 40×20.
    #[allow(clippy::print_literal)]
    #[test]
    fn test_seed_panel_diagnostic_b22() {
        let panel: [u64; 14] = [0, 1, 2, 3, 4, 5, 8, 13, 16, 21, 42, 64, 99, 128];
        let (width, terminal_height) = (40usize, 20usize);
        let render_height = terminal_height * 2;
        println!("B2.2F seed panel (diagnostic, {width}x{terminal_height}):");
        println!(
            "  {:>4} | {:<13} | {:>5} | {:>5} | {:>4} | {:>5} | {:>4} | {:>8} | {:>8}",
            "seed", "family", "q", "Re", "n", "pa", "k", "raw+", "visible"
        );
        for seed in panel {
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            let profile = elliptical_density_profile(width, render_height, config);
            let raw_positive = profile
                .data
                .iter()
                .filter(|&&v| v.is_finite() && v > 0.0)
                .count() as f64
                / (width * render_height) as f64;
            let scene = ArtModel::Elliptical.generate_scene(width, terminal_height, Some(seed));
            let visible = post_render_visible_occupancy(&scene.density);
            println!(
                "  {seed:>4} | {:<13} | {:.3} | {:.3} | {:.2} | {:.3} | {:.2} | {:.4} | {:.4}",
                family_name(config.family),
                config.axis_ratio,
                config.effective_radius,
                config.profile_index,
                config.position_angle,
                config.family.support_re_multiplier(),
                raw_positive,
                visible
            );
        }
    }

    // ── B3.1 shaped isophotes (fourth harmonic) ─────────

    /// Maps an intrinsic (deprojected) point `(u, v)` — `u` along the
    /// major axis, `v` the minor-axis coordinate stretched to the unit
    /// semi-axis — to canvas-fraction coordinates for a galaxy of axis
    /// ratio `q` and position angle `pa`. Inverse of the rotation in
    /// [`elliptical_coordinates`]: the resulting canvas point has exactly
    /// deprojected coordinates `(u, v)` in that galaxy frame.
    fn canvas_point(u: f64, v: f64, q: f64, pa: f64) -> (f64, f64) {
        (
            u * pa.cos() - v * q * pa.sin(),
            u * pa.sin() + v * q * pa.cos(),
        )
    }

    /// c = 0 must reproduce the B2 elliptical radius bit-for-bit across a
    /// broad q / pa / coordinate grid, including the exact centre.
    #[test]
    fn test_b31_c_zero_exact_b2_degeneracy() {
        for q in [0.55_f64, 0.60, 0.63, 0.72, 0.79, 0.88, 0.95, 1.0] {
            for pa in [
                0.0_f64,
                0.4,
                std::f64::consts::FRAC_PI_4,
                1.07,
                std::f64::consts::FRAC_PI_2,
                std::f64::consts::PI - 1.0e-9,
            ] {
                for gy in 0..=80usize {
                    for gx in 0..=80usize {
                        let dx = -0.5 + gx as f64 * 0.0125;
                        let dy = -0.5 + gy as f64 * 0.0125;
                        assert_eq!(
                            elliptical_shaped_radius(dx, dy, q, pa, 0.0),
                            elliptical_radius(dx, dy, q, pa),
                            "c=0 must be bit-for-bit B2 at ({dx},{dy}), q={q}, pa={pa}"
                        );
                    }
                }
                // Exact centre: both radii are exactly 0.0.
                assert_eq!(elliptical_shaped_radius(0.0, 0.0, q, pa, 0.0), 0.0);
                assert_eq!(elliptical_radius(0.0, 0.0, q, pa), 0.0);
            }
        }
    }

    /// At c = 0 the per-cell seam degenerates to the B2 contract
    /// bit-for-bit: support is `r_ell <= k·Re` and the intensity is the
    /// pure Sersic profile evaluated at `r_ell` (or exactly 0.0).
    #[test]
    fn test_b31_c_zero_cell_level_equivalence() {
        let config = EllipticalGalaxyConfig {
            family: EllipticalFamily::Classical,
            axis_ratio: 0.79,
            position_angle: 2.31,
            effective_radius: 0.31,
            profile_index: 3.9,
            core_softening_fraction: 0.0,
            central_excess: 0.0,
            outer_halo_strength: 0.0,
            outer_halo_scale: 1.0,
            isophote_shape: 0.0,
        };
        let k = config.family.support_re_multiplier();
        let (width, render_height) = (40usize, 40usize);
        for y in 0..render_height {
            for x in 0..width {
                let dx = (x as f64 - width as f64 / 2.0) / width as f64;
                let dy = (y as f64 - render_height as f64 / 2.0) / render_height as f64;
                let r_ell = elliptical_radius(dx, dy, config.axis_ratio, config.position_angle);
                let cell = elliptical_cell(dx, dy, config);
                assert_eq!(
                    cell.inside_support,
                    r_ell <= k * config.effective_radius,
                    "support must be r_ell <= k·Re at ({x},{y})"
                );
                let expected = if cell.inside_support {
                    sersic_body_profile(r_ell, config.effective_radius, config.profile_index)
                } else {
                    0.0
                };
                assert_eq!(cell.intensity, expected, "intensity at ({x},{y})");
            }
        }
    }

    /// Sign convention: at fixed shaped-radius level `R`, the contour
    /// radius on the intrinsic axes is `R·(1+c)` and on the intrinsic
    /// diagonals `R·(1−c)` — for c > 0 (disky); for c < 0 the
    /// relations reverse. Verified through the production shaped radius
    /// at several position angles.
    #[test]
    fn test_b31_sign_convention_axes_and_diagonals() {
        let q = 0.68_f64;
        let level = 0.30_f64;
        let tol = 1.0e-12 * level;

        for &c in &[0.03_f64, 0.045, -0.03, -0.045] {
            for pa in [0.0_f64, 0.71, std::f64::consts::FRAC_PI_2] {
                // Intrinsic major-axis point at deprojected radius
                // level·(1+c): its shaped radius must equal `level`.
                let (dx_a, dy_a) = canvas_point(level * (1.0 + c), 0.0, q, pa);
                let r_axis = elliptical_shaped_radius(dx_a, dy_a, q, pa, c);
                assert!(
                    (r_axis - level).abs() <= tol,
                    "axis contour at ({dx_a},{dy_a}) has shaped radius {r_axis} (want {level}), c={c}, pa={pa}"
                );

                // Intrinsic diagonal point (u = v, i.e. 45° in the
                // deprojected frame) at deprojected radius level·(1−c).
                let half = level * (1.0 - c) * std::f64::consts::FRAC_1_SQRT_2;
                let (dx_d, dy_d) = canvas_point(half, half, q, pa);
                let r_diag = elliptical_shaped_radius(dx_d, dy_d, q, pa, c);
                assert!(
                    (r_diag - level).abs() <= tol,
                    "diagonal contour at ({dx_d},{dy_d}) has shaped radius {r_diag} (want {level}), c={c}, pa={pa}"
                );
            }
        }

        // Direction of the relation: c > 0 pushes the axis contour out
        // beyond level and pulls the diagonal inside level; c < 0 reverses both.
        for &c in &[0.03_f64, -0.03] {
            let axis_rho = level * (1.0 + c);
            let diag_rho = level * (1.0 - c);
            assert_eq!(axis_rho > level, c > 0.0, "c={c}");
            assert_eq!(diag_rho < level, c > 0.0, "c={c}");
        }
    }

    /// Fourth-harmonic relations between equal-magnitude +c and −c:
    /// the deprojected contour of level `level` is `level·(1 ± c·cos 4θ)`,
    /// so the two contours are mirrored about level `level`, the −c
    /// contour is the +c contour rotated by 45°, and they coincide
    /// wherever cos 4θ = 0 — the intrinsic 22.5° + k·45°
    /// phase crossings (a quarter period of the 4θ harmonic).
    #[test]
    fn test_b31_plus_minus_c_fourth_harmonic_symmetry() {
        let q = 0.66_f64;
        let pa = 0.0_f64; // intrinsic frame coincides with the canvas frame
        let level = 0.25_f64;
        let c = 0.03_f64;
        let tol = 1.0e-12 * level;

        // 1. The production shaped radius implements the accepted
        //    contour: the point at deprojected radius level·(1 + s·c·cos
        //    4θ) in intrinsic direction θ has shaped radius `level`.
        const STEPS: usize = 720; // 0.5° intrinsic phase steps
        for i in 0..STEPS {
            let theta = 2.0 * std::f64::consts::PI * (i as f64 / STEPS as f64);
            for sign in [1.0_f64, -1.0] {
                let rho = level * (1.0 + sign * c * (4.0 * theta).cos());
                let (dx, dy) = canvas_point(rho * theta.cos(), rho * theta.sin(), q, pa);
                let r_shape = elliptical_shaped_radius(dx, dy, q, pa, sign * c);
                assert!(
                    (r_shape - level).abs() <= tol,
                    "θ={theta}: contour shaped radius {r_shape} (want {level}), sign={sign}"
                );
            }
        }

        // 2. Mirror symmetry about `level` and the 45° rotation
        //    relation, on a dense 0.1° grid.
        const FINE: usize = 3600;
        for i in 0..FINE {
            let theta = 2.0 * std::f64::consts::PI * (i as f64 / FINE as f64);
            let cos4 = (4.0 * theta).cos();
            let rho_plus = level * (1.0 + c * cos4);
            let rho_minus = level * (1.0 - c * cos4);
            assert!(
                (rho_plus + rho_minus - 2.0 * level).abs() <= tol,
                "θ={theta}: mirror symmetry about `level` failed"
            );
            let rho_plus_rot =
                level * (1.0 + c * (4.0 * (theta + std::f64::consts::FRAC_PI_4)).cos());
            assert!(
                (rho_plus_rot - rho_minus).abs() <= tol,
                "θ={theta}: −c contour is the +c contour rotated by 45°"
            );
        }

        // 3. Intrinsic 22.5° phase rotation: at θ = 22.5° + k·45°
        //    the +c and −c isophotes cross at `level` (cos 4θ = 0),
        //    so the production shaped radius is identical for +c and
        //    −c there — and equal to the plain deprojected radius.
        for k in 0..7 {
            let theta = std::f64::consts::FRAC_PI_8 + (k as f64) * std::f64::consts::FRAC_PI_4;
            let rho = 0.31_f64;
            let (dx, dy) = canvas_point(rho * theta.cos(), rho * theta.sin(), q, pa);
            let rp = elliptical_shaped_radius(dx, dy, q, pa, c);
            let rm = elliptical_shaped_radius(dx, dy, q, pa, -c);
            assert!(
                (rp - rm).abs() <= 1.0e-14 * rho,
                "k={k}: +c/−c must cross at 22.5° + k·45°"
            );
            assert!(
                (rp - rho).abs() <= 1.0e-9 * rho,
                "k={k}: crossing must sit at the plain deprojected radius"
            );
        }
    }

    /// Numerical bounds across the full allowed c range `[-0.045,
    /// +0.045]`, every family q range, representative PAs and a dense
    /// canvas grid: the shaped radius is finite and non-negative, and
    /// the denominator `1 + c·cos 4θ` stays in `[0.955, 1.045]`
    /// (modulo fp rounding, at most a few ulp).
    #[test]
    fn test_b31_shaped_radius_numerical_bounds() {
        const GRID: usize = 64;
        let cs = [
            -0.045_f64, -0.03, -0.02, -0.005, 0.0, 0.005, 0.02, 0.03, 0.045,
        ];
        // q across all family ranges: the four family lo/hi endpoints
        // plus interior values of the union [0.55, 0.95].
        let qs = [0.55_f64, 0.63, 0.72, 0.80, 0.88, 0.90, 0.95];
        let pas = [
            0.0_f64,
            0.4,
            std::f64::consts::FRAC_PI_4,
            1.07,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI - 1.0e-9,
        ];
        for &c in &cs {
            for &q in &qs {
                for &pa in &pas {
                    for gy in 0..=GRID {
                        for gx in 0..=GRID {
                            let dx = -0.5 + gx as f64 * (1.0 / GRID as f64);
                            let dy = -0.5 + gy as f64 * (1.0 / GRID as f64);
                            let r_shape = elliptical_shaped_radius(dx, dy, q, pa, c);
                            assert!(
                                r_shape.is_finite() && r_shape >= 0.0,
                                "shaped radius {r_shape} at ({dx},{dy}), c={c}, q={q}, pa={pa}"
                            );
                            if r_shape > 0.0 {
                                // The denominator recovers as r_ell / r_shape.
                                let r_ell = elliptical_radius(dx, dy, q, pa);
                                let denominator = r_ell / r_shape;
                                assert!(
                                    (0.955 - 1.0e-12..=1.045 + 1.0e-12).contains(&denominator),
                                    "denominator {denominator} outside [0.955, 1.045] at ({dx},{dy}), c={c}, q={q}, pa={pa}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// The extrema of the fourth-harmonic shape stay at the intrinsic
    /// phases 0°/45°/90°/135° for representative q across the
    /// full allowed population: the phase lattice is a property of
    /// `cos 4θ` and is independent of q. (This is asserted in the
    /// intrinsic frame; the on-sky positions of the extrema rotate with
    /// the position angle and are NOT the canvas diagonals.)
    #[test]
    fn test_b31_intrinsic_extrema_phase_independent_of_q() {
        const STEPS: usize = 3600; // 0.1° grid
        let two_pi = 2.0 * std::f64::consts::PI;
        let rho = 0.30_f64;
        let grid_step = two_pi / STEPS as f64;
        for &q in &[0.55_f64, 0.63, 0.72, 0.80, 0.88, 0.95] {
            for &c in &[0.045_f64, -0.045, 0.03, -0.03] {
                // Field over the intrinsic phase: shaped radius at a
                // fixed deprojected radius rho.
                let mut values = Vec::with_capacity(STEPS);
                for i in 0..STEPS {
                    let theta = two_pi * (i as f64 / STEPS as f64);
                    let (dx, dy) = canvas_point(rho * theta.cos(), rho * theta.sin(), q, 0.0);
                    values.push(elliptical_shaped_radius(dx, dy, q, 0.0, c));
                }
                let max_idx = values
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .unwrap()
                    .0;
                let min_idx = values
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.total_cmp(b.1))
                    .unwrap()
                    .0;
                // Extrema must sit on the 0/45/90/135 lattice.
                for (idx, label) in [(max_idx, "max"), (min_idx, "min")] {
                    let theta = two_pi * (idx as f64 / STEPS as f64);
                    let mod45 = theta % std::f64::consts::FRAC_PI_4;
                    let dist = mod45.min(std::f64::consts::FRAC_PI_4 - mod45);
                    assert!(
                        dist <= 2.0 * grid_step + 1.0e-12,
                        "q={q}, c={c}: {label} extremum at {theta} rad, off the intrinsic 0/45/90/135 lattice"
                    );
                }
                // c > 0: field minimum on the intrinsic axes, maximum
                // on the diagonals; c < 0 reverses the assignment.
                let axis_value = values[0];
                let diag_value = values[STEPS / 8]; // 45°
                assert_eq!(
                    axis_value < diag_value,
                    c > 0.0,
                    "q={q}, c={c}: axis/diagonal ordering"
                );
            }
        }
    }

    /// Rotating the position angle must rotate the full boxy/disky
    /// pattern with the galaxy: rotating a point by Δ and the
    /// position angle by Δ leaves the shaped radius unchanged, and
    /// the pattern's axis sits on the (rotated) major axis.
    #[test]
    fn test_b31_rotation_carry_rotates_the_pattern() {
        let q = 0.66_f64;
        let c = 0.04_f64;
        const GRID: usize = 96;
        for &pa in &[0.0_f64, 0.4, 1.1, 2.3] {
            for &delta in &[0.3_f64, -0.7, std::f64::consts::FRAC_PI_4] {
                let cd = delta.cos();
                let sd = delta.sin();
                for gy in 0..=GRID {
                    for gx in 0..=GRID {
                        let dx = -0.5 + gx as f64 * (1.0 / GRID as f64);
                        let dy = -0.5 + gy as f64 * (1.0 / GRID as f64);
                        let (rxd, ryd) = (dx * cd - dy * sd, dx * sd + dy * cd);
                        let a = elliptical_shaped_radius(dx, dy, q, pa, c);
                        let b = elliptical_shaped_radius(rxd, ryd, q, pa + delta, c);
                        assert!(
                            (a - b).abs() <= 1.0e-12,
                            "rotation carry failed at ({dx},{dy}), pa={pa}, delta={delta}"
                        );
                    }
                }
            }
        }

        // The on-sky axis of the pattern follows the major axis (at
        // angle pa from +x): a major-axis point at canvas distance d
        // has shaped radius d/(1 + c) for c > 0.
        for &pa in &[0.3_f64, 1.2] {
            let d = 0.35_f64;
            let (dx, dy) = (d * pa.cos(), d * pa.sin());
            let r = elliptical_shaped_radius(dx, dy, q, pa, c);
            assert!(
                (r - d / (1.0 + c)).abs() <= 1.0e-12 * d,
                "major-axis shaped radius must be d/(1+c) at pa={pa}"
            );
        }
    }

    /// The support decision and the pre-grain Sersic intensity consume
    /// the SAME shaped radius: `inside_support` iff `r_shape <= k·Re`,
    /// and when inside, intensity is exactly
    /// `sersic_body_profile(r_shape, Re, n)`. Includes anti-clipping
    /// cases where the pure ellipse says outside (`r_ell > k·Re`) but
    /// the shaped support still keeps the cell — for c > 0 along the
    /// intrinsic axes, and the sign-reversed equivalent for c < 0 —
    /// plus the complementary removed cells (`r_ell <= k·Re` but
    /// `r_shape > k·Re`).
    #[test]
    fn test_b31_shared_support_and_profile_radius() {
        const GRID: usize = 199;
        let make_config = |c: f64, pa: f64| EllipticalGalaxyConfig {
            family: EllipticalFamily::CompactDisky,
            axis_ratio: 0.63,
            position_angle: pa,
            effective_radius: 0.24,
            profile_index: 3.0,
            core_softening_fraction: 0.0,
            central_excess: 0.0,
            outer_halo_strength: 0.0,
            outer_halo_scale: 1.0,
            isophote_shape: c,
        };

        for (c, pa) in [
            (0.045_f64, 0.0_f64),
            (-0.045_f64, 0.0_f64),
            (0.045_f64, 0.9_f64),
        ] {
            let config = make_config(c, pa);
            let k = config.family.support_re_multiplier();
            let limit = k * config.effective_radius;
            let mut anti_clipped = 0usize;
            let mut removed = 0usize;
            for gy in 0..=GRID {
                for gx in 0..=GRID {
                    let dx = -0.5 + gx as f64 * (1.0 / GRID as f64);
                    let dy = -0.5 + gy as f64 * (1.0 / GRID as f64);
                    let r_ell = elliptical_radius(dx, dy, config.axis_ratio, pa);
                    let r_shape = elliptical_shaped_radius(dx, dy, config.axis_ratio, pa, c);
                    let cell = elliptical_cell(dx, dy, config);

                    assert_eq!(
                        cell.inside_support,
                        r_shape <= limit,
                        "support must be r_shape <= k·Re at ({dx},{dy})"
                    );
                    if cell.inside_support {
                        assert_eq!(
                            cell.intensity,
                            sersic_body_profile(
                                r_shape,
                                config.effective_radius,
                                config.profile_index
                            ),
                            "intensity must be the Sersic profile at r_shape ({dx},{dy})"
                        );
                        assert!(cell.intensity > 0.0);
                    } else {
                        assert_eq!(cell.intensity, 0.0, "({dx},{dy})");
                    }

                    if r_ell > limit && r_shape <= limit {
                        anti_clipped += 1;
                    }
                    if r_ell <= limit && r_shape > limit {
                        removed += 1;
                    }
                }
            }
            // Both shaped-support effects must actually occur on the
            // grid: cells pulled into support (anti-clipping) and cells
            // pushed out of support.
            assert!(
                anti_clipped > 0,
                "c={c}, pa={pa}: no anti-clipped cells (r_ell > k·Re, r_shape <= k·Re)"
            );
            assert!(
                removed > 0,
                "c={c}, pa={pa}: no removed cells (r_ell <= k·Re, r_shape > k·Re)"
            );
        }
    }

    /// B3.1 diagnostic seed panel (not a golden test): per-seed family,
    /// q, pa, Re, n, c, k, the raw post-support positive fraction
    /// (pre-render, 40×40 body map) and the post-render visible
    /// terminal-cell occupancy at 40×20 — the B2-vs-B3.1
    /// comparison panel.
    #[allow(clippy::print_literal)]
    #[test]
    fn test_b31_seed_panel_diagnostic() {
        let panel: [u64; 8] = [2859, 3023, 260, 1258, 2, 4, 64, 128];
        let (width, terminal_height) = (40usize, 20usize);
        let render_height = terminal_height * 2;
        println!("B3.1 seed panel (diagnostic, {width}x{terminal_height}):");
        println!(
            "  {:>5} | {:<13} | {:>5} | {:>5} | {:>5} | {:>4} | {:>9} | {:>4} | {:>8} | {:>8}",
            "seed", "family", "q", "pa", "Re", "n", "c", "k", "raw+", "visible"
        );
        for seed in panel {
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            let profile = elliptical_density_profile(width, render_height, config);
            let raw_positive = profile
                .data
                .iter()
                .filter(|&&v| v.is_finite() && v > 0.0)
                .count() as f64
                / (width * render_height) as f64;
            let scene = ArtModel::Elliptical.generate_scene(width, terminal_height, Some(seed));
            let visible = post_render_visible_occupancy(&scene.density);
            println!(
                "  {seed:>5} | {:<13} | {:.3} | {:.3} | {:.3} | {:.2} | {:+.6} | {:.2} | {:.4} | {:.4}",
                family_name(config.family),
                config.axis_ratio,
                config.position_angle,
                config.effective_radius,
                config.profile_index,
                config.isophote_shape,
                config.family.support_re_multiplier(),
                raw_positive,
                visible
            );
        }
    }

    // ── B3.2 central structure (softened core + central excess) ─────

    /// Zero degeneracy (helper level): core = 0 AND excess = 0 must
    /// return the body intensity bit-for-bit at a dense set of radii
    /// (including the exact centre and the widest family support edge),
    /// across family Re corners and the full n range.
    #[test]
    fn test_b32_zero_central_is_bit_for_bit_body() {
        for &re in &[0.20_f64, 0.24, 0.30, 0.34, 0.36, 0.44] {
            for &n in &[2.0_f64, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 6.0] {
                for &rho in &[
                    0.0_f64, 1e-9, 0.02, 0.05, 0.1, 0.16651, 0.2, 0.3, 0.5, 0.75, 1.0, 1.4, 1.75,
                ] {
                    let r = rho * re;
                    let body = sersic_body_profile(r, re, n);
                    assert_eq!(
                        apply_central_structure(body, r, re, n, 0.0, 0.0),
                        body,
                        "zero-central must be bit-for-bit body at r={r}, Re={re}, n={n}"
                    );
                }
            }
        }
    }

    /// Zero degeneracy (cell level): a directly-built zero-central config
    /// with a NONZERO isophote shape reproduces the B3.1 seam
    /// bit-for-bit: exactly `sersic_body_profile(r_shape, Re, n)` inside
    /// support and 0.0 outside. Built directly (not drawn) because the
    /// family contract does not generate both-zero central scenes for
    /// every family.
    #[test]
    fn test_b32_zero_central_cell_level_b31_equivalence() {
        let config = EllipticalGalaxyConfig {
            family: EllipticalFamily::Classical,
            axis_ratio: 0.79,
            position_angle: 2.31,
            effective_radius: 0.31,
            profile_index: 3.9,
            core_softening_fraction: 0.0,
            central_excess: 0.0,
            outer_halo_strength: 0.0,
            outer_halo_scale: 1.0,
            isophote_shape: 0.02,
        };
        let k = config.family.support_re_multiplier();
        let (width, render_height) = (40usize, 40usize);
        for y in 0..render_height {
            for x in 0..width {
                let dx = (x as f64 - width as f64 / 2.0) / width as f64;
                let dy = (y as f64 - render_height as f64 / 2.0) / render_height as f64;
                let r_shape = elliptical_shaped_radius(
                    dx,
                    dy,
                    config.axis_ratio,
                    config.position_angle,
                    config.isophote_shape,
                );
                let cell = elliptical_cell(dx, dy, config);
                assert_eq!(
                    cell.inside_support,
                    r_shape <= k * config.effective_radius,
                    "support must be r_shape <= k·Re at ({x},{y})"
                );
                let expected = if cell.inside_support {
                    sersic_body_profile(r_shape, config.effective_radius, config.profile_index)
                } else {
                    0.0
                };
                assert_eq!(cell.intensity, expected, "intensity at ({x},{y})");
            }
        }
    }

    /// Core centre: for every f > 0 the softened core keeps the centre
    /// exactly 1.0 (`hypot(0, rc) == rc` ⇒ ratio exactly 1), and the
    /// combined core + excess centre is exactly 1.0 as well.
    #[test]
    fn test_b32_core_centre_is_exactly_one() {
        for &re in &[0.20_f64, 0.30, 0.44] {
            for &n in &[2.0_f64, 4.0, 6.0] {
                for &f in &[0.05_f64, 0.10, 0.15, 0.20, 0.25, 0.30] {
                    let body0 = sersic_body_profile(0.0, re, n);
                    assert_eq!(body0, 1.0);
                    assert_eq!(
                        apply_central_structure(body0, 0.0, re, n, f, 0.0),
                        1.0,
                        "core centre must be exactly 1 (Re={re}, n={n}, f={f})"
                    );
                    for &e in &[0.05_f64, 0.10, 0.20] {
                        assert_eq!(
                            apply_central_structure(body0, 0.0, re, n, f, e),
                            1.0,
                            "combined centre must be exactly 1 (Re={re}, n={n}, f={f}, e={e})"
                        );
                    }
                }
            }
        }
    }

    /// Core monotonicity / no ring: dense radial grids across
    /// n = 2..6, family Re corners, and representative core fractions
    /// through 0.30 must stay finite, in (0, 1], and non-increasing in
    /// radius (a ring or a hole would appear as an increase or a zero).
    #[test]
    fn test_b32_core_monotone_non_increasing_no_ring() {
        const STEPS: usize = 4000;
        let r_max_mul = 1.75_f64; // widest family support (CompactDisky k)
        for &n in &[2.0_f64, 3.0, 4.0, 5.0, 6.0] {
            for &re in &[0.20_f64, 0.24, 0.30, 0.34, 0.36, 0.44] {
                for &f in &[0.05_f64, 0.10, 0.15, 0.20, 0.25, 0.30] {
                    let mut prev = f64::INFINITY;
                    for i in 0..=STEPS {
                        let r = r_max_mul * re * (i as f64 / STEPS as f64);
                        let v = apply_central_structure(
                            sersic_body_profile(r, re, n),
                            r,
                            re,
                            n,
                            f,
                            0.0,
                        );
                        assert!(v.is_finite(), "n={n}, Re={re}, f={f}, r={r}");
                        assert!(v > 0.0, "no hole: n={n}, Re={re}, f={f}, r={r}");
                        assert!(v <= 1.0, "n={n}, Re={re}, f={f}, r={r}");
                        if i == 0 {
                            assert_eq!(v, 1.0, "centre exactly 1 (n={n}, Re={re}, f={f})");
                        } else {
                            assert!(
                                v <= prev * (1.0 + 1e-12),
                                "ring / increase at r={r} (n={n}, Re={re}, f={f}): {v} > {prev}"
                            );
                        }
                        prev = v;
                    }
                }
            }
        }
    }

    /// Core strength ordering. A stronger core softening flattens the
    /// inner profile. Expressed mathematically (the naive pointwise
    /// claim "intensity at fixed r decreases with f" would be the WRONG
    /// direction):
    ///
    /// (a) pointwise: for every fixed r > 0, I_core(r; f) is strictly
    ///     INCREASING in f — the profile at any fixed radius rises
    ///     toward the (unchanged) centre value, so the centre's
    ///     relative brightness over radius r, 1 − I_core(r; f),
    ///     decreases;
    ///
    /// (b) enclosed concentration: the 2-D light fraction inside the
    ///     documented fixed radius r0 = 0.5·Re is strictly DECREASING
    ///     in f.
    #[test]
    fn test_b32_core_strength_ordering_flattens_central_concentration() {
        // (a) pointwise in f at fixed r > 0.
        for &n in &[2.0_f64, 3.0, 4.0, 5.0, 6.0] {
            for &re in &[0.24_f64, 0.36, 0.44] {
                for &rho in &[0.05_f64, 0.10, 0.20, 0.50, 1.00, 1.40] {
                    let r = rho * re;
                    let fs = [0.01_f64, 0.05, 0.10, 0.15, 0.20, 0.25, 0.30];
                    let mut prev = apply_central_structure(
                        sersic_body_profile(r, re, n),
                        r,
                        re,
                        n,
                        fs[0],
                        0.0,
                    );
                    for &f in &fs[1..] {
                        let v = apply_central_structure(
                            sersic_body_profile(r, re, n),
                            r,
                            re,
                            n,
                            f,
                            0.0,
                        );
                        assert!(
                            v > prev,
                            "I(r; f) must strictly increase in f at r={r} (n={n}, Re={re})"
                        );
                        prev = v;
                    }
                }
            }
        }

        // (b) enclosed 2-D light fraction within r0 = 0.5·Re.
        let re = 0.30_f64;
        let r0 = 0.5 * re;
        for &n in &[2.0_f64, 4.0, 6.0] {
            let fs = [0.0_f64, 0.05, 0.10, 0.15, 0.20, 0.25, 0.30];
            let mut prev_frac = f64::NAN;
            for &f in &fs {
                let (num, den) = enclosed_light(
                    |r| apply_central_structure(sersic_body_profile(r, re, n), r, re, n, f, 0.0),
                    r0,
                    8.0 * re,
                );
                let frac = num / den;
                if prev_frac.is_finite() {
                    assert!(
                        frac < prev_frac,
                        "enclosed(0.5Re) must strictly decrease in f (n={n}, f={f}): {frac} >= {prev_frac}"
                    );
                }
                prev_frac = frac;
            }
        }
    }

    /// The central-excess kernel contract, pinned exactly:
    ///
    /// ```text
    /// K(0)              == 1.0            (exact)
    /// K(scale)          == exp(−1)        (exact, at x = 1)
    /// K(scale·√(ln 2))  == 0.5            (to 1 ulp)
    /// FWHM              == 2·scale·√(ln 2) ≈ 0.33302·Re
    /// half-max radius   == scale·√(ln 2)  ≈ 0.16651·Re
    /// ```
    ///
    /// `scale = CENTRAL_EXCESS_SCALE_FRACTION·Re` is the e-folding scale
    /// of `K(r) = exp(−(r/scale)²)` — not a statistical Gaussian sigma
    /// (the 0.471·Re half-width belongs to the different convention
    /// `exp(−r²/(2σ²))` and must not be encoded here).
    #[test]
    fn test_b32_excess_kernel_contract() {
        let re = 0.25_f64;
        let scale = CENTRAL_EXCESS_SCALE_FRACTION * re;

        // K(0) == 1 exactly (peak at the centre).
        assert_eq!(central_excess_kernel(0.0), 1.0);

        // K(scale): x = scale/scale = 1.0 exactly → exp(−1) exactly.
        let x_at_scale = scale / scale;
        assert_eq!(x_at_scale, 1.0);
        assert_eq!(central_excess_kernel(x_at_scale), (-1.0_f64).exp());

        // Half maximum: K(scale·√(ln 2)) == 0.5 (to 1 ulp).
        let sqrt_ln2 = std::f64::consts::LN_2.sqrt();
        assert!((central_excess_kernel(sqrt_ln2) - 0.5).abs() <= 1e-15);

        // Half-maximum radius and full half-maximum width in physical
        // radius units.
        let r_half = scale * sqrt_ln2;
        assert!(
            (r_half / re - 0.16651).abs() < 1.0e-4,
            "r_half/Re = {}",
            r_half / re
        );
        let fwhm = 2.0 * r_half;
        assert!(
            (fwhm / re - 0.33302).abs() < 1.0e-4,
            "FWHM/Re = {}",
            fwhm / re
        );
        // And the same half-maximum radius through the kernel itself.
        assert!((central_excess_kernel(r_half / scale) - 0.5).abs() <= 1e-15);

        // Strictly positive and monotone non-increasing on a dense grid.
        const STEPS: usize = 2000;
        let mut prev = f64::INFINITY;
        for i in 0..=STEPS {
            let x = 3.0 * (i as f64 / STEPS as f64);
            let k = central_excess_kernel(x);
            assert!(k.is_finite() && k > 0.0, "x={x}");
            assert!(k <= prev + 1e-15, "x={x}");
            prev = k;
        }
    }

    /// Excess composite contract, e ∈ (0, 0.20] (the CompactDisky
    /// maximum): the centre stays exactly 1, and across the full family
    /// support the profile is finite, in (0, 1], and radially
    /// non-increasing — no ring, no detached maximum.
    #[test]
    fn test_b32_excess_composite_bounded_monotone() {
        const STEPS: usize = 4000;
        let r_max_mul = 1.75_f64; // CompactDisky support
        for &e in &[0.05_f64, 0.10, 0.15, 0.20] {
            for &n in &[2.0_f64, 3.5, 6.0] {
                for &re in &[0.20_f64, 0.30] {
                    let mut prev = f64::INFINITY;
                    for i in 0..=STEPS {
                        let r = r_max_mul * re * (i as f64 / STEPS as f64);
                        let v = apply_central_structure(
                            sersic_body_profile(r, re, n),
                            r,
                            re,
                            n,
                            0.0,
                            e,
                        );
                        assert!(v.is_finite(), "e={e}, n={n}, Re={re}, r={r}");
                        assert!(v > 0.0, "e={e}, n={n}, Re={re}, r={r}");
                        assert!(v <= 1.0, "e={e}, n={n}, Re={re}, r={r}");
                        if i == 0 {
                            assert_eq!(v, 1.0, "centre exactly 1 (e={e}, n={n}, Re={re})");
                        } else {
                            assert!(
                                v <= prev * (1.0 + 1e-12),
                                "ring / increase at r={r} (e={e}, n={n}, Re={re}): {v} > {prev}"
                            );
                        }
                        prev = v;
                    }
                }
            }
        }
    }

    /// Excess strength. The naive pointwise claim
    /// "I(r; e2) <= I(r; e1) for every r" is FALSE for the additive
    /// kernel: stronger `e` raises the inner profile (toward the centre
    /// value) while the peak normalization (1 + e) pulls the outer
    /// profile down. The intended behavior is pinned instead:
    ///
    /// (a) the centre value stays exactly 1.0 for every e;
    ///
    /// (b) central enclosed concentration: the 2-D light fraction
    ///     inside the documented fixed radius r0 = 0.1·Re strictly
    ///     INCREASES with e. At r0 the kernel encloses a larger light
    ///     fraction than the body does for all n ∈ [2, 6] (kernel:
    ///     1 − e^−0.25 ≈ 0.221; body: ≤ ≈ 0.172 at n = 6), and r0 sits
    ///     strictly inside the kernel/body intensity crossing (≈
    ///     0.278·Re at n = 2, ≈ 0.660·Re at n = 6), the region where
    ///     ∂I/∂e > 0 — adding the kernel therefore raises the inner
    ///     fraction;
    ///
    /// (c) beyond the crossing, at the documented outer radius
    ///     r_out = 0.9·Re (the kernel is negligible there, K ≤
    ///     exp(−20.25) ≈ 1.5e−9), I(r_out; e) strictly DECREASES with
    ///     e — centre-to-outer contrast 1/I(r_out) increases.
    #[test]
    fn test_b32_excess_strength_central_concentration() {
        let es = [0.0_f64, 0.05, 0.10, 0.15, 0.20];
        let re = 0.25_f64;

        // (a) centre exactly 1 for every e.
        for &e in &es {
            assert_eq!(
                apply_central_structure(sersic_body_profile(0.0, re, 3.0), 0.0, re, 3.0, 0.0, e),
                1.0,
                "centre must be exactly 1 (e={e})"
            );
        }

        // (b) enclosed 2-D light fraction within r0 = 0.1·Re.
        for &n in &[2.0_f64, 6.0] {
            let mut prev_frac = f64::NAN;
            for &e in &es {
                let (num, den) = enclosed_light(
                    |r| apply_central_structure(sersic_body_profile(r, re, n), r, re, n, 0.0, e),
                    0.1 * re,
                    8.0 * re,
                );
                let frac = num / den;
                if prev_frac.is_finite() {
                    assert!(
                        frac > prev_frac,
                        "enclosed(0.1Re) must strictly increase in e (n={n}, e={e}): {frac} <= {prev_frac}"
                    );
                }
                prev_frac = frac;
            }
        }

        // (c) outer profile at r_out = 0.9·Re.
        for &n in &[2.0_f64, 6.0] {
            let r_out = 0.9 * re;
            let mut prev = f64::NAN;
            for &e in &es {
                let v = apply_central_structure(
                    sersic_body_profile(r_out, re, n),
                    r_out,
                    re,
                    n,
                    0.0,
                    e,
                );
                if prev.is_finite() {
                    assert!(
                        v < prev,
                        "I(0.9Re; e) must strictly decrease in e (n={n}, e={e}): {v} >= {prev}"
                    );
                }
                prev = v;
            }
        }
    }

    /// Combined Classical case: representative simultaneous nonzero core
    /// and excess values (including the ~50/50 split 0.05/0.05) keep a
    /// monotone, ring-free, bounded profile with centre exactly 1.
    #[test]
    fn test_b32_combined_classical_monotone_bounded() {
        const STEPS: usize = 4000;
        let r_max_mul = 1.40_f64; // Classical support
        for &(f, e) in &[(0.05_f64, 0.05_f64), (0.02, 0.08), (0.08, 0.02)] {
            for &n in &[2.5_f64, 4.5] {
                for &re in &[0.24_f64, 0.36] {
                    let mut prev = f64::INFINITY;
                    for i in 0..=STEPS {
                        let r = r_max_mul * re * (i as f64 / STEPS as f64);
                        let v =
                            apply_central_structure(sersic_body_profile(r, re, n), r, re, n, f, e);
                        assert!(
                            v.is_finite() && v > 0.0 && v <= 1.0,
                            "f={f}, e={e}, n={n}, Re={re}, r={r}"
                        );
                        if i == 0 {
                            assert_eq!(v, 1.0, "centre exactly 1 (f={f}, e={e})");
                        } else {
                            assert!(
                                v <= prev * (1.0 + 1e-12),
                                "ring / increase at r={r} (f={f}, e={e}, n={n}, Re={re})"
                            );
                        }
                        prev = v;
                    }
                }
            }
        }
    }

    /// Support independence: for identical morphology with the central
    /// fields zeroed vs actual, `inside_support` is bit-identical and
    /// the positive-iff-supported invariant holds everywhere.
    #[test]
    fn test_b32_support_independent_of_central_structure() {
        let (width, render_height) = (60usize, 60usize);
        let mut configs: Vec<EllipticalGalaxyConfig> = (0..32u64)
            .map(|offset| EllipticalGalaxyConfig::for_scene_seed(2000 + offset))
            .collect();
        for seed in [207u64, 1976, 3215, 2714, 2205, 2302] {
            configs.push(EllipticalGalaxyConfig::for_scene_seed(seed));
        }
        configs.push(EllipticalGalaxyConfig {
            family: EllipticalFamily::Classical,
            axis_ratio: 0.75,
            position_angle: 0.9,
            effective_radius: 0.30,
            profile_index: 3.5,
            core_softening_fraction: 0.05,
            central_excess: 0.05,
            outer_halo_strength: 0.0,
            outer_halo_scale: 1.0,
            isophote_shape: -0.02,
        });

        for config in &configs {
            let zeroed = EllipticalGalaxyConfig {
                core_softening_fraction: 0.0,
                central_excess: 0.0,
                ..*config
            };
            for y in 0..render_height {
                for x in 0..width {
                    let dx = (x as f64 - width as f64 / 2.0) / width as f64;
                    let dy = (y as f64 - render_height as f64 / 2.0) / render_height as f64;
                    let cell = elliptical_cell(dx, dy, *config);
                    let zero_cell = elliptical_cell(dx, dy, zeroed);
                    assert_eq!(
                        cell.inside_support, zero_cell.inside_support,
                        "support must not depend on the central fields at ({x},{y})"
                    );
                    assert_eq!(
                        cell.intensity > 0.0,
                        cell.inside_support,
                        "positive iff supported at ({x},{y})"
                    );
                }
            }
        }
    }

    /// Same shaped radius (B3.1 shared-radius test extended to B3.2):
    /// support still uses `r_shape`, and the central structure consumes
    /// that SAME `r_shape` — a supported cell's intensity is exactly
    /// `apply_central_structure(sersic_body_profile(r_shape, Re, n),
    /// r_shape, Re, n, f, e)` with nonzero core and excess.
    #[test]
    fn test_b32_central_structure_consumes_same_shaped_radius() {
        const GRID: usize = 149;
        let config = EllipticalGalaxyConfig {
            family: EllipticalFamily::Classical,
            axis_ratio: 0.72,
            position_angle: 1.2,
            effective_radius: 0.30,
            profile_index: 3.4,
            core_softening_fraction: 0.05,
            central_excess: 0.05,
            outer_halo_strength: 0.0,
            outer_halo_scale: 1.0,
            isophote_shape: 0.03,
        };
        let k = config.family.support_re_multiplier();
        let limit = k * config.effective_radius;
        for gy in 0..=GRID {
            for gx in 0..=GRID {
                let dx = -0.5 + gx as f64 * (1.0 / GRID as f64);
                let dy = -0.5 + gy as f64 * (1.0 / GRID as f64);
                let r_shape = elliptical_shaped_radius(
                    dx,
                    dy,
                    config.axis_ratio,
                    config.position_angle,
                    config.isophote_shape,
                );
                let cell = elliptical_cell(dx, dy, config);
                assert_eq!(
                    cell.inside_support,
                    r_shape <= limit,
                    "support must be r_shape <= k·Re at ({dx},{dy})"
                );
                let expected = if cell.inside_support {
                    apply_central_structure(
                        sersic_body_profile(r_shape, config.effective_radius, config.profile_index),
                        r_shape,
                        config.effective_radius,
                        config.profile_index,
                        config.core_softening_fraction,
                        config.central_excess,
                    )
                } else {
                    0.0
                };
                assert_eq!(cell.intensity, expected, "intensity at ({dx},{dy})");
            }
        }
    }

    /// Grain protocol on B3.2 scenes with nonzero central structure:
    /// one draw per supported cell, zero outside, row-major,
    /// deterministic; the grain never creates or destroys support and
    /// the output stays finite in [0, 1].
    #[test]
    fn test_b32_grain_protocol_unchanged_with_central_structure() {
        let (width, render_height) = (40usize, 40usize);
        for seed in [207u64, 1976, 2205] {
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            let raw = elliptical_density_profile(width, render_height, config);

            let make = || {
                let context = GenerationContext::new(seed);
                let mut rng = StdRng::seed_from_u64(777_777);
                generate_elliptical_density(width, render_height, context, &mut rng)
            };
            let first = make();
            let second = make();
            assert_eq!(
                first.data, second.data,
                "generation must be deterministic (seed {seed})"
            );

            for y in 0..render_height {
                for x in 0..width {
                    let pre = raw.get(x, y);
                    let post = first.get(x, y);
                    assert!(
                        post.is_finite() && (0.0..=1.0).contains(&post),
                        "post-grain bounds ({x},{y}, seed {seed}): {post}"
                    );
                    assert_eq!(
                        post > 0.0,
                        pre > 0.0,
                        "grain must not create/destroy support at ({x},{y}, seed {seed})"
                    );
                    if pre > 0.0 {
                        // factor = 1 + U(−g, +g), g = ELLIPTICAL_GRAIN_FRACTION,
                        // clamped at 1.0.
                        let lo = pre * (1.0 - ELLIPTICAL_GRAIN_FRACTION);
                        let hi = (pre * (1.0 + ELLIPTICAL_GRAIN_FRACTION)).min(1.0);
                        assert!(
                            ((lo - 1e-15)..=(hi + 1e-15)).contains(&post),
                            "grain outside local band ({x},{y}, seed {seed}): pre={pre} post={post}"
                        );
                    } else {
                        assert_eq!(post, 0.0);
                    }
                }
            }
        }
    }

    /// 2-D enclosed-light trapezoid for a 1-D radial profile: returns
    /// (∫₀^{r0} 2πr·I(r) dr, ∫₀^{rmax} 2πr·I(r) dr) — the area element
    /// 2πr·dr is what makes this an enclosed-light fraction. Test-only
    /// numerical integration (dense trapezoid) for the enclosed-light
    /// concentration diagnostics.
    fn enclosed_light(profile: impl Fn(f64) -> f64, r0: f64, rmax: f64) -> (f64, f64) {
        // With rmax = 8·Re = 2.0 (Re = 0.25), dr = 2.0 / 32768 = 2⁻¹⁴
        // is exact in binary.
        const STEPS: usize = 32768;
        let dr = rmax / STEPS as f64;
        let mut num = 0.0_f64;
        let mut den = 0.0_f64;
        // w(r) = 2πr·I(r); at r = 0 the factor r makes w exactly 0.0.
        let mut prev_w = 0.0_f64;
        for i in 1..=STEPS {
            let r = i as f64 * dr;
            let w = 2.0 * std::f64::consts::PI * r * profile(r);
            let trapezoid = 0.5 * (prev_w + w) * dr;
            den += trapezoid;
            if r <= r0 {
                num += trapezoid;
            }
            prev_w = w;
        }
        (num, den)
    }

    /// B3.2 diagnostic seed panel (not a golden test): per-seed family,
    /// Re, n, core, excess, c, the raw body bounding box (40×40 canvas
    /// pixels), the raw post-support positive fraction (pre-render) and
    /// the post-render visible terminal-cell occupancy at 40×20 — the
    /// B3.1-vs-B3.2 comparison panel for the milestone diagnostic seeds.
    #[allow(clippy::print_literal)]
    #[test]
    fn test_b32_seed_panel_diagnostic() {
        let panel: [u64; 6] = [207, 1976, 3215, 2714, 2205, 2302];
        let (width, terminal_height) = (40usize, 20usize);
        println!("B3.2 seed panel (diagnostic):");
        for (tag, w, h) in [
            ("40x20", width, terminal_height),
            ("60x30", 60usize, 30usize),
        ] {
            let rh = h * 2;
            println!("  geometry {tag}: canvas {w}x{rh} px");
            for seed in panel {
                let config = EllipticalGalaxyConfig::for_scene_seed(seed);
                let profile = elliptical_density_profile(w, rh, config);
                let raw_positive = profile
                    .data
                    .iter()
                    .filter(|&&v| v.is_finite() && v > 0.0)
                    .count() as f64
                    / (w * rh) as f64;
                let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
                for y in 0..rh {
                    for x in 0..w {
                        if profile.get(x, y) > 0.0 {
                            x0 = x0.min(x);
                            y0 = y0.min(y);
                            x1 = x1.max(x);
                            y1 = y1.max(y);
                        }
                    }
                }
                let scene = ArtModel::Elliptical.generate_scene(w, h, Some(seed));
                let visible = post_render_visible_occupancy(&scene.density);
                println!(
                    "    seed {seed:>5} {:<13} Re={:.3} n={:.2} core={:.3} excess={:.3} c={:+.6} | bbox {:>2}..{:<2} x {:>2}..{:<2} | raw+={:.4} | visible={:.4}",
                    family_name(config.family),
                    config.effective_radius,
                    config.profile_index,
                    config.core_softening_fraction,
                    config.central_excess,
                    config.isophote_shape,
                    x0,
                    x1,
                    y0,
                    y1,
                    raw_positive,
                    visible
                );
            }
        }
        // Raw radial profile at fixed Re-fractions, before vs after the
        // central structure (r in Re units, shaped-radius coordinate),
        // for the two primary stress seeds.
        for seed in [207u64, 1976] {
            let config = EllipticalGalaxyConfig::for_scene_seed(seed);
            println!(
                "B3.2 radial profile seed {seed} (core={:.3}, excess={:.3}, n={:.2}, Re={:.3}), r in Re units:",
                config.core_softening_fraction,
                config.central_excess,
                config.profile_index,
                config.effective_radius
            );
            println!("  {:>6} | {:>12} | {:>12}", "r/Re", "before", "after");
            for &rho in &[0.0_f64, 0.05, 0.10, 0.20, 0.30] {
                let r = rho * config.effective_radius;
                let before = sersic_body_profile(r, config.effective_radius, config.profile_index);
                let after = apply_central_structure(
                    before,
                    r,
                    config.effective_radius,
                    config.profile_index,
                    config.core_softening_fraction,
                    config.central_excess,
                );
                println!("  {rho:>6.2} | {before:>12.6e} | {after:>12.6e}");
            }
        }
    }

    /// Post-render visible terminal-cell occupancy: runs the scene
    /// density through the production pipeline (robust normalization,
    /// gamma stretch, target-occupancy threshold) and counts vertical
    /// pair-maxima at or above the threshold — the same quantity
    /// `test_canonical_occupancy` reports for the renderer sanity band.
    fn post_render_visible_occupancy(density: &DensityMap) -> f64 {
        let profile = RenderProfile::for_model(ArtModel::Elliptical);
        let prepared = prepare_density(density.clone(), profile);
        let (processed, threshold) = match prepared {
            PreparedDensity::Starfield { density } => (density, 0.0),
            PreparedDensity::Galaxy { density, threshold } => (density, threshold),
        };
        let mut visible = 0usize;
        let mut total = 0usize;
        for y in (0..processed.height).step_by(2) {
            for x in 0..processed.width {
                let pair_max = processed.get(x, y).max(processed.get(x, y + 1));
                total += 1;
                if pair_max.is_finite() && pair_max > 0.0 && pair_max >= threshold {
                    visible += 1;
                }
            }
        }
        visible as f64 / total as f64
    }
}
