use crate::density::DensityMap;
use noise::{NoiseFn, OpenSimplex};
use rand::rngs::StdRng;
use rand::Rng;

const TAU: f64 = std::f64::consts::PI * 2.0;

/// Tunable parameters for the analytic spiral galaxy model.
#[derive(Debug, Clone, Copy)]
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
    pub threshold_floor: f64,
}

impl SpiralGalaxyConfig {
    /// Draws deterministic-looking physical parameters from the seeded RNG.
    pub fn from_rng(rng: &mut StdRng) -> Self {
        Self {
            arms: rng.gen_range(2..=3),
            pitch: rng.gen_range(0.42..0.70),
            inclination_rad: rng.gen_range(0.70..1.05),
            rotation_rad: rng.gen_range(0.0..TAU),
            bulge_sigma: rng.gen_range(0.045..0.075),
            disk_scale: rng.gen_range(0.45..0.62),
            arm_width: rng.gen_range(0.018..0.034),
            arm_strength: rng.gen_range(2.0..3.4),
            noise_scale: rng.gen_range(3.5..6.0),
            threshold_floor: 0.026,
        }
    }
}

/// Generates a spiral galaxy as a high-resolution density field.
///
/// `terminal_height` is the number of terminal text rows requested by the user.
/// The returned map has twice that height because the renderer consumes two
/// density rows per visible terminal row via half-block glyphs.
pub fn generate_spiral_galaxy(
    terminal_width: usize,
    terminal_height: usize,
    rng: &mut StdRng,
) -> DensityMap {
    let config = SpiralGalaxyConfig::from_rng(rng);

    let out_width = terminal_width.max(1);
    let out_height = terminal_height.max(1) * 2;

    // Supersampling before binning. This is deliberately modest because this is
    // a CLI visual effect, not a scientific image pipeline.
    let sample = 3;
    let high_width = out_width * sample;
    let high_height = out_height * sample;

    let noise_seed = rng.gen::<u32>();
    let coarse_noise = OpenSimplex::new(noise_seed);
    let fine_noise = OpenSimplex::new(noise_seed.wrapping_add(1));

    let high = DensityMap::from_fn(high_width, high_height, |sx, sy| {
        let x = normalized_coord(sx, high_width);
        let y = normalized_coord(sy, high_height);

        spiral_density(x, y, &config, &coarse_noise, &fine_noise)
    });

    high.downsample_average(out_width, out_height)
        .normalize()
        .gamma_stretch(0.85)
}

fn normalized_coord(i: usize, n: usize) -> f64 {
    2.0 * ((i as f64 + 0.5) / n as f64 - 0.5)
}

fn spiral_density(
    x: f64,
    y: f64,
    config: &SpiralGalaxyConfig,
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

    // Simple inclined disk deprojection.
    let cos_i = config.inclination_rad.cos().abs().max(0.30);
    let yd = yr / cos_i;

    let r = (xr * xr + yd * yd).sqrt();
    if r > 1.20 {
        return 0.0;
    }

    let theta = yd.atan2(xr);

    let bulge = gaussian(r, config.bulge_sigma) * 0.30;
    let disk = (-r / config.disk_scale).exp() * 0.035;
    let arms = spiral_arm_density(r, theta, config);

    let coarse =
        normalized_noise(coarse_noise.get([xr * config.noise_scale, yd * config.noise_scale]));

    let fine = normalized_noise(
        fine_noise.get([xr * config.noise_scale * 5.0, yd * config.noise_scale * 5.0]),
    );

    let clumpiness = 0.45 + 1.35 * coarse.powf(1.4);
    let stellar_knots = fine.powf(8.0) * arms * 0.85;

    let value = (bulge + disk + arms * clumpiness + stellar_knots) - config.threshold_floor;
    value.max(0.0)
}

fn spiral_arm_density(r: f64, theta: f64, config: &SpiralGalaxyConfig) -> f64 {
    if r < 0.045 {
        return 0.0;
    }

    // Logarithmic spiral: r = a * exp(b * theta).
    // We invert it to compare the observed angle against the nearest arm angle.
    let a = 0.075;
    let b = config.pitch;
    let base_theta = (r / a).max(1.0e-4).ln() / b;

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
    use rand::SeedableRng;

    #[test]
    fn test_spiral_galaxy_is_deterministic() {
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);

        let map1 = generate_spiral_galaxy(30, 12, &mut rng1);
        let map2 = generate_spiral_galaxy(30, 12, &mut rng2);

        assert_eq!(map1, map2);
    }

    #[test]
    fn test_spiral_galaxy_uses_half_block_height() {
        let mut rng = StdRng::seed_from_u64(42);
        let map = generate_spiral_galaxy(30, 12, &mut rng);

        assert_eq!(map.width, 30);
        assert_eq!(map.height, 24);
        assert!(map.data.iter().any(|v| *v > 0.1));
    }
}
