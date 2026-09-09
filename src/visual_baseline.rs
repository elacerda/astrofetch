//! Visual baselines for renderer-preserving refactors.
//!
//! These tests intentionally hash the final no-color terminal lines rather than
//! the intermediate floating-point density map. The contract is visual: future
//! structural refactors must preserve the current Spiral output unless a model
//! change is explicitly intended.
//!
//! Seed 4 is unbarred under `spiral/bar/v1` and retains its Phase 0 anchors.
//! Seeds 16 and 42 are barred and are anchored to the accepted Phase 1B barred
//! morphology.

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

/// Phase 0 visual-baseline anchors captured from main commit
/// `f036c2b230dc5a1faf6f9dcb2614b12d0e7726e8`.
///
/// Ordering for each seed: HalfBlock, Shade, ASCII.
const SEED_4_ANCHORS: [u64; 3] = [
    14236180911378073206_u64,
    2203561058913801456_u64,
    399856844935426647_u64,
];

/// Accepted Phase 1B visual-baseline anchors for the barred morphology.
///
/// Ordering for each seed: HalfBlock, Shade, ASCII.
const SEED_16_ANCHORS: [u64; 3] = [
    9130951262612329901_u64,
    9038831600118160687_u64,
    7384858223178912607_u64,
];

const SEED_42_ANCHORS: [u64; 3] = [
    8218115573612052101_u64,
    3710805785736049916_u64,
    838701103121160052_u64,
];

#[test]
fn test_spiral_unbarred_seed_4_visual_anchors_unchanged() {
    // Seed 4 is unbarred under `spiral/bar/v1`, so its visual output must
    // remain identical to the Phase 0 baseline.
    let actual = [
        spiral_no_color_render_signature(4, EffectiveRenderer::HalfBlock),
        spiral_no_color_render_signature(4, EffectiveRenderer::Shade),
        spiral_no_color_render_signature(4, EffectiveRenderer::Ascii),
    ];

    assert_eq!(actual, SEED_4_ANCHORS);
}

#[test]
fn test_spiral_barred_seed_16_visual_anchors() {
    // Seed 16 is barred under `spiral/bar/v1`. Its visual output is anchored
    // to the accepted Phase 1B barred morphology.
    let actual = [
        spiral_no_color_render_signature(16, EffectiveRenderer::HalfBlock),
        spiral_no_color_render_signature(16, EffectiveRenderer::Shade),
        spiral_no_color_render_signature(16, EffectiveRenderer::Ascii),
    ];

    assert_eq!(actual, SEED_16_ANCHORS);
}

#[test]
fn test_spiral_barred_seed_42_visual_anchors() {
    // Seed 42 is barred under `spiral/bar/v1`. Its visual output is anchored
    // to the accepted Phase 1B barred morphology.
    let actual = [
        spiral_no_color_render_signature(42, EffectiveRenderer::HalfBlock),
        spiral_no_color_render_signature(42, EffectiveRenderer::Shade),
        spiral_no_color_render_signature(42, EffectiveRenderer::Ascii),
    ];

    assert_eq!(actual, SEED_42_ANCHORS);
}
