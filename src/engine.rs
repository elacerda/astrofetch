use crate::density::DensityMap;
use crate::galaxy::generate_spiral_galaxy_with_shape;
use crate::render::topology::CellSamplingShape;
use crate::seed::GenerationContext;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

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

/// Pedido de cena resolvido: seed concreto + modelo concreto.
///
/// Representação interna (Phase 5B.1) que separa a resolução do pedido da
/// geração de densidade. O seed é concretizado exatamente uma vez, na
/// resolução; a geração de densidade sempre consome este seed concreto e
/// nunca re-roda `rand::random`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedScene {
    /// O modelo concreto a gerar (nunca Random).
    pub resolved_model: ArtModel,
    /// O seed concreto compartilhado pela resolução e pela geração.
    pub seed: u64,
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
                models[rng.random_range(0..models.len())]
            }
            model => *model,
        }
    }

    /// Resolve um pedido de cena em um seed concreto e um modelo concreto.
    ///
    /// Este é o único ponto onde `Option<u64>` vira seed concreto:
    /// `seed.unwrap_or_else(rand::random)` acontece exatamente uma vez por
    /// pedido de cena. O RNG de seleção é semeado com esse seed concreto e
    /// usado apenas para a resolução do modelo; ele nunca é compartilhado
    /// com a geração de densidade.
    pub fn resolve_scene(&self, seed: Option<u64>) -> ResolvedScene {
        let seed = seed.unwrap_or_else(rand::random);
        let mut selection_rng = StdRng::seed_from_u64(seed);
        let resolved_model = self.resolve(&mut selection_rng);

        ResolvedScene {
            resolved_model,
            seed,
        }
    }

    /// Gera o mapa de densidade de um pedido de cena já resolvido.
    ///
    /// O RNG de geração e o `GenerationContext` são criados a partir de
    /// `resolved.seed`, de forma independente do RNG de seleção usado em
    /// [`ArtModel::resolve_scene`]. O `shape` é encaminhado apenas ao
    /// gerador de Spiral; os demais modelos mantêm a geometria legada fixa.
    pub fn generate_density(
        &self,
        resolved: &ResolvedScene,
        width: usize,
        height: usize,
        shape: CellSamplingShape,
    ) -> DensityMap {
        // Cria um novo RNG com o mesmo seed concreto para geração da cena.
        // Isso garante que o estado do RNG não seja afetado pela seleção do modelo.
        let mut generation_rng = StdRng::seed_from_u64(resolved.seed);
        let generation_context = GenerationContext::new(resolved.seed);

        let width = width.max(1);
        let height = height.max(1);
        let render_height = height * 2;

        // resolved_model nunca será Random porque resolve_scene() já o remove
        match resolved.resolved_model {
            ArtModel::Starfield => {
                let canvas = generate_starfield(width, render_height, &mut generation_rng);
                DensityMap::from_rows(canvas).unwrap()
            }
            ArtModel::Elliptical => {
                generate_elliptical_density(width, render_height, &mut generation_rng)
            }
            ArtModel::Spiral => generate_spiral_galaxy_with_shape(
                width,
                height,
                &mut generation_rng,
                generation_context,
                shape,
            ),
            ArtModel::Cluster => {
                let canvas = generate_cluster(width, render_height, &mut generation_rng);
                DensityMap::from_rows(canvas).unwrap()
            }
            ArtModel::Random => {
                // Este caso nunca deve ser alcançado porque resolve() sempre
                // escolhe um modelo concreto. Se isso acontecer, é um bug.
                panic!("Internal error: Random model should have been resolved")
            }
        }
    }

    /// Gera uma cena com metadados preservados.
    ///
    /// O fluxo de RNG é explícito:
    /// 1. Um RNG é criado com o seed para selecionar o modelo concreto.
    /// 2. Um novo RNG é criado com o mesmo seed para gerar a cena.
    /// 3. O seed base é passado separadamente como contexto para features isoladas.
    ///
    /// Isso preserva o comportamento determinístico existente sem obrigar novas
    /// features a consumir o stream de RNG legado.
    ///
    /// Caminho legado compatível: resolve o pedido via
    /// [`ArtModel::resolve_scene`] e gera a densidade via
    /// [`ArtModel::generate_density`] com a topologia de produção
    /// `CellSamplingShape::HALF_BLOCK`.
    pub fn generate_scene(&self, width: usize, height: usize, seed: Option<u64>) -> GeneratedScene {
        let resolved = self.resolve_scene(seed);
        let density =
            self.generate_density(&resolved, width, height, CellSamplingShape::HALF_BLOCK);

        GeneratedScene {
            requested_model: *self,
            resolved_model: resolved.resolved_model,
            seed: resolved.seed,
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
        let x = rng.random_range(0..width);
        let y = rng.random_range(0..height);
        let brightness: f64 = rng.random_range(0.040_f64..0.180_f64);
        canvas[y][x] = canvas[y][x].max(brightness);
    }

    // Tiny invisible seed signature so the renderer-derived star overlay varies
    // by seed even when the field is mostly empty.
    let seed_signature: f64 = rng.random_range(0.001_f64..0.004_f64);
    canvas[0][0] = canvas[0][0].max(seed_signature);

    canvas
}

/// Gera uma galáxia elíptica com elipticidade e rotação.
fn generate_elliptical_density(width: usize, height: usize, rng: &mut StdRng) -> DensityMap {
    let mut map = DensityMap::new(width, height);

    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;

    let ellipticity = 0.2 + rng.random_range(0.0..1.0) * 0.6;
    let rotation = rng.random_range(0.0..std::f64::consts::PI);

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
                value += rng.random_range(-0.012_f64..0.012_f64);
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

    let num_stars = 34 + rng.random_range(0..56);

    for _ in 0..num_stars {
        let angle = rng.random_range(0.0..2.0 * std::f64::consts::PI);
        let r = rng.random_range(0.0_f64..1.0).powf(1.75) * 0.44;

        let x = (center_x + r * width as f64 * angle.cos()) as usize;
        let y = (center_y + r * height as f64 * angle.sin()) as usize;

        if x < width && y < height {
            let brightness = rng.random_range(0.16_f64..0.95_f64);
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
            *value += rng.random_range(-0.006_f64..0.006_f64);
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

    // ---- Phase 5B.1: split de resolução de cena x geração de densidade ----

    /// Tabela congelada pré-refator: resolução de `ArtModel::Random` para
    /// seeds representativos, capturada da implementação legada de
    /// `generate_scene` antes do split 5B.1.
    const RANDOM_RESOLUTION_FREEZE: &[(u64, ArtModel)] = &[
        (0, ArtModel::Cluster),
        (1, ArtModel::Cluster),
        (4, ArtModel::Spiral),
        (7, ArtModel::Elliptical),
        (16, ArtModel::Cluster),
        (42, ArtModel::Starfield),
        (100, ArtModel::Cluster),
        (999, ArtModel::Spiral),
        (12345, ArtModel::Spiral),
    ];

    #[test]
    fn test_resolve_scene_fixed_seed_model_resolution() {
        // Random: deve bater com a tabela congelada pré-refator.
        for &(seed, expected) in RANDOM_RESOLUTION_FREEZE {
            let resolved = ArtModel::Random.resolve_scene(Some(seed));
            assert_eq!(
                resolved.resolved_model, expected,
                "Random seed {seed} deve resolver como o algoritmo legado"
            );
            assert_eq!(resolved.seed, seed);
        }

        // Modelos concretos: resolução é identidade e preserva o seed.
        for model in [
            ArtModel::Elliptical,
            ArtModel::Spiral,
            ArtModel::Cluster,
            ArtModel::Starfield,
        ] {
            for &(seed, _) in RANDOM_RESOLUTION_FREEZE {
                let resolved = model.resolve_scene(Some(seed));
                assert_eq!(resolved.resolved_model, model, "{model:?} seed {seed}");
                assert_eq!(resolved.seed, seed);
            }
        }
    }

    #[test]
    fn test_generate_scene_equals_resolve_plus_density() {
        // Para todos os modelos (incluindo Random) e seeds representativos,
        // o wrapper legado deve ser igual a resolve_scene +
        // generate_density(HALF_BLOCK), comparando metadados e densidade.
        for model in [
            ArtModel::Random,
            ArtModel::Elliptical,
            ArtModel::Spiral,
            ArtModel::Cluster,
            ArtModel::Starfield,
        ] {
            for &(seed, _) in RANDOM_RESOLUTION_FREEZE {
                let legacy = model.generate_scene(30, 15, Some(seed));

                let resolved = model.resolve_scene(Some(seed));
                let density =
                    model.generate_density(&resolved, 30, 15, CellSamplingShape::HALF_BLOCK);

                assert_eq!(legacy.requested_model, model, "{model:?} seed {seed}");
                assert_eq!(
                    legacy.resolved_model, resolved.resolved_model,
                    "{model:?} seed {seed}"
                );
                assert_eq!(legacy.seed, resolved.seed, "{model:?} seed {seed}");
                assert_eq!(legacy.density, density, "{model:?} seed {seed}");
            }
        }
    }

    #[test]
    fn test_resolve_scene_none_concretizes_seed_once() {
        // Um pedido resolvido a partir de None carrega um seed concreto, e
        // re-resolver com esse seed capturado reproduz o mesmo pedido.
        let resolved = ArtModel::Random.resolve_scene(None);
        let re_resolved = ArtModel::Random.resolve_scene(Some(resolved.seed));
        assert_eq!(re_resolved, resolved);

        // A geração a partir do pedido resolvido é reproduzível e não
        // re-concretiza o seed (a API só aceita seed concreto).
        let density_a =
            ArtModel::Random.generate_density(&resolved, 30, 15, CellSamplingShape::HALF_BLOCK);
        let density_b =
            ArtModel::Random.generate_density(&resolved, 30, 15, CellSamplingShape::HALF_BLOCK);
        assert_eq!(density_a, density_b);

        // E bate com o caminho legado dirigido pelo seed capturado.
        let legacy = ArtModel::Random.generate_scene(30, 15, Some(resolved.seed));
        assert_eq!(legacy.resolved_model, resolved.resolved_model);
        assert_eq!(legacy.density, density_a);
    }

    #[test]
    fn test_spiral_half_block_split_bit_identical() {
        // O caminho split deve permanecer bit-a-bit idêntico ao
        // generate_scene legado para os seeds congelados da Phase 5A
        // (unbarred 4, barred 16, barred+dusty 42).
        for seed in [4_u64, 16, 42] {
            let legacy = ArtModel::Spiral.generate_scene(30, 15, Some(seed));
            let resolved = ArtModel::Spiral.resolve_scene(Some(seed));
            let density =
                ArtModel::Spiral.generate_density(&resolved, 30, 15, CellSamplingShape::HALF_BLOCK);
            assert_eq!(legacy.density, density, "seed {seed}");
            assert_eq!(density.width, 30);
            assert_eq!(density.height, 30);
        }
    }

    #[test]
    fn test_non_spiral_split_preserves_geometry_and_rng() {
        // Starfield/Elliptical/Cluster mantêm a geometria legada W×2H e a
        // saída determinística pelo caminho split; o shape não os afeta.
        for model in [ArtModel::Starfield, ArtModel::Elliptical, ArtModel::Cluster] {
            let legacy = model.generate_scene(30, 15, Some(42));
            let resolved = model.resolve_scene(Some(42));
            let density = model.generate_density(&resolved, 30, 15, CellSamplingShape::HALF_BLOCK);
            assert_eq!(density.width, 30, "{model:?}");
            assert_eq!(density.height, 30, "{model:?}"); // height * 2
            assert_eq!(legacy.density, density, "{model:?}");

            // Em 5B.1 a geometria não-Spiral não consome QUADRANT.
            let quadrant = model.generate_density(&resolved, 30, 15, CellSamplingShape::QUADRANT);
            assert_eq!(quadrant, density, "{model:?} deve ignorar shape em 5B.1");
        }
    }

    #[test]
    fn test_generate_density_forwards_shape_to_spiral() {
        // Prova em nível de engine de que generate_density encaminha o shape
        // ao gerador de Spiral: QUADRANT produz o campo 2W×2H. Sem render.
        let resolved = ArtModel::Spiral.resolve_scene(Some(42));
        let quadrant =
            ArtModel::Spiral.generate_density(&resolved, 30, 15, CellSamplingShape::QUADRANT);
        assert_eq!((quadrant.width, quadrant.height), (60, 30));

        let half =
            ArtModel::Spiral.generate_density(&resolved, 30, 15, CellSamplingShape::HALF_BLOCK);
        assert_eq!((half.width, half.height), (30, 30));
    }
}
