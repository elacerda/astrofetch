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

/// Cena gerada com metadados preservados.
#[derive(Debug, Clone)]
pub struct GeneratedScene {
    /// O modelo solicitado pelo usuário (pode ser Random).
    #[allow(dead_code)]
    pub requested_model: ArtModel,
    /// O modelo concreto resolvido (nunca Random).
    pub resolved_model: ArtModel,
    /// O seed efetivamente usado.
    #[allow(dead_code)]
    pub seed: u64,
    /// O mapa de densidade gerado.
    pub density: DensityMap,
}

impl ArtModel {
    /// Resolve o modelo solicitado para um modelo concreto.
    ///
    /// Se o modelo for Random, escolhe um dos 4 modelos concretos.
    /// O modelo retornado nunca será Random.
    fn resolve(&self, rng: &mut StdRng) -> ArtModel {
        match self {
            ArtModel::Random => {
                let models = [
                    ArtModel::Starfield,
                    ArtModel::Elliptical,
                    ArtModel::Spiral,
                    ArtModel::Cluster,
                ];
                models[rng.gen_range(0..models.len())]
            }
            model => *model,
        }
    }

    /// Gera uma cena com metadados preservados.
    ///
    /// O fluxo de RNG é explícito:
    /// 1. Um RNG é criado com o seed para selecionar o modelo concreto.
    /// 2. Um novo RNG é criado com o mesmo seed para gerar a cena.
    ///
    /// Isso preserva o comportamento determinístico existente.
    pub fn generate_scene(&self, width: usize, height: usize, seed: Option<u64>) -> GeneratedScene {
        let seed = seed.unwrap_or_else(rand::random);
        let mut selection_rng = StdRng::seed_from_u64(seed);
        let resolved_model = self.resolve(&mut selection_rng);

        // Cria um novo RNG com o mesmo seed para geração da cena
        // Isso garante que o estado do RNG não seja afetado pela seleção do modelo
        let mut generation_rng = StdRng::seed_from_u64(seed);

        let width = width.max(1);
        let height = height.max(1);
        let render_height = height * 2;

        // resolved_model nunca será Random porque resolve() já o remove
        let density = match resolved_model {
            ArtModel::Starfield => {
                let canvas = generate_starfield(width, render_height, &mut generation_rng);
                DensityMap::from_rows(canvas).unwrap()
            }
            ArtModel::Elliptical => {
                generate_elliptical_density(width, render_height, &mut generation_rng)
            }
            ArtModel::Spiral => generate_spiral_galaxy(width, height, &mut generation_rng),
            ArtModel::Cluster => {
                let canvas = generate_cluster(width, render_height, &mut generation_rng);
                DensityMap::from_rows(canvas).unwrap()
            }
            ArtModel::Random => {
                // Este caso nunca deve ser alcançado porque resolve() sempre
                // escolhe um modelo concreto. Se isso acontecer, é um bug.
                panic!("Internal error: Random model should have been resolved")
            }
        };

        GeneratedScene {
            requested_model: *self,
            resolved_model,
            seed,
            density,
        }
    }
}

/// Gera um campo de estrelas simples.
fn generate_starfield(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut canvas: Vec<Vec<f64>> = vec![vec![0.0_f64; width]; height];

    // Keep the density map very sparse. The renderer will add ASCII point
    // stars over empty cells, so this model should not fill the whole canvas.
    let num_stars = (width * height / 64).max(8);
    for _ in 0..num_stars {
        let x = rng.gen_range(0..width);
        let y = rng.gen_range(0..height);
        let brightness: f64 = rng.gen_range(0.040_f64..0.180_f64);
        canvas[y][x] = canvas[y][x].max(brightness);
    }

    // Tiny invisible seed signature so the renderer-derived star overlay varies
    // by seed even when the field is mostly empty.
    let seed_signature: f64 = rng.gen_range(0.001_f64..0.004_f64);
    canvas[0][0] = canvas[0][0].max(seed_signature);

    canvas
}

/// Gera uma galáxia elíptica com elipticidade e rotação.
fn generate_elliptical_density(width: usize, height: usize, rng: &mut StdRng) -> DensityMap {
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
            let mut value = map.get(x, y);

            // Cut very faint outskirts so the renderer does not turn the whole
            // terminal area into a noisy filled cloud.
            if value < 0.018 {
                value = 0.0;
            }

            // Very light grain only where the galaxy is actually visible.
            if value > 0.0 {
                value += rng.gen_range(-0.012_f64..0.012_f64);
            }

            map.set(x, y, value.clamp(0.0_f64, 1.0_f64));
        }
    }

    map
}

/// Gera um aglomerado de estrelas.
fn generate_cluster(width: usize, height: usize, rng: &mut StdRng) -> Vec<Vec<f64>> {
    let mut canvas = vec![vec![0.0; width]; height];

    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;

    let num_stars = 34 + rng.gen_range(0..56);

    for _ in 0..num_stars {
        let angle = rng.gen_range(0.0..2.0 * std::f64::consts::PI);
        let r = rng.gen_range(0.0_f64..1.0).powf(1.75) * 0.44;

        let x = (center_x + r * width as f64 * angle.cos()) as usize;
        let y = (center_y + r * height as f64 * angle.sin()) as usize;

        if x < width && y < height {
            let brightness = rng.gen_range(0.16_f64..0.95_f64);
            canvas[y][x] = brightness;
        }
    }

    for (y, row) in canvas.iter_mut().enumerate() {
        for (x, value) in row.iter_mut().enumerate() {
            let dx = (x as f64 - center_x) / width as f64;
            let dy = (y as f64 - center_y) / height as f64;
            let dist = (dx * dx + dy * dy).sqrt();
            let nebula = (1.0 - dist / 0.30).max(0.0).powf(5.5) * 0.045;
            *value = (*value + nebula).min(1.0);
        }
    }

    for row in &mut canvas {
        for value in row {
            // Add light noise only where there is structure
            *value += rng.gen_range(-0.006_f64..0.006_f64);
            // Clamp only negative values to zero (no positive display cutoff)
            *value = value.clamp(0.0_f64, 1.0_f64);
        }
    }

    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_random_never_returns_random() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let resolved = ArtModel::Random.resolve(&mut rng);
            assert_ne!(
                resolved,
                ArtModel::Random,
                "Random should never resolve to Random"
            );
        }
    }

    #[test]
    fn test_resolve_explicit_model_returns_itself() {
        let mut rng = StdRng::seed_from_u64(42);
        assert_eq!(ArtModel::Starfield.resolve(&mut rng), ArtModel::Starfield);
        assert_eq!(ArtModel::Elliptical.resolve(&mut rng), ArtModel::Elliptical);
        assert_eq!(ArtModel::Spiral.resolve(&mut rng), ArtModel::Spiral);
        assert_eq!(ArtModel::Cluster.resolve(&mut rng), ArtModel::Cluster);
    }

    #[test]
    fn test_resolve_random_deterministic() {
        // Testa que o mesmo seed resolve para o mesmo modelo
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        assert_eq!(
            ArtModel::Random.resolve(&mut rng1),
            ArtModel::Random.resolve(&mut rng2)
        );
    }

    #[test]
    fn test_generate_scene_preserves_requested_model() {
        let scene = ArtModel::Starfield.generate_scene(20, 10, Some(42));
        assert_eq!(scene.requested_model, ArtModel::Starfield);

        let scene = ArtModel::Random.generate_scene(20, 10, Some(42));
        assert_eq!(scene.requested_model, ArtModel::Random);
    }

    #[test]
    fn test_generate_scene_preserves_seed() {
        let scene = ArtModel::Starfield.generate_scene(20, 10, Some(42));
        assert_eq!(scene.seed, 42);
    }

    #[test]
    fn test_generate_scene_seed_roundtrip() {
        // Gera com seed 42, depois gera novamente com o seed retornado
        let scene1 = ArtModel::Starfield.generate_scene(20, 10, Some(42));
        let scene2 = ArtModel::Starfield.generate_scene(20, 10, Some(scene1.seed));
        assert_eq!(scene1.seed, scene2.seed);
        assert_eq!(scene1.density, scene2.density);
    }

    #[test]
    fn test_generate_scene_random_seed_roundtrip() {
        // Gera com None, depois gera novamente com o seed retornado
        let scene1 = ArtModel::Random.generate_scene(20, 10, None);
        let scene2 = ArtModel::Random.generate_scene(20, 10, Some(scene1.seed));
        assert_eq!(scene1.resolved_model, scene2.resolved_model);
        assert_eq!(scene1.density, scene2.density);
    }

    #[test]
    fn test_generate_scene_deterministic() {
        let scene1 = ArtModel::Starfield.generate_scene(20, 10, Some(42));
        let scene2 = ArtModel::Starfield.generate_scene(20, 10, Some(42));
        assert_eq!(scene1.density, scene2.density);
        assert_eq!(scene1.resolved_model, scene2.resolved_model);
    }

    #[test]
    fn test_generate_scene_different_seeds_different() {
        let scene1 = ArtModel::Starfield.generate_scene(20, 10, Some(42));
        let scene2 = ArtModel::Starfield.generate_scene(20, 10, Some(43));
        assert_ne!(scene1.density, scene2.density);
    }

    #[test]
    fn test_generate_scene_dimensions() {
        let scene = ArtModel::Starfield.generate_scene(20, 10, Some(42));
        assert_eq!(scene.density.width, 20);
        assert_eq!(scene.density.height, 20); // height * 2
    }

    #[test]
    fn test_generate_scene_spiral_dimensions() {
        let scene = ArtModel::Spiral.generate_scene(30, 15, Some(42));
        assert_eq!(scene.density.width, 30);
        assert_eq!(scene.density.height, 30); // height * 2
    }

    #[test]
    fn test_generate_scene_elliptical_dimensions() {
        let scene = ArtModel::Elliptical.generate_scene(30, 15, Some(42));
        assert_eq!(scene.density.width, 30);
        assert_eq!(scene.density.height, 30); // height * 2
    }

    #[test]
    fn test_generate_scene_cluster_dimensions() {
        let scene = ArtModel::Cluster.generate_scene(30, 15, Some(42));
        assert_eq!(scene.density.width, 30);
        assert_eq!(scene.density.height, 30); // height * 2
    }

    #[test]
    fn test_deterministic_spiral() {
        let canvas1 = ArtModel::Spiral
            .generate_scene(20, 10, Some(42))
            .density
            .into_rows();
        let canvas2 = ArtModel::Spiral
            .generate_scene(20, 10, Some(42))
            .density
            .into_rows();
        assert_eq!(canvas1, canvas2);
    }

    #[test]
    fn test_deterministic_elliptical() {
        let canvas1 = ArtModel::Elliptical
            .generate_scene(20, 10, Some(42))
            .density
            .into_rows();
        let canvas2 = ArtModel::Elliptical
            .generate_scene(20, 10, Some(42))
            .density
            .into_rows();
        assert_eq!(canvas1, canvas2);
    }

    #[test]
    fn test_different_seeds_different_art() {
        let canvas1 = ArtModel::Spiral
            .generate_scene(20, 10, Some(42))
            .density
            .into_rows();
        let canvas2 = ArtModel::Spiral
            .generate_scene(20, 10, Some(43))
            .density
            .into_rows();
        assert_ne!(canvas1, canvas2);
    }

    #[test]
    fn test_different_seeds_different_elliptical() {
        let canvas1 = ArtModel::Elliptical
            .generate_scene(20, 10, Some(42))
            .density
            .into_rows();
        let canvas2 = ArtModel::Elliptical
            .generate_scene(20, 10, Some(43))
            .density
            .into_rows();
        assert_ne!(canvas1, canvas2);
    }

    #[test]
    fn test_spiral_has_arms() {
        let canvas = ArtModel::Spiral
            .generate_scene(30, 15, Some(42))
            .density
            .into_rows();
        let has_structure = canvas.iter().any(|row| row.iter().any(|&v| v > 0.1));
        assert!(has_structure);
    }

    #[test]
    fn test_elliptical_has_structure() {
        let canvas = ArtModel::Elliptical
            .generate_scene(30, 15, Some(42))
            .density
            .into_rows();
        let has_structure = canvas.iter().any(|row| row.iter().any(|&v| v > 0.1));
        assert!(has_structure);
    }

    #[test]
    fn test_generate_uses_double_vertical_resolution() {
        let canvas = ArtModel::Spiral
            .generate_scene(30, 15, Some(42))
            .density
            .into_rows();
        assert_eq!(canvas.len(), 30);
        assert_eq!(canvas[0].len(), 30);
    }
}
