/// Display plan types for combining art and information.
use crate::cli::LayoutChoice;
use crate::terminal::TerminalDimensions;

/// The final display plan with layout choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayPlan {
    /// Only art, no information.
    LogoOnly { art: ArtPlan },
    /// Both art and information with a chosen layout.
    Combined { art: ArtPlan, layout: LayoutKind },
}

/// Plan for the art dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtPlan {
    pub width: usize,
    pub height: usize,
}

/// Layout kind for combining art and information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutKind {
    SideBySide,
    Stacked,
}

/// Request/input type for the display planner.
#[derive(Debug, Clone, Copy)]
pub struct PlannerRequest {
    /// Optional terminal dimensions (None when not a TTY).
    pub terminal_dimensions: Option<TerminalDimensions>,
    /// Optional requested art width (None means automatic).
    pub requested_width: Option<usize>,
    /// Optional requested art height (None means automatic).
    pub requested_height: Option<usize>,
    /// Requested layout choice.
    pub requested_layout: LayoutChoice,
    /// Output mode.
    pub output_mode: OutputMode,
    /// Measured information visible width.
    pub info_visible_width: usize,
    /// Measured information line count.
    pub info_line_count: usize,
}

/// Output mode for the planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Combined mode: art + information.
    Combined,
    /// Logo only mode: art only.
    LogoOnly,
}

// Constants for dimension calculations
pub const FALLBACK_WIDTH: usize = 40;
pub const FALLBACK_HEIGHT: usize = 20;
pub const PREFERRED_SIDE_MIN_WIDTH: usize = 20;
pub const GAP: usize = 2;

/// The display planner.
pub struct DisplayPlanner;

impl DisplayPlanner {
    /// Creates a new display planner.
    pub fn new() -> Self {
        DisplayPlanner
    }

    /// Plans the display based on the request.
    pub fn plan(&self, request: PlannerRequest) -> DisplayPlan {
        match request.output_mode {
            OutputMode::LogoOnly => {
                let art = self.plan_art_for_logo_only(
                    request.requested_width,
                    request.requested_height,
                    request.terminal_dimensions,
                );
                DisplayPlan::LogoOnly { art }
            }
            OutputMode::Combined => {
                let terminal_dims = request.terminal_dimensions;
                let info_width = request.info_visible_width;
                let info_lines = request.info_line_count;

                // First determine layout
                let layout = self.plan_layout(
                    request.requested_layout,
                    terminal_dims,
                    request.requested_width,
                    info_width,
                );

                // Then plan art dimensions based on layout
                let art = self.plan_art_for_layout(
                    request.requested_width,
                    request.requested_height,
                    terminal_dims,
                    info_width,
                    info_lines,
                    layout,
                );

                DisplayPlan::Combined { art, layout }
            }
        }
    }

    /// Plans art dimensions for LogoOnly mode, adapting to terminal dimensions.
    fn plan_art_for_logo_only(
        &self,
        requested_width: Option<usize>,
        requested_height: Option<usize>,
        terminal_dims: Option<TerminalDimensions>,
    ) -> ArtPlan {
        let terminal_width = terminal_dims.map(|d| d.width);
        let terminal_height = terminal_dims.map(|d| d.height);

        // Derive preferred dimensions using shared helper
        let PreferredArt {
            width: preferred_width,
            height: preferred_height,
            width_is_automatic,
            height_is_automatic,
        } = Self::derive_preferred_dimensions(requested_width, requested_height);

        let width = if width_is_automatic {
            match terminal_width {
                Some(tw) => preferred_width.min(tw.max(1)),
                None => preferred_width,
            }
        } else {
            requested_width.unwrap_or(preferred_width)
        };

        let height = if height_is_automatic {
            // Recalculate derived height based on final width
            let derived_height = width.div_ceil(2);
            match terminal_height {
                Some(th) => derived_height.min(th.max(1)),
                None => derived_height.min(preferred_height),
            }
        } else {
            requested_height.unwrap_or(preferred_height)
        };

        ArtPlan { width, height }
    }

    /// Plans the layout based on request and dimensions.
    fn plan_layout(
        &self,
        requested_layout: LayoutChoice,
        terminal_dims: Option<TerminalDimensions>,
        requested_width: Option<usize>,
        info_width: usize,
    ) -> LayoutKind {
        match requested_layout {
            LayoutChoice::Auto => self.plan_auto_layout(terminal_dims, requested_width, info_width),
            LayoutChoice::SideBySide => LayoutKind::SideBySide,
            LayoutChoice::Stacked => LayoutKind::Stacked,
        }
    }

    /// Plans auto layout based on terminal dimensions.
    fn plan_auto_layout(
        &self,
        terminal_dims: Option<TerminalDimensions>,
        requested_width: Option<usize>,
        info_width: usize,
    ) -> LayoutKind {
        let terminal_width = match terminal_dims {
            Some(d) => d.width,
            None => return LayoutKind::SideBySide, // No terminal, default to side-by-side
        };

        // Safe arithmetic: prevent underflow when info is wider than terminal
        let occupied_by_info = GAP.saturating_add(info_width);
        let available_for_art = terminal_width.saturating_sub(occupied_by_info);

        // 1. If explicit width is specified, only SideBySide if it fits
        if let Some(w) = requested_width {
            if w <= available_for_art {
                return LayoutKind::SideBySide;
            }
            // Explicit width doesn't fit, use Stacked
            return LayoutKind::Stacked;
        }

        // 2. Automatic width: SideBySide if preferred fits (possibly shrunk)
        if FALLBACK_WIDTH <= available_for_art {
            return LayoutKind::SideBySide;
        }

        // 3. If width is automatic and available_for_art >= PREFERRED_SIDE_MIN_WIDTH, shrink and use SideBySide
        if available_for_art >= PREFERRED_SIDE_MIN_WIDTH {
            // Width is automatic and fits after shrinking
            return LayoutKind::SideBySide;
        }

        // 4. Otherwise use Stacked
        LayoutKind::Stacked
    }

    /// Plans art dimensions based on layout decision.
    fn plan_art_for_layout(
        &self,
        requested_width: Option<usize>,
        requested_height: Option<usize>,
        terminal_dims: Option<TerminalDimensions>,
        info_width: usize,
        info_line_count: usize,
        layout: LayoutKind,
    ) -> ArtPlan {
        let terminal_width = terminal_dims.map(|d| d.width);
        let terminal_height = terminal_dims.map(|d| d.height);

        match layout {
            LayoutKind::SideBySide => {
                let preferred =
                    Self::derive_preferred_dimensions(requested_width, requested_height);

                let available_width = terminal_width
                    .map(|terminal| terminal.saturating_sub(GAP.saturating_add(info_width)));

                let final_width = if preferred.width_is_automatic {
                    available_width
                        .map(|available| preferred.width.min(available.max(1)))
                        .unwrap_or(preferred.width)
                } else {
                    preferred.width
                };

                let derived_height = if preferred.height_is_automatic {
                    final_width.div_ceil(2)
                } else {
                    preferred.height
                };

                let final_height = if preferred.height_is_automatic {
                    terminal_height
                        .map(|th| derived_height.min(th.max(1)))
                        .unwrap_or(derived_height)
                } else {
                    preferred.height
                };

                ArtPlan {
                    width: final_width.max(1),
                    height: final_height.max(1),
                }
            }
            LayoutKind::Stacked => {
                // For Stacked, we use the original plan_art logic
                self.plan_art_for_stacked(
                    requested_width,
                    requested_height,
                    terminal_dims,
                    info_line_count,
                )
            }
        }
    }

    /// Plans art dimensions for stacked layout with vertical space calculation.
    fn plan_art_for_stacked(
        &self,
        requested_width: Option<usize>,
        requested_height: Option<usize>,
        terminal_dims: Option<TerminalDimensions>,
        info_line_count: usize,
    ) -> ArtPlan {
        let terminal_width = terminal_dims.map(|d| d.width);
        let terminal_height = terminal_dims.map(|d| d.height);

        // Derive preferred dimensions using shared helper
        let PreferredArt {
            width: preferred_width,
            height: _preferred_height,
            width_is_automatic,
            height_is_automatic,
        } = Self::derive_preferred_dimensions(requested_width, requested_height);

        let width = if width_is_automatic {
            match terminal_width {
                Some(tw) => preferred_width.min(tw.max(1)),
                None => preferred_width,
            }
        } else {
            requested_width.unwrap_or(preferred_width)
        };

        // Recalculate preferred height from final width
        let preferred_height = width.div_ceil(2);

        let height = if height_is_automatic {
            // Calculate reserved space for information
            let reserved = if info_line_count > 0 {
                info_line_count.saturating_add(1) // info lines + separator
            } else {
                0
            };

            match terminal_height {
                Some(th) => {
                    let available_art_height = th.saturating_sub(reserved);
                    if available_art_height >= 1 {
                        preferred_height.min(available_art_height)
                    } else {
                        1
                    }
                }
                None => preferred_height.min(preferred_height),
            }
        } else {
            requested_height.unwrap_or(preferred_height)
        };

        ArtPlan { width, height }
    }

    /// Derives preferred dimensions and records which are automatic.
    fn derive_preferred_dimensions(
        requested_width: Option<usize>,
        requested_height: Option<usize>,
    ) -> PreferredArt {
        match (requested_width, requested_height) {
            (Some(w), Some(h)) => {
                // Both explicit: use exact values, neither automatic
                PreferredArt {
                    width: w,
                    height: h,
                    width_is_automatic: false,
                    height_is_automatic: false,
                }
            }
            (Some(w), None) => {
                // Width explicit, height automatic
                PreferredArt {
                    width: w,
                    height: w.div_ceil(2),
                    width_is_automatic: false,
                    height_is_automatic: true,
                }
            }
            (None, Some(h)) => {
                // Height explicit, width automatic
                PreferredArt {
                    width: h.saturating_mul(2),
                    height: h,
                    width_is_automatic: true,
                    height_is_automatic: false,
                }
            }
            (None, None) => {
                // Both automatic: use fallback, both automatic
                PreferredArt {
                    width: FALLBACK_WIDTH,
                    height: FALLBACK_HEIGHT,
                    width_is_automatic: true,
                    height_is_automatic: true,
                }
            }
        }
    }
}

/// Helper struct for derived preferred dimensions.
struct PreferredArt {
    width: usize,
    height: usize,
    width_is_automatic: bool,
    height_is_automatic: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planner() -> DisplayPlanner {
        DisplayPlanner::new()
    }

    fn terminal_dims(width: usize, height: usize) -> TerminalDimensions {
        TerminalDimensions { width, height }
    }

    #[test]
    fn test_plan_logo_only() {
        let request = PlannerRequest {
            terminal_dimensions: None,
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::LogoOnly,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::LogoOnly { art } => {
                assert_eq!(art.width, FALLBACK_WIDTH);
                assert_eq!(art.height, FALLBACK_HEIGHT);
            }
            _ => panic!("Expected LogoOnly"),
        }
    }

    #[test]
    fn test_plan_combined_no_terminal_uses_fallback() {
        let request = PlannerRequest {
            terminal_dimensions: None,
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, layout } => {
                assert_eq!(art.width, FALLBACK_WIDTH);
                assert_eq!(art.height, FALLBACK_HEIGHT);
                assert_eq!(layout, LayoutKind::SideBySide);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_plan_combined_wide_terminal_uses_side_by_side() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(120, 40)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, layout } => {
                assert_eq!(art.width, FALLBACK_WIDTH);
                assert_eq!(art.height, FALLBACK_HEIGHT);
                assert_eq!(layout, LayoutKind::SideBySide);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_plan_explicit_width_is_not_shrunk() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(55, 40)),
            requested_width: Some(40),
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, layout } => {
                assert_eq!(art.width, 40);
                assert_eq!(layout, LayoutKind::Stacked);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_plan_explicit_stacked_is_honored() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(120, 40)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Stacked,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art: _, layout } => {
                assert_eq!(layout, LayoutKind::Stacked);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_plan_explicit_side_by_side_is_honored() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(60, 40)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::SideBySide,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art: _, layout } => {
                assert_eq!(layout, LayoutKind::SideBySide);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_plan_width_only_derives_height() {
        let request = PlannerRequest {
            terminal_dimensions: None,
            requested_width: Some(40),
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, .. } => {
                assert_eq!(art.width, 40);
                assert_eq!(art.height, 20);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_plan_height_only_derives_width() {
        let request = PlannerRequest {
            terminal_dimensions: None,
            requested_width: None,
            requested_height: Some(20),
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, .. } => {
                assert_eq!(art.width, 40);
                assert_eq!(art.height, 20);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_plan_logo_only_does_not_depend_on_info_dimensions() {
        let request = PlannerRequest {
            terminal_dimensions: None,
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::LogoOnly,
            info_visible_width: 100,
            info_line_count: 50,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::LogoOnly { art } => {
                assert_eq!(art.width, FALLBACK_WIDTH);
                assert_eq!(art.height, FALLBACK_HEIGHT);
            }
            _ => panic!("Expected LogoOnly"),
        }
    }

    #[test]
    fn test_plan_auto_height_respects_terminal_height() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(80, 30)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, .. } => {
                assert_eq!(art.height, 20);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_automatic_vs_explicit_width_behavior() {
        let request_automatic = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(55, 30)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 20,
            info_line_count: 10,
        };

        let request_explicit = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(55, 30)),
            requested_width: Some(40),
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 20,
            info_line_count: 10,
        };

        let planner = DisplayPlanner::new();
        let plan_automatic = planner.plan(request_automatic);
        let plan_explicit = planner.plan(request_explicit);

        match (plan_automatic, plan_explicit) {
            (
                DisplayPlan::Combined {
                    art: art_auto,
                    layout: layout_auto,
                    ..
                },
                DisplayPlan::Combined {
                    art: art_exp,
                    layout: layout_exp,
                    ..
                },
            ) => {
                assert_eq!(layout_auto, LayoutKind::SideBySide);
                assert_eq!(art_auto.width, 33);
                assert_eq!(layout_exp, LayoutKind::Stacked);
                assert_eq!(art_exp.width, 40);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_automatic_layout_stacked_when_info_wider_than_terminal() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(20, 40)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { layout, .. } => {
                assert_eq!(layout, LayoutKind::Stacked);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_logo_only_wide_terminal_adapts() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(120, 40)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::LogoOnly,
            info_visible_width: 0,
            info_line_count: 0,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::LogoOnly { art } => {
                assert_eq!(art.width, 40);
                assert_eq!(art.height, 20);
            }
            _ => panic!("Expected LogoOnly"),
        }
    }

    #[test]
    fn test_logo_only_no_terminal_uses_fallback() {
        let request = PlannerRequest {
            terminal_dimensions: None,
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::LogoOnly,
            info_visible_width: 0,
            info_line_count: 0,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::LogoOnly { art } => {
                assert_eq!(art.width, FALLBACK_WIDTH);
                assert_eq!(art.height, FALLBACK_HEIGHT);
            }
            _ => panic!("Expected LogoOnly"),
        }
    }

    #[test]
    fn test_logo_only_explicit_dimensions() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(30, 12)),
            requested_width: Some(50),
            requested_height: Some(25),
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::LogoOnly,
            info_visible_width: 0,
            info_line_count: 0,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::LogoOnly { art } => {
                assert_eq!(art.width, 50);
                assert_eq!(art.height, 25);
            }
            _ => panic!("Expected LogoOnly"),
        }
    }

    #[test]
    fn test_side_by_side_auto_height_uses_terminal_height() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(120, 10)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, layout } => {
                assert_eq!(layout, LayoutKind::SideBySide);
                assert_eq!(art.height, 10);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_side_by_side_explicit_width_auto_height() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(80, 10)),
            requested_width: Some(40),
            requested_height: None,
            requested_layout: LayoutChoice::SideBySide,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, layout } => {
                assert_eq!(layout, LayoutKind::SideBySide);
                assert_eq!(art.width, 40);
                assert_eq!(art.height, 10);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_height_only_no_terminal() {
        let request = PlannerRequest {
            terminal_dimensions: None,
            requested_width: None,
            requested_height: Some(30),
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, .. } => {
                assert_eq!(art.width, 60);
                assert_eq!(art.height, 30);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_auto_height_preserves_2_1_ratio_after_width_shrinking() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(60, 40)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 30,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, layout } => {
                assert_eq!(art.width, 28);
                assert_eq!(art.height, 14);
                assert_eq!(layout, LayoutKind::SideBySide);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_medium_terminal_explicit_width_uses_stacked() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(55, 30)),
            requested_width: Some(40),
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 20,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, layout } => {
                assert_eq!(layout, LayoutKind::Stacked);
                assert_eq!(art.width, 40);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_stacked_vertical_planning_with_info_lines() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(40, 20)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Stacked,
            output_mode: OutputMode::Combined,
            info_visible_width: 20,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, layout } => {
                assert_eq!(layout, LayoutKind::Stacked);
                assert_eq!(art.width, 40);
                assert_eq!(art.height, 9);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_stacked_insufficient_height() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(40, 8)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Stacked,
            output_mode: OutputMode::Combined,
            info_visible_width: 20,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, layout } => {
                assert_eq!(layout, LayoutKind::Stacked);
                assert_eq!(art.width, 40);
                assert_eq!(art.height, 1);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_stacked_explicit_height() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(40, 8)),
            requested_width: None,
            requested_height: Some(25),
            requested_layout: LayoutChoice::Stacked,
            output_mode: OutputMode::Combined,
            info_visible_width: 20,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, layout } => {
                assert_eq!(layout, LayoutKind::Stacked);
                assert_eq!(art.width, 40);
                assert_eq!(art.height, 25);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_height_only_with_terminal() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(40, 20)),
            requested_width: None,
            requested_height: Some(30),
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 20,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, .. } => {
                assert_eq!(art.width, 40);
                assert_eq!(art.height, 30);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_width_only_with_terminal() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(40, 20)),
            requested_width: Some(60),
            requested_height: None,
            requested_layout: LayoutChoice::Auto,
            output_mode: OutputMode::Combined,
            info_visible_width: 20,
            info_line_count: 10,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, .. } => {
                assert_eq!(art.width, 60);
                assert_eq!(art.height, 9);
            }
            _ => panic!("Expected Combined"),
        }
    }

    #[test]
    fn test_stacked_empty_info_lines() {
        let request = PlannerRequest {
            terminal_dimensions: Some(terminal_dims(40, 20)),
            requested_width: None,
            requested_height: None,
            requested_layout: LayoutChoice::Stacked,
            output_mode: OutputMode::Combined,
            info_visible_width: 0,
            info_line_count: 0,
        };

        let plan = planner().plan(request);
        match plan {
            DisplayPlan::Combined { art, layout } => {
                assert_eq!(layout, LayoutKind::Stacked);
                assert_eq!(art.width, 40);
                assert_eq!(art.height, 20);
            }
            _ => panic!("Expected Combined"),
        }
    }
}
