use crate::density::DensityMap;
use crate::galaxy::generate_spiral_galaxy;
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
    ///
    /// The legacy return type is kept for compatibility with the rest of the app,
    /// but the spiral model is now generated through a row-major DensityMap and
    /// returned at double vertical resolution for half-block rendering.
    pub fn generate(&self, width: usize, height: usize, seed: Option<u64>) -> Vec<Vec<f64>> {
        let seed = seed.unwrap_or_else(rand::random);
        let mut rng = StdRng::seed_from_u64(seed);

        let width = width.max(1);
        let render_height = height.max(1) * 2;

        match self {
            ArtModel::Starfield => generate_starfield(width, render_height, &mut rng),
            ArtModel::Elliptical => generate_elliptical(width, render_height, &mut rng),
            ArtModel::Spiral => generate_spiral_galaxy(width, height.max(1), &mut rng).into_rows(),
            ArtModel::Cluster => generate_cluster(width, render_height, &mut rng),
            ArtModel::Random => {
                let models = [
                    ArtModel::Starfield,
                    ArtModel::Elliptical,
                    ArtModel::Spiral,
                    ArtModel::Cluster,
                ];
                let model = models[rng.gen_range(0..models.len())];
                model.generate(width, height.max(1), Some(seed))
            }
        }
    }
}

/// Gera um campo de estrelas simples.
fn generate_starfield(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut canvas = vec![vec![0.0; width]; height];

    let num_stars = width * height / 6;
    for _ in 0..num_stars {
        let x = rng.gen_range(0..width);
        let y = rng.gen_range(0..height);
        let brightness = rng.gen_range(0.3..1.0);
        canvas[y][x] = brightness;
    }

    let center_x = width / 2;
    let center_y = height / 2;
    for (y, row) in canvas.iter_mut().enumerate() {
        for (x, value) in row.iter_mut().enumerate() {
            let dx = (x as f64 - center_x as f64) / width as f64;
            let dy = (y as f64 - center_y as f64) / height as f64;
            let dist = (dx * dx + dy * dy).sqrt();
            let nebula = (1.0 - dist).max(0.0) * 0.1;
            *value = (*value + nebula).min(1.0);
        }
    }

    canvas
}

/// Gera uma galáxia elíptica com elipticidade e rotação.
fn generate_elliptical(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut map = DensityMap::new(width, height);

    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;

    let ellipticity = 0.2 + rng.gen_range(0.0..1.0) * 0.6;
    let rotation = rng.gen_range(0.0..std::f64::consts::PI);

    let cos_rot = rotation.cos();
    let sin_rot = rotation.sin();

    let a = 1.0;
    let b = 1.0 - ellipticity * 0.5;

    for y in 0..height {
        for x in 0..width {
            let dx = (x as f64 - center_x) / width as f64;
            let dy = (y as f64 - center_y) / height as f64;

            let x_rot = dx * cos_rot + dy * sin_rot;
            let y_rot = -dx * sin_rot + dy * cos_rot;

            let x_elliptical = x_rot / a;
            let y_elliptical = y_rot / b;

            let r = (x_elliptical * x_elliptical + y_elliptical * y_elliptical).sqrt();
            let intensity = (-(r * 3.0).powf(2.0)).exp() * 0.8;
            let core = (-(r * 8.0).powf(2.0)).exp() * 0.3;

            map.set(x, y, (intensity + core).min(1.0));
        }
    }

    for y in 0..height {
        for x in 0..width {
            let value = map.get(x, y) + rng.gen_range(-0.05_f64..0.05_f64);
            map.set(x, y, value.clamp(0.0_f64, 1.0_f64));
        }
    }

    map.into_rows()
}

/// Gera um aglomerado de estrelas.
fn generate_cluster(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut canvas = vec![vec![0.0; width]; height];

    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;

    let num_stars = 50 + rng.gen_range(0..100);

    for _ in 0..num_stars {
        let angle = rng.gen_range(0.0..2.0 * std::f64::consts::PI);
        let r = rng.gen_range(0.0_f64..1.0).sqrt() * 0.4;

        let x = (center_x + r * width as f64 * angle.cos()) as usize;
        let y = (center_y + r * height as f64 * angle.sin()) as usize;

        if x < width && y < height {
            let brightness = rng.gen_range(0.2..1.0);
            canvas[y][x] = brightness;
        }
    }

    for (y, row) in canvas.iter_mut().enumerate() {
        for (x, value) in row.iter_mut().enumerate() {
            let dx = (x as f64 - center_x) / width as f64;
            let dy = (y as f64 - center_y) / height as f64;
            let dist = (dx * dx + dy * dy).sqrt();
            let nebula = (1.0 - dist).max(0.0).powf(4.0) * 0.15;
            *value = (*value + nebula).min(1.0);
        }
    }

    for row in &mut canvas {
        for value in row {
            *value += rng.gen_range(-0.03_f64..0.03_f64);
            *value = value.clamp(0.0_f64, 1.0_f64);
        }
    }

    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_spiral() {
        let canvas1 = ArtModel::Spiral.generate(20, 10, Some(42));
        let canvas2 = ArtModel::Spiral.generate(20, 10, Some(42));
        assert_eq!(canvas1, canvas2);
    }

    #[test]
    fn test_deterministic_elliptical() {
        let canvas1 = ArtModel::Elliptical.generate(20, 10, Some(42));
        let canvas2 = ArtModel::Elliptical.generate(20, 10, Some(42));
        assert_eq!(canvas1, canvas2);
    }

    #[test]
    fn test_different_seeds_different_art() {
        let canvas1 = ArtModel::Spiral.generate(20, 10, Some(42));
        let canvas2 = ArtModel::Spiral.generate(20, 10, Some(43));
        assert_ne!(canvas1, canvas2);
    }

    #[test]
    fn test_different_seeds_different_elliptical() {
        let canvas1 = ArtModel::Elliptical.generate(20, 10, Some(42));
        let canvas2 = ArtModel::Elliptical.generate(20, 10, Some(43));
        assert_ne!(canvas1, canvas2);
    }

    #[test]
    fn test_spiral_has_arms() {
        let canvas = ArtModel::Spiral.generate(30, 15, Some(42));
        let has_structure = canvas.iter().any(|row| row.iter().any(|&v| v > 0.1));
        assert!(has_structure);
    }

    #[test]
    fn test_elliptical_has_structure() {
        let canvas = ArtModel::Elliptical.generate(30, 15, Some(42));
        let has_structure = canvas.iter().any(|row| row.iter().any(|&v| v > 0.1));
        assert!(has_structure);
    }

    #[test]
    fn test_generate_uses_double_vertical_resolution() {
        let canvas = ArtModel::Spiral.generate(30, 15, Some(42));

        assert_eq!(canvas.len(), 30);
        assert_eq!(canvas[0].len(), 30);
    }
}
