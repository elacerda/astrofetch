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

/// Gera uma galáxia elíptica com elipticidade e rotação.
#[allow(clippy::needless_range_loop)]
fn generate_elliptical(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut canvas = vec![vec![0.0; width]; height];

    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;

    // Determina elipticidade e rotação de forma determinística a partir da seed
    // Elipticidade: 0.0 (circular) a 0.8 (muito alongada)
    let ellipticity = 0.2 + (rng.gen_range(0.0..1.0) * 0.6);
    // Rotação em radianos
    let rotation = rng.gen_range(0.0..std::f64::consts::PI);

    let cos_rot = rotation.cos();
    let sin_rot = rotation.sin();

    // Fatores de escala para elipticidade
    let a = 1.0; // Eixo maior
    let b = 1.0 - ellipticity * 0.5; // Eixo menor

    for y in 0..height {
        for x in 0..width {
            // Coordenadas relativas ao centro
            let dx = (x as f64 - center_x) / width as f64;
            let dy = (y as f64 - center_y) / height as f64;

            // Aplica rotação inversa
            let x_rot = dx * cos_rot + dy * sin_rot;
            let y_rot = -dx * sin_rot + dy * cos_rot;

            // Aplica elipticidade
            let x_elliptical = x_rot / a;
            let y_elliptical = y_rot / b;

            let r = (x_elliptical * x_elliptical + y_elliptical * y_elliptical).sqrt();
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

/// Gera uma galáxia espiral com braços variáveis, inclinação e rotação.
#[allow(clippy::needless_range_loop)]
fn generate_spiral(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut canvas = vec![vec![0.0; width]; height];

    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;

    // Determina número de braços (2 a 6) de forma determinística
    let num_arms = 2 + (rng.gen_range(0..5) as usize % 5); // 2, 3, 4, 5, ou 6

    // Determina inclinação (0.0 a 0.8)
    let inclination = rng.gen_range(0.0..0.8);
    // Determina rotação
    let rotation = rng.gen_range(0.0..std::f64::consts::PI);

    let cos_rot = rotation.cos();
    let sin_rot = rotation.sin();

    // Fator de correção de aspecto
    let aspect = width as f64 / height as f64;

    // Pitch angle (quão aberto são os braços)
    let pitch_angle = 0.2 + rng.gen_range(0.0..0.3);

    for y in 0..height {
        for x in 0..width {
            // Coordenadas relativas ao centro
            let dx = (x as f64 - center_x) / aspect;
            let dy = y as f64 - center_y;

            // Aplica rotação
            let x_rot = dx * cos_rot + dy * sin_rot;
            let y_rot = -dx * sin_rot + dy * cos_rot;

            // Aplica inclinação (compressão no eixo y)
            let y_inclined = y_rot * (1.0 - inclination * 0.5);

            let r = (x_rot * x_rot + y_inclined * y_inclined).sqrt();
            let theta = y_inclined.atan2(x_rot);

            let mut intensity = 0.0;

            // Adiciona cada braço espiral
            let arm_spacing = 2.0 * std::f64::consts::PI / num_arms as f64;

            for arm in 0..num_arms {
                let arm_angle = arm as f64 * arm_spacing;
                // Equação da espiral: theta = a + b * r
                let angle_diff =
                    (theta - arm_angle - pitch_angle * r).rem_euclid(2.0 * std::f64::consts::PI);
                // Função gaussiana para suavidade dos braços
                let spiral = (-(angle_diff.abs() * 1.5).powf(2.0)).exp();
                intensity += spiral;
            }

            // Núcleo brilhante
            let core = (-(r * 2.0).powf(2.0)).exp() * 0.4;

            canvas[y][x] = (intensity + core).min(1.0);
        }
    }

    // Ruído
    for y in 0..height {
        for x in 0..width {
            canvas[y][x] += rng.gen_range(-0.04_f64..0.04_f64);
            canvas[y][x] = canvas[y][x].clamp(0.0_f64, 1.0_f64);
        }
    }

    canvas
}

/// Gera um aglomerado de estrelas.
#[allow(clippy::needless_range_loop)]
fn generate_cluster(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut canvas = vec![vec![0.0; width]; height];

    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;

    // Número de estrelas no cluster
    let num_stars = 50 + rng.gen_range(0..100);

    for _ in 0..num_stars {
        // Distribuição radial (mais estrelas no centro)
        let angle = rng.gen_range(0.0..2.0 * std::f64::consts::PI);
        let r = rng.gen_range(0.0_f64..1.0).sqrt() * 0.4; // Distribuição radial uniforme

        let x = (center_x + r * width as f64 * angle.cos()) as usize;
        let y = (center_y + r * height as f64 * angle.sin()) as usize;

        if x < width && y < height {
            let brightness = rng.gen_range(0.2..1.0);
            canvas[y][x] = brightness;
        }
    }

    // Nébula suave ao redor
    for y in 0..height {
        for x in 0..width {
            let dx = (x as f64 - center_x) / width as f64;
            let dy = (y as f64 - center_y) / height as f64;
            let dist = (dx * dx + dy * dy).sqrt();
            let nebula = (1.0 - dist).max(0.0).powf(4.0) * 0.15;
            canvas[y][x] = (canvas[y][x] + nebula).min(1.0);
        }
    }

    // Ruído
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
        // Uma galáxia espiral deve ter intensidade não uniforme
        let canvas = ArtModel::Spiral.generate(30, 15, Some(42));

        // Deve ter pelo menos um pixel com intensidade > 0.1
        let has_structure = canvas.iter().any(|row| row.iter().any(|&v| v > 0.1));
        assert!(has_structure);
    }

    #[test]
    fn test_elliptical_has_structure() {
        let canvas = ArtModel::Elliptical.generate(30, 15, Some(42));

        let has_structure = canvas.iter().any(|row| row.iter().any(|&v| v > 0.1));
        assert!(has_structure);
    }
}
