mod ansi;
mod ascii;
mod color;
mod hash;
mod profile;
// Phase 5B.2/5B.3: 2×2 quadrant renderer, wired into App/CLI (Spiral only; Phase 6A adds foreground color).
pub(crate) mod quadrant;
mod shade;
mod starfield;
mod stretch;
// Phase 4: terminal-cell sampling topology (1×2 half-block, 2×2 quadrant).
pub(crate) mod topology;

pub use ascii::render_ascii;
pub(crate) use ascii::render_ascii_with_twinkle;
pub use color::ColorPalette;
pub use profile::{prepare_density, prepare_density_with_shape, PreparedDensity, RenderProfile};
pub(crate) use quadrant::{render_quadrant_with_stars, render_quadrant_with_stars_at_frame};
pub use shade::render_shades;
pub(crate) use shade::render_shades_with_twinkle;
pub use starfield::render_starfield;
pub(crate) use starfield::render_starfield_with_twinkle;

use crate::engine::ArtModel;
use crate::render::ansi::AnsiHalfBlockLine;
use crate::render::topology::CellSamplingShape;
use crate::seed::{derive_feature_seed, ANIMATION_STAR_TWINKLE_V1};
use color::{galaxy_background_ansi, galaxy_foreground_ansi};
use hash::{hash_cell, hash_to_unit};

/// Returns the appropriate glyph for half-block rendering based on visibility.
///
/// Never returns a glyph with ANSI styling. This function is for no-color mode
/// and for determining which character to use before applying color in color mode.
pub(super) fn glyph_for_half_block(top_visible: bool, bottom_visible: bool) -> char {
    match (top_visible, bottom_visible) {
        (true, true) => '█',
        (true, false) => '▀',
        (false, true) => '▄',
        (false, false) => ' ',
    }
}

/// Renderer effectively used after model and renderer choice resolution.
///
/// Every variant is only produced for a model that already supports it:
/// `App::resolve_effective_renderer` rejects unsupported combinations
/// before an `EffectiveRenderer` value exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveRenderer {
    /// Starfield dedicated renderer.
    Starfield,
    /// Half-block renderer (half-block characters ▄▀█).
    HalfBlock,
    /// Shade renderer (shade characters ░▒▓█).
    Shade,
    /// ASCII renderer (ASCII characters .:-=+*#%@).
    Ascii,
    /// Experimental 2×2 quadrant renderer (Spiral only).
    Quadrant,
}

/// Deterministic presentation context for one star-twinkle frame.
///
/// The context is derived from one resolved scene and is consumed only by
/// renderers at presentation time. It never changes the prepared density or
/// any generation RNG stream. `frame_count` includes both static endpoint
/// frames, so frame `0` and frame `frame_count - 1` are always the exact base
/// render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StarTwinkleFrame {
    /// Concrete seed captured from the resolved scene.
    pub(crate) scene_seed: u64,
    /// Zero-based frame index in the fixed intro sequence.
    pub(crate) frame_index: u32,
    /// Total number of frames in the fixed intro sequence.
    pub(crate) frame_count: u32,
}

const TWINKLE_CYCLE_STEPS: u64 = 8;

/// Applies subtle, deterministic tier modulation to an existing star glyph.
///
/// The existing star decision remains authoritative: `None` and a space stay
/// empty, and only the three allowed tiers (`.`, `*`, `+`) are modulated. A
/// stable phase derived from the resolved scene seed, the versioned twinkle
/// namespace, and terminal-cell coordinates drives a short temporal cycle.
/// Modulation is clamped at the faint/bright endpoints and is disabled for
/// frame `0` and the final frame.
pub(crate) fn twinkle_star_glyph(
    base: Option<char>,
    scene_seed: u64,
    x: usize,
    y: usize,
    frame_index: u32,
    frame_count: u32,
) -> Option<char> {
    let base = base?;

    if base == ' ' {
        return Some(base);
    }

    let (tier, tiers): (i8, [char; 3]) = match base {
        '.' => (0, ['.', '*', '+']),
        '*' => (1, ['.', '*', '+']),
        '+' => (2, ['.', '*', '+']),
        _ => return Some(base),
    };

    if frame_count <= 1 || frame_index == 0 || frame_index >= frame_count.saturating_sub(1) {
        return Some(base);
    }

    let twinkle_seed = derive_feature_seed(scene_seed, ANIMATION_STAR_TWINKLE_V1);
    let phase = hash_cell(x, y, twinkle_seed) % TWINKLE_CYCLE_STEPS;
    let cycle_step = (phase + u64::from(frame_index)) % TWINKLE_CYCLE_STEPS;
    let delta = match cycle_step {
        2 | 3 => 1,
        6 | 7 => -1,
        _ => 0,
    };

    let modulated_tier = (tier + delta).clamp(0, 2) as usize;
    Some(tiers[modulated_tier])
}

/// Applies a frame context to a base star when animation is enabled.
fn maybe_twinkle_star(
    base: Option<char>,
    x: usize,
    y: usize,
    frame: Option<StarTwinkleFrame>,
) -> Option<char> {
    match frame {
        Some(frame) => twinkle_star_glyph(
            base,
            frame.scene_seed,
            x,
            y,
            frame.frame_index,
            frame.frame_count,
        ),
        None => base,
    }
}

/// Maps a resolved model and effective renderer to the terminal-cell
/// sampling shape used for density generation and preparation.
///
/// Semantics:
/// - `Spiral + Quadrant` -> `CellSamplingShape::QUADRANT` (logical 2W×2H);
/// - every other valid model/renderer combination -> `CellSamplingShape::HALF_BLOCK`.
///
/// `ArtModel::Random` must never reach this function: the App resolves the
/// model before renderer selection, and `App::resolve_effective_renderer`
/// rejects invalid renderer/model combinations earlier.
pub fn sampling_shape_for(model: ArtModel, renderer: EffectiveRenderer) -> CellSamplingShape {
    match (model, renderer) {
        (ArtModel::Spiral, EffectiveRenderer::Quadrant) => CellSamplingShape::QUADRANT,
        (ArtModel::Random, _) => {
            panic!("Random model should be resolved before sampling shape selection")
        }
        _ => CellSamplingShape::HALF_BLOCK,
    }
}

/// Renderiza o mapa de densidade usando caracteres de bloco Unicode meio a meio.
///
/// This is the true half-block renderer for galaxy-like models. It consumes pairs of density
/// rows and converts them to one terminal row using half-block characters.
///
/// The renderer:
/// - Takes two vertical density samples per terminal row
/// - Calculates independent visibility for top and bottom halves
/// - Uses ▀ for top-only, ▄ for bottom-only, █ for both, space for neither
/// - In color mode: uses foreground for top, background for bottom with ▀
///
/// Each terminal cell represents two vertical density samples:
/// ```text
/// top density row    -> upper half of the glyph (top visible)
/// bottom density row -> lower half of the glyph (bottom visible)
/// ```
pub fn render_half_blocks(
    canvas: &[Vec<f64>],
    threshold: f64,
    colors_enabled: bool,
    palette: ColorPalette,
) -> Vec<String> {
    render_half_blocks_with_twinkle(canvas, threshold, colors_enabled, palette, None)
}

/// Renders half-block art with an optional deterministic star-twinkle frame.
pub(crate) fn render_half_blocks_with_twinkle(
    canvas: &[Vec<f64>],
    threshold: f64,
    colors_enabled: bool,
    palette: ColorPalette,
    frame: Option<StarTwinkleFrame>,
) -> Vec<String> {
    let star_seed = star_field_seed(canvas);

    let width = canvas.first().map_or(0, Vec::len);
    let mut lines = Vec::with_capacity(canvas.len().div_ceil(2));

    for y in (0..canvas.len()).step_by(2) {
        let mut line = AnsiHalfBlockLine::with_capacity(width * 8); // Estimate: 8 bytes per cell

        for x in 0..width {
            let top = canvas[y].get(x).copied().unwrap_or(0.0);
            let bottom = canvas
                .get(y + 1)
                .and_then(|row| row.get(x))
                .copied()
                .unwrap_or(0.0);

            // Calculate independent visibility for each half
            let top_visible = top.is_finite() && top > 0.0 && top >= threshold;
            let bottom_visible = bottom.is_finite() && bottom > 0.0 && bottom >= threshold;

            // For no-color mode, check for background stars first
            if !colors_enabled {
                let galaxy_ch = glyph_for_half_block(top_visible, bottom_visible);
                if galaxy_ch == ' ' {
                    // Only inject background star if neither half is visible
                    if let Some(star_ch) = maybe_twinkle_star(
                        star_glyph_for_cell(x, y / 2, top, bottom, threshold, star_seed),
                        x,
                        y / 2,
                        frame,
                    ) {
                        // Keep stars uncolored for portability
                        line.push_cell(star_ch, None, None);
                        continue;
                    }
                }
                // No-color mode: plain glyphs
                line.push_cell(galaxy_ch, None, None);
                continue;
            }

            // Color mode: apply foreground and background colors
            // Note: colored both-visible uses '▀' (top half block) with:
            // - foreground color for the top half
            // - background color for the lower half
            // This is different from no-color mode which uses '█'
            if top_visible && bottom_visible {
                // Both visible: foreground for top, background for bottom, ▀ glyph
                let fg = galaxy_foreground_ansi(palette, top);
                let bg = galaxy_background_ansi(palette, bottom);
                line.push_cell('▀', Some(fg), Some(bg));
            } else if top_visible {
                // Top only: foreground for top, ▀ glyph
                let fg = galaxy_foreground_ansi(palette, top);
                line.push_cell('▀', Some(fg), None);
            } else if bottom_visible {
                // Bottom only: foreground for bottom, ▄ glyph
                let fg = galaxy_foreground_ansi(palette, bottom);
                line.push_cell('▄', Some(fg), None);
            } else {
                // Neither visible: plain space or background star
                // Always emit a cell to preserve terminal width
                let ch = maybe_twinkle_star(
                    star_glyph_for_cell(x, y / 2, top, bottom, threshold, star_seed),
                    x,
                    y / 2,
                    frame,
                )
                .unwrap_or(' ');
                line.push_cell(ch, None, None);
            }
        }

        lines.push(line.finish());
    }

    lines
}

/// Returns the visibility scale for a density value above threshold.
///
/// Returns None for:
/// - Non-finite values (NaN, infinity, negative infinity)
/// - Zero or negative values
/// - Values below threshold
///
/// Returns Some(scaled) for visible values, where scaled is a monotonic
/// mapping from [threshold, 1.0] to [0.0, 1.0].
pub(super) fn scale_visible(value: f64, threshold: f64) -> Option<f64> {
    // Reject non-finite values
    if !value.is_finite() {
        return None;
    }

    // Reject zero and negative
    if value <= 0.0 {
        return None;
    }

    // Reject below threshold (value exactly equal to threshold is visible)
    if value < threshold {
        return None;
    }

    // Map to [0.0, 1.0] where threshold -> 0.0 and 1.0 -> 1.0
    if threshold >= 1.0 {
        Some(1.0)
    } else {
        let scaled = ((value - threshold) / (1.0 - threshold)).clamp(0.0, 1.0);
        Some(scaled)
    }
}

pub(super) fn star_glyph_for_cell(
    x: usize,
    y: usize,
    top: f64,
    bottom: f64,
    threshold: f64,
    seed: u64,
) -> Option<char> {
    // Historical two-half local-density calculation: the maximum of the two
    // vertical samples this renderer consumes for the terminal cell.
    let local_density = top.max(bottom);
    star_glyph_for_local_density(x, y, local_density, threshold, seed)
}

/// Returns the deterministic background-star glyph for a terminal cell given
/// an already-computed local density.
///
/// This is the single shared star decision used by every galaxy renderer
/// (HalfBlock, Shade, Ascii, and the star-aware Quadrant renderer). It is
/// topology-agnostic: the caller computes `local_density` as the maximum of
/// the subcell densities the renderer samples for that terminal cell (two
/// vertical halves for the 1×2 renderers, four subcells for the 2×2
/// quadrant renderer) and passes it here.
///
/// The decision is fully deterministic (no RNG):
/// - If `local_density > threshold * 0.35`, no star is emitted. This is the
///   local-density suppression gate: a cell close enough to visible galaxy
///   structure would lose a faint star, so it is left empty.
/// - Otherwise, a hash of `(x, y, seed)` selects the glyph tier:
///     - `r < 0.0004` => `+` (bright, very rare)
///     - `r < 0.0022` => `*` (medium, rare)
///     - `r < 0.0132` => `.` (faint, common)
///     - otherwise    => no star
///
/// The probability constants, glyph thresholds, and the `hash_cell` /
/// `hash_to_unit` primitives are unchanged from the historical behavior.
///
/// # Arguments
/// - `x`, `y`: terminal-cell coordinates.
/// - `local_density`: maximum sampled density for the cell (raw value, same
///   units as the canvas densities).
/// - `threshold`: the galaxy visibility threshold (same units as the canvas).
/// - `seed`: the star-field seed for the canvas (`star_field_seed`).
///
/// # Returns
/// `Some(glyph)` for a star (`+`, `*`, or `.`), or `None` for no star.
pub(super) fn star_glyph_for_local_density(
    x: usize,
    y: usize,
    local_density: f64,
    threshold: f64,
    seed: u64,
) -> Option<char> {
    // Do not draw background stars over visible galaxy structure.
    // A star should never replace either visible galaxy half.
    if local_density > threshold * 0.35 {
        return None;
    }

    let r = hash_to_unit(hash_cell(x, y, seed));

    // Sparse background:
    // - "." = faint common stars
    // - "*" = medium rare stars
    // - "+" = bright very rare stars
    if r < 0.0004 {
        Some('+')
    } else if r < 0.0022 {
        Some('*')
    } else if r < 0.0132 {
        Some('.')
    } else {
        None
    }
}

pub(super) fn star_field_seed(canvas: &[Vec<f64>]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;

    for (i, value) in canvas.iter().flatten().enumerate() {
        // Sample all values but quantize them. This keeps the star field
        // deterministic and makes it vary with the galaxy seed without passing
        // the CLI seed into the renderer API.
        let quantized = (value.clamp(0.0, 1.0) * 4096.0).round() as u64;
        hash ^= quantized.wrapping_add((i as u64).wrapping_mul(0x9e3779b97f4a7c15));
        hash = hash.wrapping_mul(0x100000001b3);
    }

    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    use color::RESET;

    // Constant for default palette (Nebula) in tests
    const DEFAULT_PALETTE: ColorPalette = ColorPalette::Nebula;

    // ===== sampling_shape_for tests =====

    #[test]
    fn test_sampling_shape_spiral_quadrant_is_quadrant() {
        assert_eq!(
            sampling_shape_for(ArtModel::Spiral, EffectiveRenderer::Quadrant),
            CellSamplingShape::QUADRANT
        );
    }

    #[test]
    fn test_sampling_shape_all_other_valid_pairs_are_half_block() {
        let models = [
            ArtModel::Spiral,
            ArtModel::Elliptical,
            ArtModel::Cluster,
            ArtModel::Starfield,
        ];
        let renderers = [
            EffectiveRenderer::HalfBlock,
            EffectiveRenderer::Shade,
            EffectiveRenderer::Ascii,
            EffectiveRenderer::Starfield,
        ];
        for model in models {
            for renderer in renderers {
                assert_eq!(
                    sampling_shape_for(model, renderer),
                    CellSamplingShape::HALF_BLOCK,
                    "{model:?} + {renderer:?} must keep HALF_BLOCK"
                );
            }
        }
        // Spiral with the non-quadrant renderers also stays HALF_BLOCK.
        assert_eq!(
            sampling_shape_for(ArtModel::Spiral, EffectiveRenderer::HalfBlock),
            CellSamplingShape::HALF_BLOCK
        );
    }

    #[test]
    #[should_panic(expected = "Random model should be resolved before sampling shape selection")]
    fn test_sampling_shape_random_is_rejected() {
        let _ = sampling_shape_for(ArtModel::Random, EffectiveRenderer::HalfBlock);
    }

    #[test]
    fn test_deterministic_render() {
        let model = crate::engine::ArtModel::Starfield;
        let canvas1 = model.generate_scene(10, 5, Some(42)).density.into_rows();
        let canvas2 = model.generate_scene(10, 5, Some(42)).density.into_rows();

        assert_eq!(canvas1, canvas2);
    }

    #[test]
    fn test_render_starfield_collapses_density_rows() {
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let canvas = vec![
            vec![0.0, 0.04, 0.10, 0.20],
            vec![0.0, 0.0, 0.0, 0.0],
            vec![0.02, 0.08, 0.15, 0.18],
            vec![0.0, 0.0, 0.0, 0.0],
        ];

        let result = render_starfield(&canvas, false, &terminal, ColorPalette::Nebula);

        assert_eq!(result, vec![" .*+", " .++"]);
    }

    #[test]
    fn test_render_starfield_no_color_is_ansi_free() {
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let canvas = vec![vec![0.20]];

        let result = render_starfield(&canvas, false, &terminal, ColorPalette::Nebula);

        assert_eq!(result, vec!["+"]);
        assert!(!result[0].contains('\x1b'));
    }

    #[test]
    fn test_render_starfield_colored_contains_ansi() {
        let terminal = crate::terminal::Terminal::with_colors(true, true);
        let canvas = vec![vec![0.20]];

        let result = render_starfield(&canvas, true, &terminal, ColorPalette::Nebula);

        assert!(result[0].contains('\x1b'));
        assert!(result[0].contains('+'));
    }

    #[test]
    fn test_render_half_blocks_with_precomputed_threshold() {
        let canvas = vec![vec![1.0, 0.0, 1.0], vec![0.0, 1.0, 1.0]];

        // Use a low threshold so all cells are visible
        let result = render_half_blocks(&canvas, 0.0, false, DEFAULT_PALETTE);

        // Cell 0: top=1.0 (visible), bottom=0.0 (not visible) → '▀'
        // Cell 1: top=0.0 (not visible), bottom=1.0 (visible) → '▄'
        // Cell 2: top=1.0 (visible), bottom=1.0 (visible) → '█'
        assert_eq!(result, vec!["▀▄█"]);
    }

    #[test]
    fn test_render_half_blocks_zero_map_is_empty() {
        // Create a canvas with at least two density rows, all zeros
        let canvas = vec![vec![0.0, 0.0, 0.0, 0.0], vec![0.0, 0.0, 0.0, 0.0]];

        // With any threshold, zero-density cells should render as spaces
        let result = render_half_blocks(&canvas, 0.0, false, DEFAULT_PALETTE);

        // Each terminal row should contain only spaces (no galaxy glyphs)
        assert_eq!(result.len(), 1);
        assert!(result[0].chars().all(|c| c == ' '));
    }

    // ===== No-color half-block tests =====

    #[test]
    fn test_half_blocks_top_only() {
        // Top visible, bottom invisible → '▀'
        let canvas = vec![vec![0.5], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        assert_eq!(result, vec!["▀"]);
    }

    #[test]
    fn test_half_blocks_bottom_only() {
        // Top invisible, bottom visible → '▄'
        let canvas = vec![vec![0.0], vec![0.5]];
        let result = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        assert_eq!(result, vec!["▄"]);
    }

    #[test]
    fn test_half_blocks_both_visible() {
        // Both visible → '█'
        let canvas = vec![vec![0.5], vec![0.5]];
        let result = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        assert_eq!(result, vec!["█"]);
    }

    #[test]
    fn test_half_blocks_neither_visible() {
        // Neither visible → ' '
        let canvas = vec![vec![0.0], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_half_blocks_zero_values_are_invisible() {
        // Zero values should be invisible regardless of threshold
        let canvas = vec![vec![0.0], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.0, false, DEFAULT_PALETTE);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_half_blocks_negative_values_are_invisible() {
        // Negative values should be invisible
        let canvas = vec![vec![-0.5], vec![-0.5]];
        let result = render_half_blocks(&canvas, 0.0, false, DEFAULT_PALETTE);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_half_blocks_non_finite_values_are_invisible() {
        // Non-finite values should be invisible
        let canvas = vec![vec![f64::NAN], vec![f64::INFINITY]];
        let result = render_half_blocks(&canvas, 0.0, false, DEFAULT_PALETTE);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_half_blocks_odd_height() {
        // Test with odd number of rows (last row has no bottom)
        // 3 rows → ceil(3/2) = 2 terminal rows
        let canvas = vec![vec![0.5, 0.0], vec![0.0, 0.5], vec![0.5, 0.0]];
        let result = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        // Terminal row 0 (canvas rows 0-1):
        //   Cell 0: top=0.5 (visible), bottom=0.0 (not visible) → '▀'
        //   Cell 1: top=0.0 (not visible), bottom=0.5 (visible) → '▄'
        //   Result: "▀▄"
        // Terminal row 1 (canvas rows 2-3):
        //   Cell 0: top=0.5 (visible), bottom=0.0 (missing, invisible) → '▀'
        //   Cell 1: top=0.0 (not visible), bottom=0.0 (missing, invisible) → ' '
        //   Result: "▀ "
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "▀▄");
        assert_eq!(result[1], "▀ ");
    }

    #[test]
    fn test_half_blocks_preserve_terminal_width() {
        // Width should be preserved
        let canvas = vec![vec![0.5, 0.0, 0.5, 0.0], vec![0.0, 0.5, 0.0, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        // 2 rows → 1 terminal row with 4 characters
        // Note: Unicode half-block characters are 3 bytes each in UTF-8
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].chars().count(), 4);
        // Check the actual content
        assert_eq!(result[0], "▀▄▀▄");
    }

    #[test]
    fn test_half_blocks_no_color_is_ansi_free() {
        // No-color mode should not contain ANSI sequences
        let canvas = vec![vec![0.5], vec![0.5]];
        let result = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        assert!(!result[0].contains('\x1b'));
        assert_eq!(result, vec!["█"]);
    }

    #[test]
    fn test_half_blocks_deterministic() {
        // Same input should produce same output
        let canvas = vec![vec![0.5, 0.0], vec![0.0, 0.5]];
        let result1 = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        let result2 = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        assert_eq!(result1, result2);
    }

    // ===== Colored half-block tests =====

    #[test]
    fn test_half_blocks_colored_top_only() {
        // Top only: foreground sequence + ▀ + reset
        let canvas = vec![vec![0.5], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);

        let line = &result[0];
        // Top intensity 0.5 maps to index 65 (muted green) with sequence \x1b[38;5;65m
        // Expected: \x1b[38;5;65m▀\x1b[0m
        assert_eq!(line, "\x1b[38;5;65m▀\x1b[0m");
    }

    #[test]
    fn test_half_blocks_colored_bottom_only() {
        // Bottom only: foreground sequence + ▄ + reset
        let canvas = vec![vec![0.0], vec![0.5]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);

        let line = &result[0];
        // Bottom intensity 0.5 maps to index 65 (muted green) with sequence \x1b[38;5;65m
        // Expected: \x1b[38;5;65m▄\x1b[0m
        assert_eq!(line, "\x1b[38;5;65m▄\x1b[0m");
    }

    #[test]
    fn test_half_blocks_colored_both_visible() {
        // Both visible: foreground sequence + background sequence + ▀ + reset
        // Use different intensities so palette indexes differ
        // top = 0.50 -> index 65 (muted green) -> \x1b[38;5;65m
        // bottom = 0.80 -> index 130 (muted orange/red) -> \x1b[48;5;130m
        // Colored both-visible uses '▀' (top half block), not '█' (full block)
        let canvas = vec![vec![0.50], vec![0.80]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);

        let line = &result[0];
        // Expected: \x1b[38;5;65m\x1b[48;5;130m▀\x1b[0m
        // 38;5;65 for foreground (top), 48;5;130 for background (bottom)
        // Colored both-visible uses '▀' glyph (top half block)
        assert_eq!(line, "\x1b[38;5;65m\x1b[48;5;130m▀\x1b[0m");
    }

    #[test]
    fn test_half_blocks_colored_ends_with_reset() {
        // Every visible cell should end with reset
        let canvas = vec![vec![0.5, 0.5], vec![0.5, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);

        for line in &result {
            assert!(
                line.ends_with(RESET),
                "Line should end with reset: {}",
                line
            );
        }
    }

    #[test]
    fn test_half_blocks_no_ansi_leakage() {
        // ANSI state should not leak into later cells
        // With the new grouping: cells 0 and 1 have the same fg (index 65) so they are grouped
        // Cell 2 has different complete style (fg+bg vs fg-only) so it gets RESET before its style
        // Cell 0: top visible -> \x1b[38;5;65m
        // Cell 1: bottom visible -> same fg, no RESET, continues with \x1b[38;5;65m
        // Cell 2: both visible -> different complete style, RESET, then \x1b[38;5;65m\x1b[48;5;65m
        // The key is that RESET occurs between different complete styles
        let canvas = vec![vec![0.5, 0.0, 0.5], vec![0.0, 0.5, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);

        let line = &result[0];
        // With grouping:
        // Cell 0: top visible (fg) -> \x1b[38;5;65m, glyph '▀'
        // Cell 1: bottom visible (fg) -> same fg, no reset, continues with '▄'
        // Cell 2: both visible (fg+bg) -> different complete style, RESET, new fg, new bg, glyph '▀'
        // Result: \x1b[38;5;65m▀▄\x1b[0m\x1b[38;5;65m\x1b[48;5;65m▀\x1b[0m
        // Note: cells 0 and 1 share the same fg, so they're grouped without RESET between them
        let expected = "\x1b[38;5;65m▀▄\x1b[0m\x1b[38;5;65m\x1b[48;5;65m▀\x1b[0m";
        assert_eq!(line, expected);
    }

    #[test]
    fn test_half_blocks_background_star_never_overwrites_visible() {
        // Background stars should never overwrite visible galaxy halves
        let canvas = vec![vec![0.5], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);

        // Top is visible, so no star should appear
        assert_eq!(result, vec!["▀"]);
    }

    /// Tests the deterministic background-star helper directly.
    ///
    /// This test finds a coordinate where a star is emitted and verifies:
    /// 1. Such a coordinate exists
    /// 2. Repeated calls return the exact same glyph
    /// 3. The glyph is one of the supported background-star glyphs
    #[test]
    fn test_star_glyph_for_cell_deterministic() {
        // Create a canvas that will produce a deterministic star field seed
        let canvas = vec![vec![0.0, 0.0], vec![0.0, 0.0]];

        // Get the seed used by render_half_blocks
        let star_seed = star_field_seed(&canvas);

        // Threshold high enough that no galaxy structure is visible
        let threshold = 0.5;

        // Search bounded range for a coordinate that produces a star
        let mut found_star = false;
        let mut star_glyph = ' ';
        let mut star_x = 0;
        let mut star_y = 0;

        for y in 0..50 {
            for x in 0..50 {
                let top = 0.0;
                let bottom = 0.0;
                if let Some(glyph) = star_glyph_for_cell(x, y, top, bottom, threshold, star_seed) {
                    found_star = true;
                    star_glyph = glyph;
                    star_x = x;
                    star_y = y;
                    break;
                }
            }
            if found_star {
                break;
            }
        }

        // Assert that a star-emitting coordinate exists
        assert!(
            found_star,
            "Expected to find a coordinate where star_glyph_for_cell returns a star"
        );

        // Assert that repeated calls return the exact same glyph
        for _ in 0..10 {
            let glyph = star_glyph_for_cell(star_x, star_y, 0.0, 0.0, threshold, star_seed);
            assert_eq!(
                glyph,
                Some(star_glyph),
                "Star glyph should be deterministic"
            );
        }

        // Assert the glyph is one of the supported background-star glyphs
        assert!(
            star_glyph == '.' || star_glyph == '*' || star_glyph == '+',
            "Star glyph should be one of '.', '*', or '+', got '{}'",
            star_glyph
        );
    }

    #[test]
    fn test_twinkle_helper_is_deterministic_and_preserves_endpoints() {
        let base = Some('*');
        let first = twinkle_star_glyph(base, 42, 7, 3, 2, 6);
        let second = twinkle_star_glyph(base, 42, 7, 3, 2, 6);

        assert_eq!(first, second);
        assert_eq!(twinkle_star_glyph(base, 42, 7, 3, 0, 6), base);
        assert_eq!(twinkle_star_glyph(base, 42, 7, 3, 5, 6), base);
    }

    #[test]
    fn test_twinkle_helper_never_invents_or_hides_stars() {
        for frame_index in 0..6 {
            assert_eq!(twinkle_star_glyph(None, 42, 0, 0, frame_index, 6), None);
            assert_eq!(
                twinkle_star_glyph(Some(' '), 42, 0, 0, frame_index, 6),
                Some(' ')
            );

            for base in ['.', '*', '+'] {
                let glyph = twinkle_star_glyph(Some(base), 42, 0, 0, frame_index, 6)
                    .expect("existing star must remain present");
                assert!(
                    matches!(glyph, '.' | '*' | '+'),
                    "unexpected twinkle glyph {glyph:?}"
                );
            }
        }
    }

    #[test]
    fn test_twinkle_helper_uses_asynchronous_per_star_phases() {
        let outputs: Vec<Option<char>> = (0..64)
            .map(|x| twinkle_star_glyph(Some('*'), 42, x, 0, 1, 6))
            .collect();

        assert!(
            outputs.windows(2).any(|pair| pair[0] != pair[1]),
            "different star coordinates must not all blink in lockstep"
        );
    }

    #[test]
    fn test_twinkle_helper_changes_a_controlled_intermediate_tier() {
        let changed = (0..64).any(|x| twinkle_star_glyph(Some('.'), 42, x, 0, 1, 6) != Some('.'));

        assert!(changed, "the deterministic fixture must visibly twinkle");
    }

    #[test]
    fn test_twinkle_render_keeps_starfield_positions_fixed() {
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let canvas = vec![
            vec![0.05, 0.10, 0.20, 0.0, 0.05, 0.10, 0.20, 0.0],
            vec![0.0; 8],
        ];
        let static_frame = render_starfield(&canvas, false, &terminal, DEFAULT_PALETTE);
        let intermediate = render_starfield_with_twinkle(
            &canvas,
            false,
            &terminal,
            DEFAULT_PALETTE,
            Some(StarTwinkleFrame {
                scene_seed: 42,
                frame_index: 1,
                frame_count: 6,
            }),
        );

        let static_chars: Vec<char> = static_frame[0].chars().collect();
        let intermediate_chars: Vec<char> = intermediate[0].chars().collect();
        assert_eq!(static_chars.len(), intermediate_chars.len());
        for (base, animated) in static_chars.iter().zip(intermediate_chars.iter()) {
            assert_eq!(base.is_whitespace(), animated.is_whitespace());
            if base.is_whitespace() {
                assert_eq!(*animated, ' ');
            } else {
                assert!(matches!(animated, '.' | '*' | '+'));
            }
        }
        assert!(
            static_chars
                .iter()
                .zip(intermediate_chars.iter())
                .any(|(base, animated)| base != animated),
            "the controlled starfield fixture must change an intermediate tier"
        );
    }

    #[test]
    fn test_twinkle_render_preserves_galaxy_background_star_existence() {
        let frame = Some(StarTwinkleFrame {
            scene_seed: 42,
            frame_index: 1,
            frame_count: 6,
        });
        let fixture_canvas = |logical_columns_per_cell: usize| {
            (2..=800)
                .map(|terminal_width| terminal_width * logical_columns_per_cell)
                .find_map(|logical_width| {
                    let canvas = vec![vec![0.0; logical_width], vec![0.0; logical_width]];
                    let star_seed = star_field_seed(&canvas);
                    let terminal_width = logical_width / logical_columns_per_cell;
                    let has_changed_star = (0..terminal_width).any(|x| {
                        let base = star_glyph_for_local_density(x, 0, 0.0, 0.5, star_seed);
                        base.is_some()
                            && twinkle_star_glyph(Some(base.unwrap()), 42, x, 0, 1, 6) != base
                    });
                    has_changed_star.then_some(canvas)
                })
                .expect("bounded empty-galaxy fixture must contain a twinkling star")
        };
        let half_canvas = fixture_canvas(1);
        let quadrant_canvas = fixture_canvas(2);

        let cases = [
            (
                render_half_blocks(&half_canvas, 0.5, false, DEFAULT_PALETTE),
                render_half_blocks_with_twinkle(&half_canvas, 0.5, false, DEFAULT_PALETTE, frame),
            ),
            (
                crate::render::render_shades(&half_canvas, 0.5, false, DEFAULT_PALETTE),
                crate::render::render_shades_with_twinkle(
                    &half_canvas,
                    0.5,
                    false,
                    DEFAULT_PALETTE,
                    frame,
                ),
            ),
            (
                crate::render::render_ascii(&half_canvas, 0.5, false, DEFAULT_PALETTE),
                crate::render::render_ascii_with_twinkle(
                    &half_canvas,
                    0.5,
                    false,
                    DEFAULT_PALETTE,
                    frame,
                ),
            ),
            (
                render_quadrant_with_stars(&quadrant_canvas, 0.5, false, DEFAULT_PALETTE),
                render_quadrant_with_stars_at_frame(
                    &quadrant_canvas,
                    0.5,
                    false,
                    DEFAULT_PALETTE,
                    frame,
                ),
            ),
        ];

        for (case_index, (static_frame, animated_frame)) in cases.into_iter().enumerate() {
            assert_eq!(static_frame.len(), animated_frame.len());
            let mut saw_star = false;
            let mut saw_change = false;
            for (static_line, animated_line) in static_frame.iter().zip(animated_frame.iter()) {
                for (base, animated) in static_line.chars().zip(animated_line.chars()) {
                    if matches!(base, '.' | '*' | '+') {
                        saw_star = true;
                        assert!(matches!(animated, '.' | '*' | '+'));
                    } else {
                        assert_eq!(base, animated);
                    }
                    saw_change |= base != animated;
                }
            }
            assert!(
                saw_star,
                "synthetic empty galaxy case {case_index} must contain a star"
            );
            assert!(
                saw_change,
                "an existing background star in case {case_index} must twinkle"
            );
        }
    }

    #[test]
    fn test_half_blocks_deterministic_star_repeated_calls() {
        // Repeated calls with same canvas should produce same star result
        let canvas = vec![vec![0.0], vec![0.0]];
        let result1 = render_half_blocks(&canvas, 0.5, false, DEFAULT_PALETTE);
        let result2 = render_half_blocks(&canvas, 0.5, false, DEFAULT_PALETTE);
        assert_eq!(result1, result2, "Star field should be deterministic");
    }

    #[test]
    fn test_half_blocks_star_prevented_by_visible_galaxy() {
        // A visible galaxy half should prevent star replacement
        // Top visible (0.5 > 0.1 threshold), so no star should appear
        let canvas = vec![vec![0.5], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        assert_eq!(result, vec!["▀"]);
    }

    #[test]
    fn test_half_blocks_starfield_unchanged() {
        // Starfield rendering should remain unchanged
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let canvas = vec![
            vec![0.0, 0.04, 0.10, 0.20],
            vec![0.0, 0.0, 0.0, 0.0],
            vec![0.02, 0.08, 0.15, 0.18],
            vec![0.0, 0.0, 0.0, 0.0],
        ];

        let result = render_starfield(&canvas, false, &terminal, ColorPalette::Nebula);

        assert_eq!(result, vec![" .*+", " .++"]);
    }

    // ===== New tests for ANSI grouping with AnsiHalfBlockLine =====

    #[test]
    fn test_half_blocks_grouped_foreground_only() {
        let canvas = vec![vec![0.5, 0.5], vec![0.0, 0.0]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);
        let line = &result[0];
        assert_eq!(line, "\x1b[38;5;65m▀▀\x1b[0m");
    }

    #[test]
    fn test_half_blocks_grouped_foreground_plus_background() {
        let canvas = vec![vec![0.5, 0.5], vec![0.5, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);
        let line = &result[0];
        assert_eq!(line, "\x1b[38;5;65m\x1b[48;5;65m▀▀\x1b[0m");
    }

    #[test]
    fn test_half_blocks_top_only_to_both_visible() {
        let canvas = vec![vec![0.5, 0.5], vec![0.0, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);
        let line = &result[0];
        // fg-only -> fg+bg: RESET between, both use '▀'
        assert_eq!(
            line,
            "\x1b[38;5;65m▀\x1b[0m\x1b[38;5;65m\x1b[48;5;65m▀\x1b[0m"
        );
    }

    #[test]
    fn test_half_blocks_both_visible_to_top_only() {
        let canvas = vec![vec![0.5, 0.5], vec![0.5, 0.0]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);
        let line = &result[0];
        // fg+bg -> fg-only: RESET removes background, both use '▀'
        assert_eq!(
            line,
            "\x1b[38;5;65m\x1b[48;5;65m▀\x1b[0m\x1b[38;5;65m▀\x1b[0m"
        );
    }

    #[test]
    fn test_half_blocks_styled_to_star_resets() {
        let canvas = vec![vec![0.5, 0.0], vec![0.0, 0.0]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);
        let line = &result[0];
        // After RESET: exactly one plain star or space
        let last_reset = line.rfind("\x1b[0m").unwrap();
        let after = &line[last_reset + 4..];
        assert!(after.len() == 1 && " .+*".contains(after));
    }

    #[test]
    fn test_half_blocks_dim_to_non_dim_resets() {
        let canvas = vec![vec![0.1, 0.3], vec![0.0, 0.0]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);
        let line = &result[0];
        assert_eq!(line, "\x1b[2;38;5;17m▀\x1b[0m\x1b[38;5;30m▀\x1b[0m");
    }

    #[test]
    fn test_half_blocks_colored_width_preserved_for_invisible_cells() {
        let canvas = vec![vec![0.0, 0.0, 0.0], vec![0.0, 0.0, 0.0]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);
        let line = &result[0];
        // Invisible cells produce plain spaces without ANSI
        assert_eq!(line, "   ");
    }

    #[test]
    fn test_half_blocks_no_color_no_ansi_sequences() {
        let canvas = vec![vec![0.5, 0.5], vec![0.5, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        let line = &result[0];
        assert!(!line.contains('\x1b'));
        assert_eq!(line, "██");
    }

    #[test]
    fn test_half_blocks_colored_both_visible_uses_wiggle_glyph() {
        let canvas = vec![vec![0.5, 0.5], vec![0.5, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);
        let line = &result[0];
        assert_eq!(line, "\x1b[38;5;65m\x1b[48;5;65m▀▀\x1b[0m");
    }

    #[test]
    fn test_half_blocks_no_color_both_visible_uses_full_block() {
        let canvas = vec![vec![0.5, 0.5], vec![0.5, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, false, DEFAULT_PALETTE);
        let line = &result[0];
        assert_eq!(line, "██");
    }

    #[test]
    fn test_half_blocks_colored_glyphs_preserved() {
        let canvas = vec![vec![0.5, 0.0, 0.5], vec![0.0, 0.5, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, true, DEFAULT_PALETTE);
        let line = &result[0];
        // top-only '▀', bottom-only '▄', both '▀'
        assert!(line.contains('▀') && line.contains('▄'));
        assert_eq!(line.matches('▀').count(), 2);
        assert_eq!(line.matches('▄').count(), 1);
    }
}
