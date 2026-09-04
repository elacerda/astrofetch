//! Phase 0 visual baselines for renderer-preserving refactors.
//!
//! These tests intentionally hash the final no-color terminal lines rather than
//! the intermediate floating-point density map. The contract is visual: future
//! structural refactors must preserve the current Spiral output unless a model
//! change is explicitly intended.

use crate::engine::ArtModel;
use crate::render::{
    prepare_density, render_ascii, render_half_blocks, render_shades, ColorPalette,
    EffectiveRenderer, PreparedDensity, RenderProfile,
};

const BASELINE_WIDTH: usize = 40;
const BASELINE_HEIGHT: usize = 20;

fn spiral_no_color_render_signature(seed: u64, renderer: EffectiveRenderer) -> u64 {
    let scene = ArtModel::Spiral.generate_scene(BASELINE_WIDTH, BASELINE_HEIGHT, Some(seed));
    let profile = RenderProfile::for_model_and_renderer(ArtModel::Spiral, renderer);
    let prepared = prepare_density(scene.density, profile);

    let PreparedDensity::Galaxy { density, threshold } = prepared else {
        panic!("Spiral must use galaxy density preparation");
    };

    let rows = density.into_rows();
    let lines = match renderer {
        EffectiveRenderer::HalfBlock => {
            render_half_blocks(&rows, threshold, false, ColorPalette::Nebula)
        }
        EffectiveRenderer::Shade => render_shades(&rows, threshold, false, ColorPalette::Nebula),
        EffectiveRenderer::Ascii => render_ascii(&rows, threshold, false, ColorPalette::Nebula),
        EffectiveRenderer::Starfield => panic!("Spiral baseline does not use Starfield renderer"),
    };

    hash_terminal_lines(&lines)
}

fn hash_terminal_lines(lines: &[String]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;

    for line in lines {
        for byte in (line.len() as u64).to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for &byte in line.as_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }

    hash
}

#[test]
fn test_spiral_no_color_render_anchors() {
    // Baseline captured from the Phase 0 branch rooted at main commit
    // f036c2b230dc5a1faf6f9dcb2614b12d0e7726e8. Production generation and
    // rendering code are unchanged from that baseline in this phase.
    //
    // Ordering for each seed: HalfBlock, Shade, ASCII.
    let actual = [
        spiral_no_color_render_signature(4, EffectiveRenderer::HalfBlock),
        spiral_no_color_render_signature(4, EffectiveRenderer::Shade),
        spiral_no_color_render_signature(4, EffectiveRenderer::Ascii),
        spiral_no_color_render_signature(16, EffectiveRenderer::HalfBlock),
        spiral_no_color_render_signature(16, EffectiveRenderer::Shade),
        spiral_no_color_render_signature(16, EffectiveRenderer::Ascii),
        spiral_no_color_render_signature(42, EffectiveRenderer::HalfBlock),
        spiral_no_color_render_signature(42, EffectiveRenderer::Shade),
        spiral_no_color_render_signature(42, EffectiveRenderer::Ascii),
    ];

    let expected = [
        14236180911378073206_u64,
        2203561058913801456_u64,
        399856844935426647_u64,
        966378517523225027_u64,
        7075520085466075627_u64,
        4636907329852037665_u64,
        11074360617109061495_u64,
        11494504210253584690_u64,
        18184781296923229223_u64,
    ];

    assert_eq!(actual, expected);
}
