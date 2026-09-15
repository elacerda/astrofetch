use super::{
    ansi::AnsiForegroundLine,
    color::{starfield_foreground_ansi, ColorPalette},
    hash::{hash_cell, hash_to_unit},
    twinkle_star_glyph, StarTwinkleFrame,
};
use crate::seed::{derive_feature_seed, ANIMATION_STAR_MOTION_V1};
use crate::terminal::Terminal;

const STAR_MOTION_SUBSET_MODULUS: u64 = 8;
const STAR_MOTION_FRAME_SALT: u64 = 0x4f1b_2d93_8a7c_65e1;
const STAR_MOTION_DIRECTIONS: [(isize, isize); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

#[derive(Debug, Clone, Copy)]
struct StarCell {
    value: f64,
    glyph: char,
    source_x: usize,
    source_y: usize,
}

#[derive(Debug, Clone, Copy)]
struct StarMove {
    destination_x: usize,
    destination_y: usize,
    priority: u64,
}

pub fn render_starfield(
    canvas: &[Vec<f64>],
    colors_enabled: bool,
    terminal: &Terminal,
    palette: ColorPalette,
) -> Vec<String> {
    render_starfield_with_twinkle(canvas, colors_enabled, terminal, palette, None)
}

/// Renders the dedicated starfield with an optional deterministic twinkle frame.
pub(crate) fn render_starfield_with_twinkle(
    canvas: &[Vec<f64>],
    colors_enabled: bool,
    terminal: &Terminal,
    palette: ColorPalette,
    frame: Option<StarTwinkleFrame>,
) -> Vec<String> {
    let (base_cells, width) = prepare_starfield_cells(canvas);
    let moved_cells = frame
        .filter(|frame| is_intermediate_frame(*frame))
        .map(|frame| move_starfield_cells(&base_cells, frame));
    let cells = moved_cells.as_deref().unwrap_or(&base_cells);

    render_starfield_cells(cells, width, colors_enabled, terminal, palette, frame)
}

/// Converts prepared density rows into one visible-star cell per terminal
/// cell. This is a presentation-only sampling pass; it does not generate or
/// mutate scene density.
fn prepare_starfield_cells(canvas: &[Vec<f64>]) -> (Vec<Vec<Option<StarCell>>>, usize) {
    let width = canvas.first().map_or(0, Vec::len);
    let logical_height = canvas.len().div_ceil(2);
    let mut cells = vec![vec![None; width]; logical_height];

    for (y, top_row) in canvas.iter().step_by(2).enumerate() {
        for (x, cell) in cells[y].iter_mut().enumerate() {
            let top = top_row.get(x).copied().unwrap_or(0.0);
            let bottom = canvas
                .get(y * 2 + 1)
                .and_then(|row| row.get(x))
                .copied()
                .unwrap_or(0.0);

            let value = top.max(bottom);
            let glyph = starfield_glyph(value);
            if glyph != ' ' {
                *cell = Some(StarCell {
                    value,
                    glyph,
                    source_x: x,
                    source_y: y,
                });
            }
        }
    }

    (cells, width)
}

/// Returns true only for frames between the two static animation endpoints.
fn is_intermediate_frame(frame: StarTwinkleFrame) -> bool {
    frame.frame_count > 1
        && frame.frame_index > 0
        && frame.frame_index < frame.frame_count.saturating_sub(1)
}

/// Derives a stable per-star, per-frame motion value without consuming any
/// legacy generation RNG stream.
fn star_motion_hash(source_x: usize, source_y: usize, frame_index: u32, motion_seed: u64) -> u64 {
    let frame_seed = hash_cell(
        frame_index as usize,
        0,
        motion_seed ^ STAR_MOTION_FRAME_SALT,
    );
    hash_cell(source_x, source_y, motion_seed ^ frame_seed.rotate_left(23))
}

/// Returns a cardinal destination one terminal cell from x and y.
///
/// Out-of-bounds directions are rejected instead of clamped or wrapped, so
/// edge stars remain inside the terminal frame.
fn cardinal_destination(
    x: usize,
    y: usize,
    direction: usize,
    width: usize,
    height: usize,
) -> Option<(usize, usize)> {
    match STAR_MOTION_DIRECTIONS[direction] {
        (0, -1) if y > 0 => Some((x, y - 1)),
        (1, 0) if x < width.saturating_sub(1) => Some((x + 1, y)),
        (0, 1) if y < height.saturating_sub(1) => Some((x, y + 1)),
        (-1, 0) if x > 0 => Some((x - 1, y)),
        _ => None,
    }
}

/// Applies deterministic, collision-safe micro-motion to prepared stars.
fn move_starfield_cells(
    base_cells: &[Vec<Option<StarCell>>],
    frame: StarTwinkleFrame,
) -> Vec<Vec<Option<StarCell>>> {
    let height = base_cells.len();
    let width = base_cells.first().map_or(0, Vec::len);
    let motion_seed = derive_feature_seed(frame.scene_seed, ANIMATION_STAR_MOTION_V1);
    let mut stars = Vec::new();

    for (source_y, row) in base_cells.iter().enumerate() {
        for (source_x, cell) in row.iter().enumerate() {
            if let Some(cell) = cell {
                stars.push((source_x, source_y, *cell));
            }
        }
    }

    let mut proposals: Vec<Option<StarMove>> = vec![None; stars.len()];
    let mut winner_at: Vec<Vec<Option<usize>>> = vec![vec![None; width]; height];

    for (star_index, &(source_x, source_y, _)) in stars.iter().enumerate() {
        let motion = star_motion_hash(source_x, source_y, frame.frame_index, motion_seed);
        if !motion.is_multiple_of(STAR_MOTION_SUBSET_MODULUS) {
            continue;
        }

        let direction = ((motion >> 8) % STAR_MOTION_DIRECTIONS.len() as u64) as usize;
        let Some((destination_x, destination_y)) =
            cardinal_destination(source_x, source_y, direction, width, height)
        else {
            continue;
        };

        // Only claim destinations that were empty in the prepared frame. This
        // prevents a moving star from overwriting a stationary or moving star.
        if base_cells[destination_y][destination_x].is_some() {
            continue;
        }

        let proposal = StarMove {
            destination_x,
            destination_y,
            priority: motion,
        };
        proposals[star_index] = Some(proposal);

        let current_winner = winner_at[destination_y][destination_x];
        let wins = current_winner.is_none_or(|current_index| {
            let current_priority = proposals[current_index]
                .expect("a destination winner must have a proposal")
                .priority;
            (proposal.priority, star_index) < (current_priority, current_index)
        });
        if wins {
            winner_at[destination_y][destination_x] = Some(star_index);
        }
    }

    let mut moved_cells = base_cells.to_vec();
    for (star_index, proposal) in proposals.into_iter().enumerate() {
        let Some(proposal) = proposal else {
            continue;
        };
        if winner_at[proposal.destination_y][proposal.destination_x] != Some(star_index) {
            continue;
        }

        let (source_x, source_y, cell) = stars[star_index];
        moved_cells[source_y][source_x] = None;
        moved_cells[proposal.destination_y][proposal.destination_x] = Some(cell);
    }

    moved_cells
}

/// Renders a prepared Starfield cell grid, optionally applying A2 twinkle to
/// each star while retaining source coordinates for deterministic identity.
fn render_starfield_cells(
    cells: &[Vec<Option<StarCell>>],
    width: usize,
    colors_enabled: bool,
    terminal: &Terminal,
    palette: ColorPalette,
    frame: Option<StarTwinkleFrame>,
) -> Vec<String> {
    let mut lines = Vec::with_capacity(cells.len());

    for row in cells {
        let mut line = AnsiForegroundLine::with_capacity(width);

        for cell in row {
            let Some(cell) = cell else {
                line.push_plain(' ');
                continue;
            };

            let ch = frame.map_or(cell.glyph, |frame| {
                twinkle_star_glyph(
                    Some(cell.glyph),
                    frame.scene_seed,
                    cell.source_x,
                    cell.source_y,
                    frame.frame_index,
                    frame.frame_count,
                )
                .unwrap_or(cell.glyph)
            });

            if colors_enabled && terminal.colors_enabled() {
                let hue = hash_to_unit(hash_cell(
                    cell.source_x,
                    cell.source_y,
                    0x51a7_f17e_d00d_cafe,
                ));
                let color = starfield_foreground_ansi(palette, cell.value, hue);
                line.push_styled(ch, color);
            } else {
                line.push_plain(ch);
            }
        }

        lines.push(line.finish());
    }

    lines
}

fn starfield_glyph(value: f64) -> char {
    if value < 0.030 {
        ' '
    } else if value < 0.085 {
        '.'
    } else if value < 0.150 {
        '*'
    } else {
        '+'
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cardinal_destination, move_starfield_cells, prepare_starfield_cells, render_starfield,
        render_starfield_with_twinkle, starfield_foreground_ansi, starfield_glyph,
    };
    use crate::render::color::{ColorPalette, RESET};
    use crate::render::hash::{hash_cell, hash_to_unit};
    use crate::render::StarTwinkleFrame;
    use crate::terminal::Terminal;

    // ===== Starfield ANSI grouping tests =====

    #[test]
    fn test_starfield_same_style_run_uses_single_ansi_sequence() {
        let seed = 0x51a7_f17e_d00d_cafe;
        let hue_a = hash_to_unit(hash_cell(0, 0, seed));
        let hue_b = hash_to_unit(hash_cell(1, 0, seed));

        let value_a = 0.05;
        let value_b = 0.05;

        let glyph_a = starfield_glyph(value_a);
        let glyph_b = starfield_glyph(value_b);

        let style_a = starfield_foreground_ansi(ColorPalette::Nebula, value_a, hue_a);
        let style_b = starfield_foreground_ansi(ColorPalette::Nebula, value_b, hue_b);

        assert_eq!(glyph_a, glyph_b);
        assert_eq!(style_a, style_b);

        let canvas = vec![vec![value_a, value_b, value_a, value_b], vec![0.0; 4]];

        let terminal = Terminal::with_colors(true, true);
        let result = render_starfield(&canvas, true, &terminal, ColorPalette::Nebula);

        assert_eq!(
            result[0],
            format!("{style_a}{glyph_a}{glyph_b}{glyph_a}{glyph_b}{RESET}")
        );

        let style_count = result[0].matches(style_a).count();
        let reset_count = result[0].matches(RESET).count();

        assert_eq!(
            style_count, 1,
            "Same-style run should use single style sequence"
        );
        assert_eq!(reset_count, 1, "Same-style run should use single reset");
    }

    #[test]
    fn test_starfield_style_transition_pushes_reset() {
        let seed = 0x51a7_f17e_d00d_cafe;

        let hue_a = hash_to_unit(hash_cell(0, 0, seed));
        let hue_b = hash_to_unit(hash_cell(1, 0, seed));

        let value_a = 0.10;
        let value_b = 0.20;

        let glyph_a = starfield_glyph(value_a);
        let glyph_b = starfield_glyph(value_b);

        let style_a = starfield_foreground_ansi(ColorPalette::Nebula, value_a, hue_a);
        let style_b = starfield_foreground_ansi(ColorPalette::Nebula, value_b, hue_b);

        assert_ne!(style_a, style_b);

        let canvas = vec![vec![value_a, value_b], vec![0.0; 2]];

        let terminal = Terminal::with_colors(true, true);
        let result = render_starfield(&canvas, true, &terminal, ColorPalette::Nebula);

        assert_eq!(
            result[0],
            format!("{style_a}{glyph_a}{RESET}{style_b}{glyph_b}{RESET}")
        );
    }

    #[test]
    fn test_starfield_colored_to_plain() {
        let value = 0.05;
        let glyph = starfield_glyph(value);

        let seed = 0x51a7_f17e_d00d_cafe;
        let hue = hash_to_unit(hash_cell(0, 0, seed));
        let style = starfield_foreground_ansi(ColorPalette::Nebula, value, hue);

        let canvas = vec![vec![value, 0.0], vec![0.0; 2]];

        let terminal = Terminal::with_colors(true, true);
        let result = render_starfield(&canvas, true, &terminal, ColorPalette::Nebula);

        assert_eq!(result[0], format!("{}{}{} ", style, glyph, RESET));
    }

    fn checkerboard_canvas(width: usize, logical_height: usize) -> Vec<Vec<f64>> {
        let mut canvas = vec![vec![0.0; width]; logical_height * 2];
        for (y, row) in canvas.iter_mut().step_by(2).enumerate() {
            for (x, value) in row.iter_mut().enumerate() {
                if (x + y) % 2 == 0 {
                    *value = 0.20;
                }
            }
        }
        canvas
    }

    fn motion_frame(frame_index: u32) -> StarTwinkleFrame {
        StarTwinkleFrame {
            scene_seed: 42,
            frame_index,
            frame_count: 6,
        }
    }

    #[test]
    fn test_starfield_motion_is_deterministic_and_changes_intermediate_frames() {
        let canvas = checkerboard_canvas(16, 10);
        let terminal = Terminal::with_colors(true, false);
        let static_frame = render_starfield(&canvas, false, &terminal, ColorPalette::Nebula);
        let first = render_starfield_with_twinkle(
            &canvas,
            false,
            &terminal,
            ColorPalette::Nebula,
            Some(motion_frame(1)),
        );
        let first_endpoint = render_starfield_with_twinkle(
            &canvas,
            false,
            &terminal,
            ColorPalette::Nebula,
            Some(motion_frame(0)),
        );
        let first_repeat = render_starfield_with_twinkle(
            &canvas,
            false,
            &terminal,
            ColorPalette::Nebula,
            Some(motion_frame(1)),
        );
        let second = render_starfield_with_twinkle(
            &canvas,
            false,
            &terminal,
            ColorPalette::Nebula,
            Some(motion_frame(2)),
        );
        let final_frame = render_starfield_with_twinkle(
            &canvas,
            false,
            &terminal,
            ColorPalette::Nebula,
            Some(motion_frame(5)),
        );

        assert_eq!(first, first_repeat);
        assert_eq!(first_endpoint, static_frame);
        assert_ne!(first, second);
        assert_ne!(first, static_frame);
        assert_eq!(final_frame, static_frame);
    }

    #[test]
    fn test_starfield_motion_preserves_count_and_moves_at_most_one_cell() {
        let (base_cells, width) = prepare_starfield_cells(&checkerboard_canvas(16, 10));
        let moved_cells = move_starfield_cells(&base_cells, motion_frame(1));
        let height = base_cells.len();
        let base_count = base_cells
            .iter()
            .flat_map(|row| row.iter())
            .filter(|cell| cell.is_some())
            .count();
        let moved_count = moved_cells
            .iter()
            .flat_map(|row| row.iter())
            .filter(|cell| cell.is_some())
            .count();
        let mut seen_sources = vec![vec![false; width]; height];

        assert_eq!(base_count, moved_count);
        for (destination_y, row) in moved_cells.iter().enumerate() {
            for (destination_x, cell) in row.iter().enumerate() {
                let Some(cell) = cell else {
                    continue;
                };
                assert!(cell.source_x < width);
                assert!(cell.source_y < height);
                assert!(
                    destination_x.abs_diff(cell.source_x) + destination_y.abs_diff(cell.source_y)
                        <= 1,
                    "star moved more than one terminal cell"
                );
                assert!(!seen_sources[cell.source_y][cell.source_x]);
                seen_sources[cell.source_y][cell.source_x] = true;
            }
        }
        assert_eq!(
            seen_sources.iter().flatten().filter(|seen| **seen).count(),
            base_count
        );
    }

    #[test]
    fn test_starfield_motion_rejects_edge_destinations() {
        assert_eq!(cardinal_destination(0, 0, 0, 4, 4), None);
        assert_eq!(cardinal_destination(0, 0, 3, 4, 4), None);
        assert_eq!(cardinal_destination(3, 3, 1, 4, 4), None);
        assert_eq!(cardinal_destination(3, 3, 2, 4, 4), None);
        assert_eq!(cardinal_destination(0, 0, 1, 4, 4), Some((1, 0)));
        assert_eq!(cardinal_destination(0, 0, 2, 4, 4), Some((0, 1)));
    }
}
