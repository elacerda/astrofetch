use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Modelo de arte ASCII.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArtModel {
    Random,
    Elliptical,
    Spiral,
    Cluster,
    Starfield,
}

impl ArtModel {
    /// Gera uma matriz de luminosidade baseada no modelo.
    pub fn generate(&self, width: usize, height: usize, seed: Option<u64>) -> Vec<Vec<f64>> {
        let seed = seed.unwrap_or(rand::random());
        let mut rng = StdRng::seed_from_u64(seed);

        match self {
            ArtModel::Starfield => generate_starfield(width, height, &mut rng),
            ArtModel::Elliptical => generate_elliptical(width, height, &mut rng),
            ArtModel::Spiral => generate_spiral(width, height, &mut rng),
            ArtModel::Cluster => generate_cluster(width, height, &mut rng),
            ArtModel::Random => {
                let models = [
                    ArtModel::Starfield,
                    ArtModel::Elliptical,
                    ArtModel::Spiral,
                    ArtModel::Cluster,
                ];
                let model = models[rng.gen_range(0..models.len())];
                model.generate(width, height, Some(seed))
            }
        }
    }
}

/// Gera um campo de estrelas simples.
#[allow(clippy::needless_range_loop)]
fn generate_starfield(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut canvas = vec![vec![0.0; width]; height];

    // Estrelas aleatórias
    let num_stars = width * height / 4;
    for _ in 0..num_stars {
        let x = rng.gen_range(0..width);
        let y = rng.gen_range(0..height);
        let brightness = rng.gen_range(0.3..1.0);
        canvas[y][x] = brightness;
    }

    // Nébula suave
    let center_x = width / 2;
    let center_y = height / 2;
    for y in 0..height {
        for x in 0..width {
            let dx = (x as f64 - center_x as f64) / width as f64;
            let dy = (y as f64 - center_y as f64) / height as f64;
            let dist = (dx * dx + dy * dy).sqrt();
            let nebula = (1.0 - dist).max(0.0) * 0.1;
            canvas[y][x] = (canvas[y][x] + nebula).min(1.0);
        }
    }

    canvas
}

/// Gera uma galáxia elíptica.
#[allow(clippy::needless_range_loop)]
fn generate_elliptical(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut canvas = vec![vec![0.0; width]; height];

    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;

    for y in 0..height {
        for x in 0..width {
            let dx = (x as f64 - center_x) / width as f64;
            let dy = (y as f64 - center_y) / height as f64;

            let r = (dx * dx + dy * dy).sqrt();
            let intensity = (-(r * 3.0).powf(2.0)).exp() * 0.8;
            let core = (-(r * 8.0).powf(2.0)).exp() * 0.3;

            canvas[y][x] = (intensity + core).min(1.0);
        }
    }

    // Ruído
    for y in 0..height {
        for x in 0..width {
            canvas[y][x] += rng.gen_range(-0.05_f64..0.05_f64);
            canvas[y][x] = canvas[y][x].clamp(0.0_f64, 1.0_f64);
        }
    }

    canvas
}

/// Gera uma galáxia espiral.
#[allow(clippy::needless_range_loop)]
fn generate_spiral(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut canvas = vec![vec![0.0; width]; height];

    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;
    let aspect = width as f64 / height as f64;

    for y in 0..height {
        for x in 0..width {
            let dx = (x as f64 - center_x) / aspect;
            let dy = y as f64 - center_y;

            let r = (dx * dx + dy * dy).sqrt();
            let theta = dy.atan2(dx);

            // Braços espirais simples
            let arm_spacing = std::f64::consts::PI;
            let pitch_angle = 0.3;
            let mut intensity = 0.0;

            for arm in 0..2 {
                let arm_angle = arm as f64 * arm_spacing;
                let angle_diff =
                    (theta - arm_angle - pitch_angle * r).rem_euclid(2.0 * std::f64::consts::PI);
                let spiral = (-(angle_diff.abs() * 2.0).powf(2.0)).exp();
                intensity += spiral;
            }

            // Núcleo
            let core = (-(r * 3.0).powf(2.0)).exp() * 0.3;

            canvas[y][x] = (intensity + core).min(1.0);
        }
    }

    // Ruído
    for y in 0..height {
        for x in 0..width {
            canvas[y][x] += rng.gen_range(-0.05_f64..0.05_f64);
            canvas[y][x] = canvas[y][x].clamp(0.0_f64, 1.0_f64);
        }
    }

    canvas
}

/// Gera um aglomerado estelar.
#[allow(clippy::needless_range_loop)]
fn generate_cluster(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut canvas = vec![vec![0.0; width]; height];

    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;

    // Estrelas centrais
    let num_stars = 150;
    for _ in 0..num_stars {
        let angle = rng.gen_range(0.0..2.0 * std::f64::consts::PI);
        let r = rng.gen_range(0.0..1.0_f64).powf(0.5) * 0.3;

        let x = ((center_x + r * angle.cos() * width as f64) as usize).min(width - 1);
        let y = ((center_y + r * angle.sin() * height as f64) as usize).min(height - 1);

        let brightness = rng.gen_range(0.5..1.0);
        canvas[y][x] = brightness;
    }

    // Estrelas periféricas
    let num_outer = 50;
    for _ in 0..num_outer {
        let angle = rng.gen_range(0.0..2.0 * std::f64::consts::PI);
        let r = rng.gen_range(0.3..0.6);

        let x = ((center_x + r * angle.cos() * width as f64) as usize).min(width - 1);
        let y = ((center_y + r * angle.sin() * height as f64) as usize).min(height - 1);

        let brightness = rng.gen_range(0.2..0.6);
        canvas[y][x] = brightness;
    }

    // Ruído suave
    for y in 0..height {
        for x in 0..width {
            canvas[y][x] += rng.gen_range(-0.03_f64..0.03_f64);
            canvas[y][x] = canvas[y][x].clamp(0.0_f64, 1.0_f64);
        }
    }

    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_starfield() {
        let model = ArtModel::Starfield;
        let canvas1 = model.generate(10, 5, Some(42));
        let canvas2 = model.generate(10, 5, Some(42));
        assert_eq!(canvas1, canvas2);
    }

    #[test]
    fn test_different_seeds_produce_different_results() {
        let model = ArtModel::Starfield;
        let canvas1 = model.generate(10, 5, Some(42));
        let canvas2 = model.generate(10, 5, Some(99));
        assert_ne!(canvas1, canvas2);
    }
}
