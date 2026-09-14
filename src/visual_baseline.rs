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
use crate::render::topology::CellSamplingShape;
use crate::render::{
    prepare_density, prepare_density_with_shape, render_ascii, render_half_blocks,
    render_quadrant_with_stars, render_shades, ColorPalette, EffectiveRenderer, PreparedDensity,
    RenderProfile,
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
        EffectiveRenderer::Quadrant => {
            panic!("Spiral baseline does not use the Quadrant renderer in this checkpoint")
        }
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

/// Phase 6D: no-color signature of the Spiral Quadrant renderer at
/// 40×20, mirroring the production split engine path without going through
/// `App` or `Terminal`:
///
/// `resolve_scene(seed)` -> `generate_density(..., QUADRANT)` ->
/// `prepare_density_with_shape(..., QUADRANT)` ->
/// `render_quadrant_with_stars(..., colors_enabled = false, Nebula)`.
///
/// The fingerprint covers the QUADRANT sampling output, normalization and
/// stretch, the quadrant occupancy threshold (target 0.26), the 4-bit glyph
/// geometry, and the deterministic background stars. It intentionally does
/// not cover ANSI color bytes, so colored Quadrant remains protected by the
/// focused unit tests and the colored/no-color geometry-identity invariant.
fn spiral_quadrant_no_color_render_signature(seed: u64) -> u64 {
    let resolved = ArtModel::Spiral.resolve_scene(Some(seed));
    let density = ArtModel::Spiral.generate_density(
        &resolved,
        BASELINE_WIDTH,
        BASELINE_HEIGHT,
        CellSamplingShape::QUADRANT,
    );
    let profile =
        RenderProfile::for_model_and_renderer(ArtModel::Spiral, EffectiveRenderer::Quadrant);
    let prepared = prepare_density_with_shape(density, profile, CellSamplingShape::QUADRANT);

    let PreparedDensity::Galaxy { density, threshold } = prepared else {
        panic!("Spiral + Quadrant must use galaxy density preparation");
    };

    let canvas = density.into_rows();
    let lines = render_quadrant_with_stars(&canvas, threshold, false, ColorPalette::Nebula);

    hash_terminal_lines(&lines)
}

/// Phase 0 visual-baseline anchors captured from main commit
/// `f036c2b230dc5a1faf6f9dcb2614b12d0e7726e8`.
///
/// Ordering for each seed: HalfBlock, Shade, ASCII.
const SEED_4_ANCHORS: [u64; 3] = [
    9021070325485438629_u64,
    277280708508277260_u64,
    3350187248859413498_u64,
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

/// Phase 6D permanent no-color Quadrant anchors, captured from the
/// production-equivalent split pipeline at the fixed 40×20 baseline size.
///
/// Seed 4: unbarred representative morphology.
const SEED_4_QUADRANT_ANCHOR: u64 = 963437774247816460_u64;
/// Seed 16: barred / long-arm morphology.
const SEED_16_QUADRANT_ANCHOR: u64 = 1819724489803438532_u64;
/// Seed 42: barred dense-core adversarial morphology.
const SEED_42_QUADRANT_ANCHOR: u64 = 10539789323035215409_u64;

#[test]
fn test_spiral_quadrant_seed_4_visual_anchor() {
    // Seed 4: unbarred representative morphology. Phase 6D acceptance: the
    // no-color Quadrant output at 40×20 is permanently anchored.
    assert_eq!(
        spiral_quadrant_no_color_render_signature(4),
        SEED_4_QUADRANT_ANCHOR
    );
}

#[test]
fn test_spiral_quadrant_seed_16_visual_anchor() {
    // Seed 16: barred / long-arm morphology. Phase 6D acceptance: the
    // no-color Quadrant output at 40×20 is permanently anchored.
    assert_eq!(
        spiral_quadrant_no_color_render_signature(16),
        SEED_16_QUADRANT_ANCHOR
    );
}

#[test]
fn test_spiral_quadrant_seed_42_visual_anchor() {
    // Seed 42: barred dense-core adversarial morphology. Phase 6D
    // acceptance: the no-color Quadrant output at 40×20 is permanently
    // anchored.
    assert_eq!(
        spiral_quadrant_no_color_render_signature(42),
        SEED_42_QUADRANT_ANCHOR
    );
}
