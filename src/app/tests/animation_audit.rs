//! A4 audit: pure, unit-testable quantification of the fixed intro frame
//! contracts.
//!
//! This module is compiled only in test builds (wired into the `app` test
//! module). It renders the full deterministic frame sequence for
//! representative fixed-seed scenes at normal dimensions and measures:
//!
//! - the fraction of galaxy-background stars whose glyph tier changes on
//!   each intermediate frame;
//! - the fraction of dedicated-Starfield stars whose glyph tier changes on
//!   each intermediate frame;
//! - the fraction of dedicated-Starfield stars that actually change
//!   position on each intermediate frame (after edge/collision rejection);
//! - stars created or destroyed between frames (must stay zero);
//! - line count and visible-width stability;
//! - first/final frame byte identity against the static render.
//!
//! Galaxy background stars are identified with the same source-of-truth
//! decision the renderers use (`star_glyph_for_cell` /
//! `star_glyph_for_local_density`), because the Ascii galaxy glyph ladder
//! also contains `.`, `*`, and `+`. The module performs no terminal I/O
//! and never mutates production state.
//!
//! Measured behavior (fixed seeds, 40x20 / 60x16 art):
//! - galaxy scenes hold 0-10 background stars; per-frame tier-change
//!   fractions cluster around 0.3 where stars exist and hit 0.0 or 1.0 in
//!   1-2 star scenes (small-population noise, not a global pulse);
//! - the dedicated Starfield holds 24-30 stars; per-frame twinkle runs
//!   0.16-0.56 and per-frame motion 0.00-0.20 (nominally 1/8 of stars
//!   propose motion; edge/collision rejection keeps it a clear minority);
//! - a few stars can stay frozen for the whole 4-frame window when both
//!   of their twinkle steps land on the clamped side of their tier
//!   (up to 25% of the faintest/brightest stars) — inherent to sampling
//!   an 8-step cycle with 4 intermediate frames.

use super::*;
use crate::render::{star_field_seed, star_glyph_for_cell, star_glyph_for_local_density};

/// Fixed seeds used by every audit measurement.
const AUDIT_SEEDS: [u64; 4] = [7, 42, 1234, 99991];

/// Normal art dimensions (terminal cells) used by the audit.
const AUDIT_ART: (usize, usize) = (40, 20);

/// A wider normal dimension used for the dedicated Starfield audit.
const AUDIT_ART_WIDE: (usize, usize) = (60, 16);

/// Strips ANSI escape sequences (`ESC [ ... final byte`) from `line`.
fn strip_ansi(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for param in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&param) {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Returns true for the three background-star tier glyphs.
fn star_tier(ch: char) -> Option<u8> {
    match ch {
        '.' => Some(0),
        '*' => Some(1),
        '+' => Some(2),
        _ => None,
    }
}

/// Maps a CLI model to its concrete engine model.
fn engine_model(model: &ArtModel) -> EngineModel {
    match model {
        ArtModel::Random => EngineModel::Random,
        ArtModel::Spiral => EngineModel::Spiral,
        ArtModel::Elliptical => EngineModel::Elliptical,
        ArtModel::Cluster => EngineModel::Cluster,
        ArtModel::Starfield => EngineModel::Starfield,
    }
}

/// One rendered scene: the full frame sequence, the static render, and the
/// prepared density used to derive the ground-truth background-star set.
struct SceneRender {
    /// Every frame of the fixed intro sequence.
    frames: Vec<Vec<String>>,
    /// Legacy static render (no frame context).
    static_frame: Vec<String>,
    /// Prepared canvas rows (terminal sampling already applied).
    canvas: Vec<Vec<f64>>,
    /// Galaxy visibility threshold (None for the dedicated Starfield).
    threshold: Option<f64>,
}

fn render_scene(
    model: ArtModel,
    renderer: RendererChoice,
    seed: u64,
    art: (usize, usize),
    colors: bool,
) -> SceneRender {
    let engine = engine_model(&model);
    let app = build_test_app_pipeline(model, renderer, Some(seed), !colors, colors);
    let terminal = Terminal::with_colors(true, colors);
    let prepared = app
        .prepare_art(colors, engine, art.0, art.1)
        .expect("audit scene must prepare");
    let static_frame = app
        .render_art(&terminal, colors, engine, art.0, art.1)
        .expect("audit static render must succeed");
    let frames = app
        .render_animation_frames(&terminal, &prepared)
        .expect("audit frame sequence must render");
    let (canvas, threshold) = match &prepared.prepared_density {
        PreparedArtDensity::Galaxy { canvas, threshold } => (canvas.clone(), Some(*threshold)),
        PreparedArtDensity::Starfield { canvas } => (canvas.clone(), None),
    };
    SceneRender {
        frames,
        static_frame,
        canvas,
        threshold,
    }
}

/// Stripped (ANSI-free) grid of one frame, plus per-line visible widths.
struct FrameGrid {
    rows: Vec<Vec<char>>,
    widths: Vec<usize>,
}

fn frame_grid(frame: &[String]) -> FrameGrid {
    let rows = frame
        .iter()
        .map(|line| strip_ansi(line).chars().collect())
        .collect();
    let widths = frame.iter().map(|line| visible_width(line)).collect();
    FrameGrid { rows, widths }
}

/// Ground-truth background stars `(x, y, glyph)` of a galaxy scene,
/// derived from the prepared canvas with the same decision the renderers
/// use (two vertical samples per terminal cell).
fn galaxy_ground_truth(scene: &SceneRender, art_width: usize) -> Vec<(usize, usize, char)> {
    let threshold = scene.threshold.expect("galaxy scene must have a threshold");
    let star_seed = star_field_seed(&scene.canvas);
    let rows = scene.canvas.len().div_ceil(2);
    let mut stars = Vec::new();
    for y in 0..rows {
        for x in 0..art_width {
            if let Some(glyph) = star_glyph_for_cell(
                x,
                y,
                scene
                    .canvas
                    .get(2 * y)
                    .and_then(|row| row.get(x))
                    .copied()
                    .unwrap_or(0.0),
                scene
                    .canvas
                    .get(2 * y + 1)
                    .and_then(|row| row.get(x))
                    .copied()
                    .unwrap_or(0.0),
                threshold,
                star_seed,
            ) {
                stars.push((x, y, glyph));
            }
        }
    }
    stars
}

/// Ground-truth background stars of a 2x2-quadrant galaxy scene (four
/// subcells per terminal cell).
fn quadrant_ground_truth(scene: &SceneRender, art_width: usize) -> Vec<(usize, usize, char)> {
    let threshold = scene.threshold.expect("galaxy scene must have a threshold");
    let star_seed = star_field_seed(&scene.canvas);
    let rows = scene.canvas.len().div_ceil(2);
    let mut stars = Vec::new();
    for y in 0..rows {
        for x in 0..art_width {
            let mut local = 0.0f64;
            for (dy, dx) in [
                (0usize, 0usize),
                (0usize, 1usize),
                (1usize, 0usize),
                (1usize, 1usize),
            ] {
                local = local.max(
                    scene
                        .canvas
                        .get(2 * y + dy)
                        .and_then(|row| row.get(2 * x + dx))
                        .copied()
                        .unwrap_or(0.0),
                );
            }
            if let Some(glyph) = star_glyph_for_local_density(x, y, local, threshold, star_seed) {
                stars.push((x, y, glyph));
            }
        }
    }
    stars
}

/// Star cells read from the rendered frame output (dedicated Starfield,
/// whose glyph set is exactly the three tiers plus space).
fn starfield_output_stars(grid: &FrameGrid) -> Vec<(usize, usize, char)> {
    grid.rows
        .iter()
        .enumerate()
        .flat_map(|(y, row)| {
            row.iter()
                .enumerate()
                .filter_map(move |(x, ch)| star_tier(*ch).map(|_| (x, y, *ch)))
        })
        .collect()
}

/// Per-frame audit metrics.
struct FrameMetrics {
    /// Star count of the ground-truth set (frame 0).
    stars: usize,
    /// Per intermediate frame: fraction of ground-truth stars whose tier
    /// changed.
    tier_changed_fraction: Vec<f64>,
    /// Per intermediate frame: fraction of stars that moved to a different
    /// cell (dedicated Starfield; always zero for galaxy renderers).
    position_moved_fraction: Vec<f64>,
    /// Per intermediate frame: number of stars that moved.
    position_moved_count: Vec<usize>,
    /// Per intermediate frame: ground-truth stars whose cell is empty space
    /// (a pinned star may never vanish into space).
    missing: Vec<usize>,
    /// Per intermediate frame: ground-truth stars hidden behind shifted
    /// galaxy structure (allowed only for the A5 Spiral arm motion).
    occluded: Vec<usize>,
    /// Frame-0 visible structure cells (non-space, non-star-tier glyphs).
    visible_cells: usize,
    /// Per intermediate frame: output stars absent from the ground truth.
    extra: Vec<usize>,
    /// Per intermediate frame: non-star cells that differ from frame 0.
    morphology_delta: Vec<usize>,
    /// Stars created or destroyed across the whole sequence (max-min count).
    created_or_destroyed: usize,
    /// Line count and visible widths identical on every frame.
    geometry_stable: bool,
    /// Frame 0 and final frame byte-identical to the static render.
    endpoints_static: bool,
    /// Maximum number of intermediate frames a single star changed tier in.
    max_per_star_changes: usize,
    /// Number of distinct per-star change patterns across the intermediate
    /// frames (asynchrony evidence).
    distinct_tier_patterns: usize,
    /// Per intermediate frame: distinct displacement direction vectors.
    direction_vectors: Vec<usize>,
    /// Stars displaced more than one terminal cell (must be zero).
    displaced_beyond_one_cell: usize,
}

fn measure_scene(
    scene: &SceneRender,
    ground_truth: &[(usize, usize, char)],
    is_starfield: bool,
) -> FrameMetrics {
    let base = frame_grid(&scene.frames[0]);
    let star_count = ground_truth.len();
    let intermediate = scene.frames.len().saturating_sub(2);

    // Sanity: the static frame must agree with the ground-truth decision.
    for &(x, y, glyph) in ground_truth {
        assert_eq!(
            base.rows[y][x], glyph,
            "static frame must agree with the star decision at ({x}, {y})"
        );
    }

    let mut tier_changed_fraction = Vec::with_capacity(intermediate);
    let mut position_moved_fraction = Vec::with_capacity(intermediate);
    let mut position_moved_count = Vec::with_capacity(intermediate);
    let mut missing = Vec::with_capacity(intermediate);
    let mut occluded = Vec::with_capacity(intermediate);
    let mut extra = Vec::with_capacity(intermediate);
    let mut morphology_delta = Vec::with_capacity(intermediate);
    let mut direction_vectors = Vec::with_capacity(intermediate);
    let mut per_star_changes = vec![0usize; star_count];
    let mut change_patterns = vec![0u8; star_count];
    let mut output_counts: Vec<usize> = Vec::with_capacity(scene.frames.len());
    let mut geometry_stable = true;
    let mut displaced_beyond_one_cell = 0usize;

    let gt_set: std::collections::HashSet<(usize, usize)> =
        ground_truth.iter().map(|&(x, y, _)| (x, y)).collect();
    // Frame-0 visible structure cells: non-space cells that are not
    // ground-truth star cells (the population A5 arm motion may touch).
    let mut visible_cells = 0usize;
    for (y, row) in base.rows.iter().enumerate() {
        for (x, ch) in row.iter().enumerate() {
            if *ch != ' ' && !gt_set.contains(&(x, y)) {
                visible_cells += 1;
            }
        }
    }

    for (frame_index, frame) in scene.frames.iter().enumerate() {
        let grid = frame_grid(frame);
        if grid.rows.len() != base.rows.len() || grid.widths != base.widths {
            geometry_stable = false;
        }
        output_counts.push(starfield_output_stars(&grid).len());

        if frame_index == 0 || frame_index + 1 == scene.frames.len() {
            continue;
        }

        let output_stars = starfield_output_stars(&grid);
        let tier_by_pos: std::collections::HashMap<(usize, usize), char> = output_stars
            .iter()
            .map(|&(x, y, ch)| ((x, y), ch))
            .collect();
        let output_set: std::collections::HashSet<(usize, usize)> =
            output_stars.iter().map(|&(x, y, _)| (x, y)).collect();

        let mut tier_changed = 0usize;
        let mut morphology_delta_count = 0usize;
        if is_starfield {
            for (star_index, &(x, y, glyph)) in ground_truth.iter().enumerate() {
                // A star that moved away in this frame is counted by the
                // position metrics, not by the twinkle metrics: only a star
                // still sitting on its base cell can show a tier change.
                // (No other star can ever occupy this cell: motion
                // destinations must be empty in the base frame.)
                if !output_set.contains(&(x, y)) {
                    continue;
                }
                if tier_by_pos.get(&(x, y)).copied() != Some(glyph) {
                    tier_changed += 1;
                    per_star_changes[star_index] += 1;
                    change_patterns[star_index] |= 1 << (frame_index - 1);
                }
            }
        } else {
            // Galaxy renderers: tier glyphs can also belong to the galaxy
            // structure (Ascii), so the ground-truth decision is the only
            // star source. A ground-truth star cell must keep showing a
            // tier glyph; a missing one is counted as `missing` below.
            for (star_index, &(x, y, glyph)) in ground_truth.iter().enumerate() {
                let ch = grid.rows[y][x];
                if star_tier(ch).is_none() {
                    continue;
                }
                if ch != glyph {
                    tier_changed += 1;
                    per_star_changes[star_index] += 1;
                    change_patterns[star_index] |= 1 << (frame_index - 1);
                }
            }
        }
        for (y, row) in base.rows.iter().enumerate() {
            for (x, base_ch) in row.iter().enumerate() {
                if gt_set.contains(&(x, y)) {
                    continue;
                }
                if grid.rows[y][x] != *base_ch {
                    morphology_delta_count += 1;
                }
            }
        }

        // Galaxy: a ground-truth star cell that is no longer a tier glyph is
        // either space (a true disappearance, always a bug) or a structure
        // glyph (occluded by the shifted galaxy pattern, A5 Spiral only).
        let (missing_count, occluded_count) = if is_starfield {
            (gt_set.difference(&output_set).count(), 0)
        } else {
            let mut missing_count = 0usize;
            let mut occluded_count = 0usize;
            for &(x, y, _) in ground_truth.iter() {
                if star_tier(grid.rows[y][x]).is_some() {
                    continue;
                }
                if grid.rows[y][x] == ' ' {
                    missing_count += 1;
                } else {
                    occluded_count += 1;
                }
            }
            (missing_count, occluded_count)
        };
        let moved_in = if is_starfield {
            output_set.difference(&gt_set).count()
        } else {
            0
        };

        let neighbor_of = |set: &std::collections::HashSet<(usize, usize)>,
                           (x, y): (usize, usize)| {
            [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(ox, oy)| {
                let (nx, ny) = (x as isize + ox, y as isize + oy);
                nx >= 0 && ny >= 0 && set.contains(&(nx as usize, ny as usize))
            })
        };
        for pos in output_set.difference(&gt_set) {
            if !neighbor_of(&gt_set, *pos) {
                displaced_beyond_one_cell += 1;
            }
        }
        for pos in gt_set.difference(&output_set) {
            if !neighbor_of(&output_set, *pos) {
                displaced_beyond_one_cell += 1;
            }
        }

        let mut directions = std::collections::HashSet::new();
        for &(x, y, _) in output_stars
            .iter()
            .filter(|s| !gt_set.contains(&(s.0, s.1)))
        {
            for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (sx, sy) = (x as isize + ox, y as isize + oy);
                if sx >= 0 && sy >= 0 && gt_set.contains(&(sx as usize, sy as usize)) {
                    directions.insert((ox, oy));
                }
            }
        }

        tier_changed_fraction.push(tier_changed as f64 / star_count.max(1) as f64);
        position_moved_fraction.push(moved_in as f64 / star_count.max(1) as f64);
        position_moved_count.push(moved_in);
        missing.push(missing_count);
        occluded.push(occluded_count);
        extra.push(moved_in);
        morphology_delta.push(morphology_delta_count);
        direction_vectors.push(directions.len());
    }

    let created_or_destroyed = if is_starfield {
        output_counts.iter().max().copied().unwrap_or(0)
            - output_counts.iter().min().copied().unwrap_or(0)
    } else {
        missing.iter().copied().max().unwrap_or(0) + extra.iter().copied().max().unwrap_or(0)
    };

    let endpoints_static = scene.frames.first() == Some(&scene.static_frame)
        && scene.frames.last() == Some(&scene.static_frame);

    FrameMetrics {
        stars: star_count,
        tier_changed_fraction,
        position_moved_fraction,
        position_moved_count,
        missing,
        occluded,
        visible_cells,
        extra,
        morphology_delta,
        created_or_destroyed,
        geometry_stable,
        endpoints_static,
        max_per_star_changes: per_star_changes.iter().copied().max().unwrap_or(0),
        distinct_tier_patterns: change_patterns
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        direction_vectors,
        displaced_beyond_one_cell,
    }
}

/// Hard invariants every galaxy scene must satisfy on every intermediate
/// frame.
///
/// `spiral_motion_allowed` is the A5 relaxation: the Spiral model
/// intentionally re-evaluates its frozen morphology at a small deterministic
/// phase on intermediate frames, so a bounded share of its visible structure
/// cells may change and a pinned background star may be occluded (hidden
/// behind shifted structure). A pinned star may still never vanish into
/// space. The changed-cell budget below is a broad regression/diagnostic
/// ceiling, not a guarantee of how subtle the drift is: it exists to catch
/// catastrophic full-frame structural movement, since even a modest real
/// rotation rewrites far more structure cells than the budget allows. Every
/// non-Spiral model keeps the strict A4 contract (zero morphology delta,
/// zero occlusion).
fn assert_galaxy_invariants(scene_name: &str, metrics: &FrameMetrics, spiral_motion_allowed: bool) {
    assert!(
        metrics.missing.iter().all(|m| *m == 0) && metrics.extra.iter().all(|e| *e == 0),
        "{scene_name}: background star coordinates must be invariant across frames"
    );
    if spiral_motion_allowed {
        // Broad regression/diagnostic ceiling, not a "subtle drift"
        // guarantee: on the audit fixtures the A5 drift (2.5 deg phase)
        // peaks at 33% of visible cells for HalfBlock, 41% for Shade, and
        // 61% for Ascii (the finer glyph ladder quantizes the same angular
        // shift more visibly), so the 75% budget sits well above the
        // expected motion. Its job is to catch catastrophic full-frame
        // structural movement: any appreciable rotation (tens of degrees)
        // rewrites ~all of the pattern and fails this bound.
        let budget = metrics.visible_cells * 3 / 4;
        assert!(
            metrics.morphology_delta.iter().all(|d| *d <= budget),
            "{scene_name}: A5 arm motion exceeds the broad regression ceiling (max delta {} > budget {} over {} visible cells; deltas={:?})",
            metrics.morphology_delta.iter().copied().max().unwrap_or(0),
            budget,
            metrics.visible_cells,
            metrics.morphology_delta
        );
        // Occlusion of pinned stars by shifted structure is the expected
        // A5 consequence; the count is reported by the audit table.
    } else {
        assert!(
            metrics.morphology_delta.iter().all(|d| *d == 0),
            "{scene_name}: galaxy foreground/morphology cells must be invariant across frames"
        );
        assert!(
            metrics.occluded.iter().all(|o| *o == 0),
            "{scene_name}: structure must never shift over background stars"
        );
    }
    assert!(
        metrics.created_or_destroyed == 0,
        "{scene_name}: no background star may be created or destroyed"
    );
    assert!(
        metrics.geometry_stable,
        "{scene_name}: geometry must be stable"
    );
    assert!(
        metrics.endpoints_static,
        "{scene_name}: endpoints must equal the static render"
    );
}

/// Hard invariants every dedicated-Starfield scene must satisfy on every
/// intermediate frame.
fn assert_starfield_invariants(scene_name: &str, metrics: &FrameMetrics) {
    assert!(metrics.stars > 0, "{scene_name}: scene must contain stars");
    assert!(
        metrics.created_or_destroyed == 0,
        "{scene_name}: star count must be preserved (no creation/destruction)"
    );
    assert!(
        metrics.displaced_beyond_one_cell == 0,
        "{scene_name}: every star displacement must stay within one terminal cell"
    );
    assert!(
        metrics.geometry_stable,
        "{scene_name}: line count and visible widths must be stable"
    );
    assert!(
        metrics.endpoints_static,
        "{scene_name}: endpoints must equal the static render"
    );
}

// ===== Audit tests =====

/// Quantifies twinkle and motion fractions for the representative audit
/// fixtures. Run with `--nocapture` to print the measurement table; the
/// structural invariants (count, geometry, endpoints) are asserted,
/// while the structural motion ceilings live in the dedicated contract
/// tests.
#[test]
fn test_audit_report_twinkle_and_motion_fractions() {
    let galaxy_cases: [(ArtModel, RendererChoice, &str); 6] = [
        (
            ArtModel::Spiral,
            RendererChoice::HalfBlock,
            "Spiral/HalfBlock",
        ),
        (ArtModel::Spiral, RendererChoice::Shade, "Spiral/Shade"),
        (ArtModel::Spiral, RendererChoice::Ascii, "Spiral/Ascii"),
        (
            ArtModel::Spiral,
            RendererChoice::Quadrant,
            "Spiral/Quadrant",
        ),
        (
            ArtModel::Elliptical,
            RendererChoice::Auto,
            "Elliptical/Auto",
        ),
        (ArtModel::Cluster, RendererChoice::Auto, "Cluster/Auto"),
    ];

    for (model, renderer, name) in galaxy_cases.iter() {
        for &seed in &AUDIT_SEEDS {
            let scene = render_scene(model.clone(), *renderer, seed, AUDIT_ART, false);
            let ground_truth = if *renderer == RendererChoice::Quadrant {
                quadrant_ground_truth(&scene, AUDIT_ART.0)
            } else {
                galaxy_ground_truth(&scene, AUDIT_ART.0)
            };
            let metrics = measure_scene(&scene, &ground_truth, false);
            let spiral = matches!(*model, ArtModel::Spiral);
            assert_galaxy_invariants(name, &metrics, spiral);
            let stars = metrics.stars;
            let tier = if metrics.stars > 0 {
                metrics
                    .tier_changed_fraction
                    .iter()
                    .map(|v| format!("{v:.2}"))
                    .collect::<Vec<_>>()
            } else {
                vec!["-".to_string(); 4]
            };
            let motion = if spiral {
                format!(
                    " motion/frame={:?} occl/frame={:?} visible={}",
                    metrics.morphology_delta, metrics.occluded, metrics.visible_cells
                )
            } else {
                String::new()
            };
            eprintln!(
                "AUDIT {name:<16} seed={seed:<6} stars={stars:<4} tier/frame={tier:?} created=0 geometry=ok endpoints=ok{motion}"
            );
        }
    }

    let starfield_cases: [(ArtModel, RendererChoice, &str); 1] =
        [(ArtModel::Starfield, RendererChoice::Auto, "Starfield/Auto")];
    for (model, renderer, name) in starfield_cases.iter() {
        for &seed in &AUDIT_SEEDS {
            for art in [AUDIT_ART, AUDIT_ART_WIDE] {
                let scene = render_scene(model.clone(), *renderer, seed, art, false);
                let ground_truth = starfield_output_stars(&frame_grid(&scene.frames[0]));
                let metrics = measure_scene(&scene, &ground_truth, true);
                assert_starfield_invariants(name, &metrics);
                let tier = metrics
                    .tier_changed_fraction
                    .iter()
                    .map(|v| format!("{v:.2}"))
                    .collect::<Vec<_>>();
                let pos = metrics
                    .position_moved_fraction
                    .iter()
                    .map(|v| format!("{v:.2}"))
                    .collect::<Vec<_>>();
                eprintln!(
                    "AUDIT {name:<16} seed={seed:<6} art={art:?} stars={:<4} tier/frame={tier:?} pos/frame={pos:?} dirs/frame={:?} created=0 geometry=ok endpoints=ok",
                    metrics.stars, metrics.direction_vectors
                );
            }
        }
    }
}

/// Galaxy background star coordinates must be invariant across all frames
/// for every galaxy renderer and seed. Galaxy structure must be invariant
/// for every non-Spiral model; the A5 Spiral arm motion may rewrite a
/// bounded fraction of the visible structure cells (and may occlude pinned
/// background stars). The 75%-of-visible-cells ceiling is a broad
/// regression bound against catastrophic full-frame structural movement,
/// not a precision guarantee on the drift's subtlety.
#[test]
fn test_audit_galaxy_positions_and_morphology_invariant() {
    let cases: [(ArtModel, RendererChoice, &str); 5] = [
        (
            ArtModel::Spiral,
            RendererChoice::HalfBlock,
            "Spiral/HalfBlock",
        ),
        (ArtModel::Spiral, RendererChoice::Shade, "Spiral/Shade"),
        (ArtModel::Spiral, RendererChoice::Ascii, "Spiral/Ascii"),
        (
            ArtModel::Spiral,
            RendererChoice::Quadrant,
            "Spiral/Quadrant",
        ),
        (
            ArtModel::Elliptical,
            RendererChoice::Auto,
            "Elliptical/Auto",
        ),
    ];

    for (model, renderer, name) in cases.iter() {
        for &seed in &AUDIT_SEEDS {
            let scene = render_scene(model.clone(), *renderer, seed, AUDIT_ART, false);
            let ground_truth = if *renderer == RendererChoice::Quadrant {
                quadrant_ground_truth(&scene, AUDIT_ART.0)
            } else {
                galaxy_ground_truth(&scene, AUDIT_ART.0)
            };
            let metrics = measure_scene(&scene, &ground_truth, false);
            let spiral = matches!(*model, ArtModel::Spiral);
            assert_galaxy_invariants(name, &metrics, spiral);
            if spiral {
                // A5: with a nonzero phase the pattern must actually move
                // (at least one intermediate frame differs from frame 0),
                // while the change stays within the broad regression ceiling.
                assert!(
                    metrics.morphology_delta.iter().any(|d| *d > 0),
                    "{name} seed={seed}: A5 phase motion must be visible in some frame"
                );
            }
            if metrics.stars >= 4 {
                assert!(
                    metrics.tier_changed_fraction.iter().any(|f| *f > 0.0),
                    "{name} seed={seed}: twinkle must be visible across the sequence"
                );
            }
        }
    }
}

/// The dedicated Starfield preserves its star count, moves each star by at
/// most one terminal cell, and keeps the first/final frames byte-identical
/// to the static render on real generated scenes.
#[test]
fn test_audit_starfield_count_and_displacement_on_real_scenes() {
    let cases: [(u64, (usize, usize)); 4] = [
        (7, AUDIT_ART),
        (42, AUDIT_ART),
        (1234, AUDIT_ART),
        (42, AUDIT_ART_WIDE),
    ];
    for (seed, art) in cases.iter() {
        let scene = render_scene(
            ArtModel::Starfield,
            RendererChoice::Auto,
            *seed,
            *art,
            false,
        );
        let ground_truth = starfield_output_stars(&frame_grid(&scene.frames[0]));
        let metrics = measure_scene(&scene, &ground_truth, true);
        assert_starfield_invariants("Starfield/Auto", &metrics);
    }
}

/// Repeated frame generation from the same seed must be byte-identical, in
/// color and no-color.
#[test]
fn test_audit_repeated_generation_is_byte_identical() {
    let cases: [(ArtModel, RendererChoice, bool); 4] = [
        (ArtModel::Starfield, RendererChoice::Auto, false),
        (ArtModel::Starfield, RendererChoice::Auto, true),
        (ArtModel::Spiral, RendererChoice::HalfBlock, false),
        (ArtModel::Spiral, RendererChoice::HalfBlock, true),
    ];

    for (model, renderer, colors) in cases.iter() {
        let first = render_scene(model.clone(), *renderer, 42, AUDIT_ART, *colors);
        let second = render_scene(model.clone(), *renderer, 42, AUDIT_ART, *colors);
        assert_eq!(
            first.frames, second.frames,
            "{model:?}/{renderer:?} colors={colors}: repeated frame generation must be byte-identical"
        );
        assert_eq!(
            first.static_frame, second.static_frame,
            "{model:?}/{renderer:?} colors={colors}: static render must be byte-identical"
        );
    }
}

/// Twinkle must be asynchronous: no two-a-like global pulse. Real scenes
/// must contain at least two distinct per-star change patterns (when at
/// least two stars exist), and no single star may twinkle in more than two
/// of the four intermediate frames.
#[test]
fn test_audit_twinkle_is_asynchronous_not_global_pulse() {
    let cases: [(ArtModel, RendererChoice, &str, bool); 3] = [
        (
            ArtModel::Starfield,
            RendererChoice::Auto,
            "Starfield/Auto",
            true,
        ),
        (
            ArtModel::Spiral,
            RendererChoice::HalfBlock,
            "Spiral/HalfBlock",
            false,
        ),
        (
            ArtModel::Spiral,
            RendererChoice::Quadrant,
            "Spiral/Quadrant",
            false,
        ),
    ];

    for (model, renderer, name, is_starfield) in cases.iter() {
        let scene = render_scene(model.clone(), *renderer, 42, AUDIT_ART, false);
        let ground_truth = if *is_starfield {
            starfield_output_stars(&frame_grid(&scene.frames[0]))
        } else if *renderer == RendererChoice::Quadrant {
            quadrant_ground_truth(&scene, AUDIT_ART.0)
        } else {
            galaxy_ground_truth(&scene, AUDIT_ART.0)
        };
        let metrics = measure_scene(&scene, &ground_truth, *is_starfield);
        if metrics.stars >= 2 {
            assert!(
                metrics.distinct_tier_patterns >= 2,
                "{name}: stars must not twinkle in lockstep (patterns={:?})",
                metrics.distinct_tier_patterns
            );
        }
        assert!(
            metrics.max_per_star_changes <= 2,
            "{name}: a single star may not twinkle in more than 2 of the 4 intermediate frames"
        );
    }
}

/// Starfield motion must stay a clear minority of stars on every frame
/// (nominally 1/8 propose motion), be visible in most frames, and use
/// multiple displacement directions when more than one star moves (no
/// global translation).
#[test]
fn test_audit_starfield_motion_is_minority_without_global_translation() {
    for &seed in &AUDIT_SEEDS {
        let scene = render_scene(
            ArtModel::Starfield,
            RendererChoice::Auto,
            seed,
            AUDIT_ART,
            false,
        );
        let ground_truth = starfield_output_stars(&frame_grid(&scene.frames[0]));
        let metrics = measure_scene(&scene, &ground_truth, true);
        assert_starfield_invariants("Starfield/Auto", &metrics);

        let moved_frames = metrics
            .position_moved_fraction
            .iter()
            .filter(|f| **f > 0.0)
            .count();
        assert!(
            moved_frames >= 3,
            "seed={seed}: motion must be visible in at least 3 of 4 intermediate frames (got {moved_frames})"
        );
        for (frame, &fraction) in metrics.position_moved_fraction.iter().enumerate() {
            assert!(
                fraction <= 0.25,
                "seed={seed} frame {frame}: only a minority of stars may move per frame (fraction {fraction})"
            );
        }
        for (frame, &directions) in metrics.direction_vectors.iter().enumerate() {
            if metrics.position_moved_count[frame] >= 2 {
                assert!(
                    directions >= 2,
                    "seed={seed} frame {frame}: motion must not be a single global direction (got {directions})"
                );
            }
        }
    }
}

/// Colored animated frames keep stable visible widths, and the last ANSI
/// sequence of every styled line is RESET (no ANSI state may leak across
/// lines or frames).
#[test]
fn test_audit_colored_frames_keep_widths_and_reset_ansi_state() {
    let cases: [(ArtModel, RendererChoice, &str); 2] = [
        (ArtModel::Starfield, RendererChoice::Auto, "Starfield/Auto"),
        (
            ArtModel::Spiral,
            RendererChoice::HalfBlock,
            "Spiral/HalfBlock",
        ),
    ];

    for (model, renderer, name) in cases.iter() {
        let scene = render_scene(model.clone(), *renderer, 42, AUDIT_ART, true);
        let is_starfield = matches!(model, ArtModel::Starfield);
        let ground_truth = if is_starfield {
            starfield_output_stars(&frame_grid(&scene.frames[0]))
        } else {
            galaxy_ground_truth(&scene, AUDIT_ART.0)
        };
        let metrics = measure_scene(&scene, &ground_truth, is_starfield);
        assert!(
            metrics.geometry_stable,
            "{name}: colored visible widths must be stable across frames"
        );
        assert!(
            metrics.endpoints_static,
            "{name}: colored endpoints must equal the static render"
        );
        for frame in &scene.frames {
            for line in frame {
                if let Some(pos) = line.rfind('\x1b') {
                    assert!(
                        line[pos..].starts_with("\x1b[0m"),
                        "{name}: the last ANSI sequence of every line must be RESET (no state leakage)"
                    );
                }
            }
        }
    }
}

/// Side-by-side composition keeps the system information region
/// byte-identical on every animated frame and the layout geometry stable.
#[test]
fn test_audit_combined_side_by_side_keeps_system_lines_identical() {
    let app = build_test_app_pipeline(
        ArtModel::Starfield,
        RendererChoice::Auto,
        Some(42),
        true,
        false,
    );
    let terminal = Terminal::with_colors(true, false);
    let info_lines = app.build_info_lines(&base_snapshot());
    let prepared = app
        .prepare_art(false, EngineModel::Starfield, AUDIT_ART.0, AUDIT_ART.1)
        .unwrap();
    let art_frames = app.render_animation_frames(&terminal, &prepared).unwrap();
    const GAP: usize = 2;

    let outputs: Vec<Vec<String>> = art_frames
        .iter()
        .map(|art| {
            compose_layout(
                art,
                &info_lines,
                AUDIT_ART.0,
                crate::display_plan::LayoutKind::SideBySide,
            )
        })
        .collect();

    let widths: Vec<usize> = outputs[0].iter().map(|line| visible_width(line)).collect();
    for (frame_index, (output, art_lines)) in outputs.iter().zip(art_frames.iter()).enumerate() {
        assert_eq!(
            output
                .iter()
                .map(|line| visible_width(line))
                .collect::<Vec<_>>(),
            widths,
            "side-by-side layout geometry must be stable across frames"
        );
        for (line_index, info_line) in info_lines.iter().enumerate() {
            let art_line = &art_lines[line_index];
            assert_eq!(
                visible_width(art_line),
                AUDIT_ART.0,
                "art must render exactly its planned width"
            );
            let expected = format!("{art_line}{:padding$}{info_line}", "", padding = GAP);
            assert_eq!(
                &output[line_index], &expected,
                "frame {frame_index} line {line_index}: system information must be byte-identical on every side-by-side frame"
            );
        }
    }
}
