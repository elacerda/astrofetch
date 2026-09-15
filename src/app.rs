use crate::cli::{Args, ArtModel, Command, RendererChoice};
use crate::display_plan::{DisplayPlanner, OutputMode, PlannerRequest};
use crate::engine::ArtModel as EngineModel;
use crate::error::AppError;
use crate::galaxy::{spiral_animation_phase_rad, PreparedSpiralScene};
use crate::layout::compose_layout;
use crate::render::topology::CellSamplingShape;
use crate::render::{
    prepare_density, prepare_density_with_shape, prepare_galaxy_density_pinned, render_ascii,
    render_ascii_with_twinkle, render_half_blocks, render_half_blocks_with_twinkle,
    render_quadrant_with_stars, render_quadrant_with_stars_at_frame, render_shades,
    render_shades_with_twinkle, render_starfield, render_starfield_with_twinkle,
    robust_normalization_bounds, sampling_shape_for, ColorPalette, EffectiveRenderer,
    PreparedDensity, RenderProfile, StarTwinkleFrame,
};
use crate::system::{
    get_disk_detail_fields, get_display_field_order, CollectionProfile, SystemSnapshot,
};
use crate::terminal::{visible_width, Terminal, TerminalDimensions};
use clap::Parser;

use crate::cli::PaletteChoice;

const HEADER_COLOR: &str = "\x1b[93m";
const LABEL_COLOR: &str = "\x1b[94m";
const VALUE_COLOR: &str = "\x1b[97m";
const RESET: &str = "\x1b[0m";

/// Resolve the effective color palette from the CLI palette choice.
///
/// The resolution matrix is infallible:
/// - Auto -> Nebula
/// - Nebula -> Nebula
/// - Cividis -> Cividis
/// - Amber -> Amber
/// - Mono -> Mono
pub(super) fn resolve_color_palette(requested: PaletteChoice) -> ColorPalette {
    match requested {
        PaletteChoice::Auto | PaletteChoice::Nebula => ColorPalette::Nebula,
        PaletteChoice::Cividis => ColorPalette::Cividis,
        PaletteChoice::Amber => ColorPalette::Amber,
        PaletteChoice::Mono => ColorPalette::Mono,
    }
}

#[derive(Debug)]
struct InfoLine {
    label: String,
    value: String,
}

/// Prepared art state shared by every frame of an animated intro.
///
/// Scene resolution, density generation, normalization, stretching, threshold
/// selection, and row materialization happen before the first frame is
/// rendered. Frame rendering only reads this state and applies a pure
/// presentation context to existing stars.
#[derive(Debug)]
struct PreparedArt {
    /// Concrete model selected for this scene.
    resolved_model: EngineModel,
    /// Concrete seed captured during scene resolution.
    scene_seed: u64,
    /// Terminal art width used during density generation.
    art_width: usize,
    /// Terminal art height used during density generation.
    art_height: usize,
    /// Effective terminal-cell sampling shape used during generation.
    sampling_shape: CellSamplingShape,
    /// Renderer selected after CLI/model resolution.
    effective_renderer: EffectiveRenderer,
    /// Prepared density rows and, for galaxy renderers, the fixed threshold.
    prepared_density: PreparedArtDensity,
    /// Effective application color state captured before rendering frames.
    colors_enabled: bool,
    /// Resolved palette shared by every frame.
    palette: ColorPalette,
    /// A5: frozen Spiral scene for intermediate animation frames.
    spiral_animation: Option<SpiralAnimationPrep>,
}

/// A5 Spiral intermediate-frame state (Spiral scenes only).
///
/// `scene` is the single frozen morphology derivation for the concrete scene
/// seed (spiral configuration, bar, dust, and noise seed). `bounds` are the
/// static frame's robust normalization percentile bounds, captured from the
/// same raw density the static preparation consumed. When `bounds` is
/// `None` (a degenerate static normalization), intermediate frames fall back
/// to the prepared static density so no new structure can appear mid-sequence.
#[derive(Debug, Clone)]
struct SpiralAnimationPrep {
    /// Frozen Spiral morphology for the concrete scene seed.
    scene: PreparedSpiralScene,
    /// Static-frame robust normalization bounds, if usable.
    bounds: Option<(f64, f64)>,
}

/// Row-materialized form of one prepared density.
#[derive(Debug)]
enum PreparedArtDensity {
    /// Dedicated Starfield density with no galaxy threshold.
    Starfield { canvas: Vec<Vec<f64>> },
    /// Galaxy density with the fixed prepared visibility threshold.
    Galaxy {
        canvas: Vec<Vec<f64>>,
        threshold: f64,
    },
}

/// Aplicação principal do AstroFetch.
pub struct App {
    args: Args,
    terminal: Terminal,
}

impl App {
    /// Cria uma nova instância do App.
    pub fn new() -> Result<Self, AppError> {
        let args = Args::parse();
        let terminal = Terminal::new();

        Ok(Self { args, terminal })
    }

    /// Executa o app principal.
    ///
    /// After the main work completes, a passive update notification check is
    /// performed. The check is silent, throttled to once per 24 hours, and
    /// gated on an interactive stderr, so it never delays or breaks the fetch
    /// output.
    pub fn run() -> Result<(), AppError> {
        let app = Self::new()?;
        let no_update_check = app.args.no_update_check;
        app.execute()?;
        crate::update_check::maybe_check(no_update_check);
        Ok(())
    }

    /// Executa a lógica principal.
    fn execute(&self) -> Result<(), AppError> {
        if let Some(command) = &self.args.command {
            return match command {
                Command::SetupShell(args) => crate::setup_shell::run(args),
                Command::UninstallShell(args) => crate::setup_shell::uninstall(args),
            };
        }

        // Verifica --no-color explicito
        let colors_enabled = if self.args.no_color {
            false
        } else {
            self.terminal.colors_enabled()
        };

        // Cria o terminal com a configuração final de cores
        let terminal = Terminal {
            is_tty: self.terminal.is_tty,
            colors_enabled,
        };

        // Detect terminal capabilities
        let terminal_dims = self.terminal.dimensions();

        // Branch by output mode
        if self.args.info_only {
            self.execute_info_only(&terminal)
        } else {
            let engine_model = match self.args.model {
                ArtModel::Random => EngineModel::Random,
                ArtModel::Elliptical => EngineModel::Elliptical,
                ArtModel::Spiral => EngineModel::Spiral,
                ArtModel::Cluster => EngineModel::Cluster,
                ArtModel::Starfield => EngineModel::Starfield,
            };

            if self.args.logo_only {
                self.execute_logo_only(&terminal, colors_enabled, engine_model, terminal_dims)
            } else {
                self.execute_combined(&terminal, colors_enabled, engine_model, terminal_dims)
            }
        }
    }

    /// Emits the final rendered frame.
    ///
    /// When `--animate` is enabled and stdout is an interactive terminal,
    /// the frozen frame is replayed in place through the short intro
    /// runner. The intro only starts when the whole frame safely fits the
    /// current terminal viewport without wrapping or scrolling; whenever it
    /// is skipped (or the handler cannot be installed, or stdout is not a
    /// TTY), the legacy static path is used unchanged, so non-animated and
    /// non-TTY output stays byte-identical.
    fn emit_output(&self, terminal: &Terminal, lines: &[String]) -> Result<(), AppError> {
        if crate::animation::should_animate(self.args.animate, terminal.is_tty) {
            match crate::animation::run_intro(lines) {
                Ok(crate::animation::IntroOutcome::Completed) => return Ok(()),
                Ok(crate::animation::IntroOutcome::Interrupted) => {
                    return Err(AppError::Interrupted);
                }
                Ok(crate::animation::IntroOutcome::Skipped) => {
                    // The frame does not safely fit the current terminal
                    // viewport (or its size is unknown): the intro wrote
                    // nothing, so fall through to the static path below.
                }
                Err(crate::animation::IntroError::HandlerInstall(_)) => {
                    // No terminal side effects happened yet: fall back to
                    // the static output instead of failing the run.
                }
                Err(crate::animation::IntroError::Io(err)) => {
                    return Err(AppError::Animation(err.to_string()));
                }
            }
        }
        terminal.print_lines(lines)?;
        Ok(())
    }

    /// Presents a fixed sequence of already-rendered frames.
    ///
    /// The terminal runner only receives strings; scene generation, density
    /// preparation, and star selection remain entirely in the app/render
    /// layers. Any geometry or handler gate failure falls back to the first
    /// frame, which is the legacy static render.
    fn emit_frames(&self, terminal: &Terminal, frames: &[Vec<String>]) -> Result<(), AppError> {
        if crate::animation::should_animate(self.args.animate, terminal.is_tty) {
            match crate::animation::run_intro_frames(frames) {
                Ok(crate::animation::IntroOutcome::Completed) => return Ok(()),
                Ok(crate::animation::IntroOutcome::Interrupted) => {
                    return Err(AppError::Interrupted);
                }
                Ok(crate::animation::IntroOutcome::Skipped) => {
                    // The intro wrote nothing: fall through to the static
                    // first frame below.
                }
                Err(crate::animation::IntroError::HandlerInstall(_)) => {
                    // No terminal side effects happened yet: use static output.
                }
                Err(crate::animation::IntroError::Io(err)) => {
                    return Err(AppError::Animation(err.to_string()));
                }
            }
        }

        if let Some(first_frame) = frames.first() {
            terminal.print_lines(first_frame)?;
        }
        Ok(())
    }

    /// Returns the collection profile based on CLI compact flag.
    fn collection_profile(&self) -> CollectionProfile {
        if self.args.compact {
            CollectionProfile::Compact
        } else {
            CollectionProfile::Full
        }
    }

    /// Executa em modo InfoOnly: apenas informações do sistema.
    fn execute_info_only(&self, terminal: &Terminal) -> Result<(), AppError> {
        // Build formatted information lines
        let system = SystemSnapshot::collect_with(self.collection_profile());
        let info_lines = self.build_info_lines(&system);

        terminal.print_lines(&info_lines)?;
        Ok(())
    }

    /// Executa em modo LogoOnly: apenas arte ASCII.
    fn execute_logo_only(
        &self,
        terminal: &Terminal,
        colors_enabled: bool,
        engine_model: EngineModel,
        terminal_dims: Option<TerminalDimensions>,
    ) -> Result<(), AppError> {
        // Detect terminal dimensions for art planning
        let planner = DisplayPlanner::new();
        let request = PlannerRequest {
            terminal_dimensions: terminal_dims,
            requested_width: self.args.width,
            requested_height: self.args.height,
            requested_layout: self.args.layout,
            output_mode: OutputMode::LogoOnly,
            info_visible_width: 0,
            info_line_count: 0,
        };

        let display_plan = planner.plan(request);

        let (art_width, art_height) = match display_plan {
            crate::display_plan::DisplayPlan::LogoOnly { art } => (art.width, art.height),
            _ => unreachable!(),
        };

        if crate::animation::should_animate(self.args.animate, terminal.is_tty) {
            let prepared = self.prepare_art(colors_enabled, engine_model, art_width, art_height)?;
            let frames = self.render_animation_frames(terminal, &prepared)?;
            self.emit_frames(terminal, &frames)
        } else {
            let art_lines = self.render_art(
                terminal,
                colors_enabled,
                engine_model,
                art_width,
                art_height,
            )?;

            self.emit_output(terminal, &art_lines)
        }
    }

    /// Executa em modo Combined: arte ASCII + informações do sistema.
    fn execute_combined(
        &self,
        terminal: &Terminal,
        colors_enabled: bool,
        engine_model: EngineModel,
        terminal_dims: Option<TerminalDimensions>,
    ) -> Result<(), AppError> {
        // Build formatted information lines and measure dimensions
        let system = SystemSnapshot::collect_with(self.collection_profile());
        let info_lines = self.build_info_lines(&system);
        let info_visible_width = info_lines
            .iter()
            .map(|s| visible_width(s))
            .max()
            .unwrap_or(0);
        let info_line_count = info_lines.len();

        // Resolve art dimensions from terminal capabilities and explicit overrides
        let planner = DisplayPlanner::new();
        let request = PlannerRequest {
            terminal_dimensions: terminal_dims,
            requested_width: self.args.width,
            requested_height: self.args.height,
            requested_layout: self.args.layout,
            output_mode: OutputMode::Combined,
            info_visible_width,
            info_line_count,
        };

        let display_plan = planner.plan(request);

        let (art_width, art_height) = match display_plan {
            crate::display_plan::DisplayPlan::Combined { art, .. } => (art.width, art.height),
            _ => unreachable!(),
        };

        if crate::animation::should_animate(self.args.animate, terminal.is_tty) {
            let prepared = self.prepare_art(colors_enabled, engine_model, art_width, art_height)?;
            let art_frames = self.render_animation_frames(terminal, &prepared)?;

            match display_plan {
                crate::display_plan::DisplayPlan::Combined { art, layout } => {
                    let output_frames: Vec<Vec<String>> = art_frames
                        .iter()
                        .map(|art_lines| compose_layout(art_lines, &info_lines, art.width, layout))
                        .collect();
                    self.emit_frames(terminal, &output_frames)?;
                }
                _ => unreachable!(),
            }
        } else {
            let art_lines = self.render_art(
                terminal,
                colors_enabled,
                engine_model,
                art_width,
                art_height,
            )?;

            match display_plan {
                crate::display_plan::DisplayPlan::Combined { art, layout } => {
                    let output_lines = compose_layout(&art_lines, &info_lines, art.width, layout);
                    self.emit_output(terminal, &output_lines)?;
                }
                _ => unreachable!(),
            }
        }

        Ok(())
    }

    /// Prepares one scene for static or animated presentation.
    ///
    /// Resolution and density generation happen exactly once in this method.
    /// The legacy `generate_scene` wrapper remains the source for every
    /// non-Quadrant renderer, while Quadrant keeps its existing split path so
    /// its concrete seed and sampling shape are preserved. The returned rows
    /// and threshold are immutable presentation state shared by all frames.
    fn prepare_art(
        &self,
        colors_enabled: bool,
        engine_model: EngineModel,
        art_width: usize,
        art_height: usize,
    ) -> Result<PreparedArt, AppError> {
        if engine_model == EngineModel::Random && self.args.renderer == RendererChoice::Quadrant {
            return Err(AppError::Cli(
                "the quadrant renderer cannot be combined with the random model; choose a concrete model (e.g. --model spiral)".to_string(),
            ));
        }

        let (resolved_model, scene_seed, density, effective_renderer, shape) = if self.args.renderer
            == RendererChoice::Quadrant
        {
            // Split path: the concrete seed from resolve_scene is
            // preserved and never re-rolled.
            let resolved = engine_model.resolve_scene(self.args.seed);
            let effective_renderer =
                Self::resolve_effective_renderer(self.args.renderer, resolved.resolved_model)?;
            let shape = sampling_shape_for(resolved.resolved_model, effective_renderer);
            let density = engine_model.generate_density(&resolved, art_width, art_height, shape);
            (
                resolved.resolved_model,
                resolved.seed,
                density,
                effective_renderer,
                shape,
            )
        } else {
            // Legacy path: unchanged for every existing renderer.
            let scene = engine_model.generate_scene(art_width, art_height, self.args.seed);
            let effective_renderer =
                Self::resolve_effective_renderer(self.args.renderer, scene.resolved_model)?;
            (
                scene.resolved_model,
                scene.seed,
                scene.density,
                effective_renderer,
                CellSamplingShape::HALF_BLOCK,
            )
        };

        let profile = RenderProfile::for_model_and_renderer(resolved_model, effective_renderer);
        // A5: for Spiral scenes, freeze the morphology once from the same
        // concrete seed the static generation used, and capture the static
        // frame's robust normalization bounds from the raw density before
        // preparation consumes it. Non-Spiral models never build this state.
        let spiral_animation = (resolved_model == EngineModel::Spiral).then(|| {
            let bounds = robust_normalization_bounds(&density, profile.normalization);
            SpiralAnimationPrep {
                scene: PreparedSpiralScene::for_scene_seed(scene_seed),
                bounds,
            }
        });
        // HALF_BLOCK keeps the legacy preparation call bit-for-bit; only
        // QUADRANT goes through the shape-aware preparation.
        let prepared = if shape == CellSamplingShape::QUADRANT {
            prepare_density_with_shape(density, profile, shape)
        } else {
            prepare_density(density, profile)
        };
        let effective_palette = resolve_color_palette(self.args.palette);

        let prepared_density = match prepared {
            PreparedDensity::Starfield { density } => PreparedArtDensity::Starfield {
                canvas: density.into_rows(),
            },
            PreparedDensity::Galaxy { density, threshold } => PreparedArtDensity::Galaxy {
                canvas: density.into_rows(),
                threshold,
            },
        };

        Ok(PreparedArt {
            resolved_model,
            scene_seed,
            art_width,
            art_height,
            sampling_shape: shape,
            effective_renderer,
            prepared_density,
            colors_enabled,
            palette: effective_palette,
            spiral_animation,
        })
    }

    /// Renders one frame from prepared art without resolving or generating it.
    fn render_prepared_art(
        prepared: &PreparedArt,
        terminal: &Terminal,
        frame: Option<StarTwinkleFrame>,
    ) -> Result<Vec<String>, AppError> {
        Self::render_art_density(prepared, terminal, frame, &prepared.prepared_density, None)
    }

    /// Renders one frame from an explicit prepared density.
    ///
    /// `density` is normally `prepared.prepared_density`; A5 Spiral
    /// intermediate frames pass a per-phase density instead. `star_canvas`
    /// optionally pins the background-star decision (star seed and per-cell
    /// local density) to a reference canvas, so a frame sequence whose
    /// structure canvas varies per frame still shows the exact same
    /// background stars. `None` keeps the legacy behavior: the star
    /// decision reads the structure canvas itself.
    fn render_art_density(
        prepared: &PreparedArt,
        terminal: &Terminal,
        frame: Option<StarTwinkleFrame>,
        density: &PreparedArtDensity,
        star_canvas: Option<&[Vec<f64>]>,
    ) -> Result<Vec<String>, AppError> {
        debug_assert_eq!(
            sampling_shape_for(prepared.resolved_model, prepared.effective_renderer),
            prepared.sampling_shape
        );

        let effective_colors = prepared.colors_enabled && terminal.colors_enabled();
        match (density, prepared.effective_renderer) {
            (PreparedArtDensity::Starfield { canvas }, EffectiveRenderer::Starfield) => {
                Ok(match frame {
                    Some(frame) => render_starfield_with_twinkle(
                        canvas,
                        prepared.colors_enabled,
                        terminal,
                        prepared.palette,
                        Some(frame),
                    ),
                    None => render_starfield(
                        canvas,
                        prepared.colors_enabled,
                        terminal,
                        prepared.palette,
                    ),
                })
            }
            (PreparedArtDensity::Galaxy { canvas, threshold }, EffectiveRenderer::HalfBlock) => {
                Ok(match frame {
                    Some(frame) => render_half_blocks_with_twinkle(
                        canvas,
                        *threshold,
                        effective_colors,
                        prepared.palette,
                        Some(frame),
                        star_canvas,
                    ),
                    None => {
                        render_half_blocks(canvas, *threshold, effective_colors, prepared.palette)
                    }
                })
            }
            (PreparedArtDensity::Galaxy { canvas, threshold }, EffectiveRenderer::Shade) => {
                Ok(match frame {
                    Some(frame) => render_shades_with_twinkle(
                        canvas,
                        *threshold,
                        effective_colors,
                        prepared.palette,
                        Some(frame),
                        star_canvas,
                    ),
                    None => render_shades(canvas, *threshold, effective_colors, prepared.palette),
                })
            }
            (PreparedArtDensity::Galaxy { canvas, threshold }, EffectiveRenderer::Ascii) => {
                Ok(match frame {
                    Some(frame) => render_ascii_with_twinkle(
                        canvas,
                        *threshold,
                        effective_colors,
                        prepared.palette,
                        Some(frame),
                        star_canvas,
                    ),
                    None => render_ascii(canvas, *threshold, effective_colors, prepared.palette),
                })
            }
            (PreparedArtDensity::Galaxy { canvas, threshold }, EffectiveRenderer::Quadrant) => {
                Ok(match frame {
                    Some(frame) => render_quadrant_with_stars_at_frame(
                        canvas,
                        *threshold,
                        effective_colors,
                        prepared.palette,
                        Some(frame),
                        star_canvas,
                    ),
                    None => render_quadrant_with_stars(
                        canvas,
                        *threshold,
                        effective_colors,
                        prepared.palette,
                    ),
                })
            }
            (PreparedArtDensity::Starfield { .. }, _) => Err(AppError::Render(
                "Starfield density cannot be rendered with galaxy renderers".to_string(),
            )),
            (PreparedArtDensity::Galaxy { .. }, EffectiveRenderer::Starfield) => {
                Err(AppError::Render(
                    "Galaxy density cannot be rendered with starfield renderer".to_string(),
                ))
            }
        }
    }

    /// Builds the fixed deterministic intro frames from one prepared scene.
    fn render_animation_frames(
        &self,
        terminal: &Terminal,
        prepared: &PreparedArt,
    ) -> Result<Vec<Vec<String>>, AppError> {
        let schedule = crate::animation::intro_schedule();
        let mut frames = Vec::with_capacity(schedule.frame_count as usize);

        for frame_index in 0..schedule.frame_count {
            let is_static_endpoint =
                frame_index == 0 || frame_index == schedule.frame_count.saturating_sub(1);
            let frame = if is_static_endpoint {
                None
            } else {
                Some(StarTwinkleFrame {
                    scene_seed: prepared.scene_seed,
                    frame_index,
                    frame_count: schedule.frame_count,
                })
            };

            // A5: intermediate Spiral frames re-evaluate the frozen
            // morphology at the frame phase. Static endpoints and every
            // non-Spiral model keep the prepared static density. When a
            // per-phase density is rendered, the star decision is pinned to
            // the static canvas so the background star field never re-rolls.
            let frame_density = match (frame, prepared.spiral_animation.as_ref()) {
                (Some(_), Some(prep)) if prep.bounds.is_some() => {
                    let phase = spiral_animation_phase_rad(frame_index, schedule.frame_count);
                    Some(Self::spiral_frame_density(prepared, &prep.scene, phase))
                }
                _ => None,
            };
            let (density, star_canvas): (&PreparedArtDensity, Option<&[Vec<f64>]>) =
                match &frame_density {
                    Some(frame_density) => {
                        let static_canvas = match &prepared.prepared_density {
                            PreparedArtDensity::Galaxy { canvas, .. } => Some(canvas.as_slice()),
                            PreparedArtDensity::Starfield { .. } => None,
                        };
                        (frame_density, static_canvas)
                    }
                    None => (&prepared.prepared_density, None),
                };

            frames.push(Self::render_art_density(
                prepared,
                terminal,
                frame,
                density,
                star_canvas,
            )?);
        }

        Ok(frames)
    }

    /// A5: prepares one intermediate-frame density for a Spiral scene.
    ///
    /// The frozen scene morphology is re-evaluated at `phase` (no RNG draw is
    /// consumed), then prepared with the static frame's threshold and the
    /// static frame's pinned robust normalization bounds, so only the
    /// angular pattern varies between frames. Callers guarantee that
    /// `prepared.spiral_animation` exists with usable bounds.
    fn spiral_frame_density(
        prepared: &PreparedArt,
        scene: &PreparedSpiralScene,
        phase: f64,
    ) -> PreparedArtDensity {
        let threshold = match &prepared.prepared_density {
            PreparedArtDensity::Galaxy { threshold, .. } => *threshold,
            PreparedArtDensity::Starfield { .. } => {
                unreachable!("Spiral scenes always prepare galaxy densities")
            }
        };
        let bounds = prepared
            .spiral_animation
            .as_ref()
            .and_then(|prep| prep.bounds)
            .expect("spiral_frame_density is only called with usable bounds");
        let profile = RenderProfile::for_model_and_renderer(
            prepared.resolved_model,
            prepared.effective_renderer,
        );
        let raw = scene.density_at(
            prepared.art_width,
            prepared.art_height,
            prepared.sampling_shape,
            phase,
        );
        let density = prepare_galaxy_density_pinned(&raw, profile, bounds);
        PreparedArtDensity::Galaxy {
            canvas: density.into_rows(),
            threshold,
        }
    }

    /// Shared art pipeline for logo-only and combined static output.
    ///
    /// Static rendering delegates to the same preparation and dispatch used by
    /// animated frames. Its frame context is absent, so existing renderer
    /// behavior and byte-level output remain unchanged.
    fn render_art(
        &self,
        terminal: &Terminal,
        colors_enabled: bool,
        engine_model: EngineModel,
        art_width: usize,
        art_height: usize,
    ) -> Result<Vec<String>, AppError> {
        let prepared = self.prepare_art(colors_enabled, engine_model, art_width, art_height)?;
        Self::render_prepared_art(&prepared, terminal, None)
    }

    /// Resolves the effective renderer based on the requested renderer choice and resolved model.
    ///
    /// Every explicit renderer choice works with every concrete model, with
    /// the exception of the experimental Quadrant renderer, which currently
    /// supports Spiral only:
    /// - Galaxy models (Spiral, Elliptical, Cluster) with Auto → HalfBlock
    /// - Galaxy models with HalfBlock → HalfBlock
    /// - Galaxy models with Shade → Shade
    /// - Galaxy models with Ascii → Ascii
    /// - Spiral with Quadrant → Quadrant
    /// - Elliptical/Cluster/Starfield with Quadrant → Cli error (unsupported)
    /// - Starfield with Auto → Starfield
    /// - Starfield with HalfBlock → HalfBlock
    /// - Starfield with Shade → Shade
    /// - Starfield with Ascii → Ascii
    /// - Random model (unresolved) → Render error (should never happen)
    fn resolve_effective_renderer(
        requested: RendererChoice,
        resolved_model: EngineModel,
    ) -> Result<EffectiveRenderer, AppError> {
        match (resolved_model, requested) {
            // Galaxy models
            (EngineModel::Spiral, RendererChoice::Auto) => Ok(EffectiveRenderer::HalfBlock),
            (EngineModel::Spiral, RendererChoice::HalfBlock) => Ok(EffectiveRenderer::HalfBlock),
            (EngineModel::Spiral, RendererChoice::Shade) => Ok(EffectiveRenderer::Shade),
            (EngineModel::Spiral, RendererChoice::Ascii) => Ok(EffectiveRenderer::Ascii),
            (EngineModel::Spiral, RendererChoice::Quadrant) => Ok(EffectiveRenderer::Quadrant),
            (EngineModel::Elliptical, RendererChoice::Auto) => Ok(EffectiveRenderer::HalfBlock),
            (EngineModel::Elliptical, RendererChoice::HalfBlock) => {
                Ok(EffectiveRenderer::HalfBlock)
            }
            (EngineModel::Elliptical, RendererChoice::Shade) => Ok(EffectiveRenderer::Shade),
            (EngineModel::Elliptical, RendererChoice::Ascii) => Ok(EffectiveRenderer::Ascii),
            (EngineModel::Elliptical, RendererChoice::Quadrant) => {
                Err(Self::quadrant_unsupported_model_error())
            }
            (EngineModel::Cluster, RendererChoice::Auto) => Ok(EffectiveRenderer::HalfBlock),
            (EngineModel::Cluster, RendererChoice::HalfBlock) => Ok(EffectiveRenderer::HalfBlock),
            (EngineModel::Cluster, RendererChoice::Shade) => Ok(EffectiveRenderer::Shade),
            (EngineModel::Cluster, RendererChoice::Ascii) => Ok(EffectiveRenderer::Ascii),
            (EngineModel::Cluster, RendererChoice::Quadrant) => {
                Err(Self::quadrant_unsupported_model_error())
            }

            // Starfield model
            (EngineModel::Starfield, RendererChoice::Auto) => Ok(EffectiveRenderer::Starfield),
            (EngineModel::Starfield, RendererChoice::HalfBlock) => Ok(EffectiveRenderer::HalfBlock),
            (EngineModel::Starfield, RendererChoice::Shade) => Ok(EffectiveRenderer::Shade),
            (EngineModel::Starfield, RendererChoice::Ascii) => Ok(EffectiveRenderer::Ascii),
            (EngineModel::Starfield, RendererChoice::Quadrant) => {
                Err(Self::quadrant_unsupported_model_error())
            }

            // Random model should be resolved before this function is called
            (EngineModel::Random, _) => Err(AppError::Render(
                "unresolved Random model reached renderer selection (internal error)".to_string(),
            )),
        }
    }

    /// Clear error for an explicit `--renderer quadrant` on a model that the
    /// experimental Quadrant renderer does not support yet.
    fn quadrant_unsupported_model_error() -> AppError {
        AppError::Cli(
            "the quadrant renderer currently supports the spiral model only; use --model spiral with --renderer quadrant".to_string(),
        )
    }

    /// Renders prepared density using the effective renderer.
    ///
    /// Validates that the prepared density and effective renderer are compatible.
    #[cfg_attr(not(test), allow(dead_code))]
    fn render_prepared_density(
        prepared: PreparedDensity,
        effective_renderer: EffectiveRenderer,
        colors_enabled: bool,
        terminal: &Terminal,
        palette: ColorPalette,
    ) -> Result<Vec<String>, AppError> {
        match (prepared, effective_renderer) {
            (PreparedDensity::Starfield { density }, EffectiveRenderer::Starfield) => {
                let canvas = density.into_rows();
                Ok(render_starfield(&canvas, colors_enabled, terminal, palette))
            }
            (PreparedDensity::Galaxy { density, threshold }, EffectiveRenderer::HalfBlock) => {
                let canvas = density.into_rows();
                Ok(render_half_blocks(
                    &canvas,
                    threshold,
                    colors_enabled && terminal.colors_enabled(),
                    palette,
                ))
            }
            (PreparedDensity::Galaxy { density, threshold }, EffectiveRenderer::Shade) => {
                let canvas = density.into_rows();
                Ok(render_shades(
                    &canvas,
                    threshold,
                    colors_enabled && terminal.colors_enabled(),
                    palette,
                ))
            }
            (PreparedDensity::Galaxy { density, threshold }, EffectiveRenderer::Ascii) => {
                let canvas = density.into_rows();
                Ok(render_ascii(
                    &canvas,
                    threshold,
                    colors_enabled && terminal.colors_enabled(),
                    palette,
                ))
            }
            (PreparedDensity::Galaxy { density, threshold }, EffectiveRenderer::Quadrant) => {
                let canvas = density.into_rows();
                Ok(render_quadrant_with_stars(
                    &canvas,
                    threshold,
                    colors_enabled && terminal.colors_enabled(),
                    palette,
                ))
            }
            // Internal mismatch - should never happen if resolve_effective_renderer is correct
            (PreparedDensity::Starfield { .. }, _) => Err(AppError::Render(
                "Starfield density cannot be rendered with galaxy renderers".to_string(),
            )),
            (PreparedDensity::Galaxy { .. }, EffectiveRenderer::Starfield) => {
                Err(AppError::Render(
                    "Galaxy density cannot be rendered with starfield renderer".to_string(),
                ))
            }
        }
    }

    /// Constrói as linhas de informações do sistema.
    fn build_info_lines(&self, system: &SystemSnapshot) -> Vec<String> {
        let mut lines = Vec::new();
        let colors_enabled = self.info_colors_enabled();

        // user@host (apenas em modo full)
        if !self.args.compact {
            lines.push(format_header(&system.user_host, colors_enabled));
        }

        let mut info_fields: Vec<InfoLine> = get_display_field_order(system, self.args.compact)
            .into_iter()
            .map(|field_name| InfoLine {
                label: field_name.to_string(),
                value: system.get(field_name),
            })
            .collect();

        if self.args.disk_details {
            let disk_detail_fields: Vec<InfoLine> = get_disk_detail_fields()
                .into_iter()
                .map(|field| InfoLine {
                    label: field.label,
                    value: field.value,
                })
                .collect();

            if !disk_detail_fields.is_empty() {
                if let Some(disk_index) = info_fields.iter().position(|line| line.label == "Disk") {
                    info_fields.splice(disk_index + 1..disk_index + 1, disk_detail_fields);
                } else {
                    info_fields.extend(disk_detail_fields);
                }
            }
        }
        let label_width = info_fields
            .iter()
            .map(|line| visible_width(&line.label))
            .max()
            .unwrap_or(0);

        lines.extend(
            info_fields
                .iter()
                .map(|line| format_info_line(line, label_width, colors_enabled)),
        );

        lines
    }

    fn info_colors_enabled(&self) -> bool {
        !self.args.no_color && self.terminal.colors_enabled()
    }
}

fn format_header(text: &str, colors_enabled: bool) -> String {
    if colors_enabled {
        format!("{}{}{}", HEADER_COLOR, text, RESET)
    } else {
        text.to_string()
    }
}

fn format_info_line(line: &InfoLine, label_width: usize, colors_enabled: bool) -> String {
    let label_padding = " ".repeat(label_width.saturating_sub(visible_width(&line.label)) + 1);

    if colors_enabled {
        format!(
            "{}{}:{}{}{}{}{}",
            LABEL_COLOR, line.label, RESET, label_padding, VALUE_COLOR, line.value, RESET
        )
    } else {
        format!("{}:{}{}", line.label, label_padding, line.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{ArtModel, LayoutChoice, RendererChoice};
    use crate::engine::ArtModel as EngineModel;
    use crate::terminal::Terminal;
    use std::collections::BTreeMap;

    use crate::density::DensityMap;

    mod animation_audit;

    fn build_test_app(compact: bool, no_color: bool, colors_enabled: bool) -> App {
        App {
            args: Args {
                command: None,
                model: ArtModel::Random,
                width: None,
                height: None,
                seed: None,
                no_color,
                logo_only: false,
                info_only: false,
                animate: false,
                compact,
                disk_details: false,
                layout: LayoutChoice::Auto,
                renderer: RendererChoice::Auto,
                palette: crate::cli::PaletteChoice::Auto,
                no_update_check: false,
            },
            terminal: Terminal {
                is_tty: colors_enabled,
                colors_enabled,
            },
        }
    }

    fn build_test_app_pipeline(
        model: ArtModel,
        renderer: RendererChoice,
        seed: Option<u64>,
        no_color: bool,
        colors_enabled: bool,
    ) -> App {
        App {
            args: Args {
                command: None,
                model,
                width: None,
                height: None,
                seed,
                no_color,
                logo_only: false,
                info_only: false,
                animate: false,
                compact: false,
                disk_details: false,
                layout: LayoutChoice::Auto,
                renderer,
                palette: crate::cli::PaletteChoice::Auto,
                no_update_check: false,
            },
            terminal: Terminal {
                is_tty: colors_enabled,
                colors_enabled,
            },
        }
    }

    fn base_snapshot() -> SystemSnapshot {
        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), "Linux".to_string());
        fields.insert("Kernel".to_string(), "6.x".to_string());
        fields.insert("Uptime".to_string(), "1h 2m".to_string());
        fields.insert("Shell".to_string(), "bash".to_string());
        fields.insert("Disk".to_string(), "1G/2G (50%)".to_string());
        fields.insert("CPU".to_string(), "Test CPU".to_string());
        fields.insert("RAM".to_string(), "1.0GB / 2.0GB".to_string());

        SystemSnapshot {
            user_host: "astro@station".to_string(),
            fields,
        }
    }

    #[test]
    fn test_build_info_lines_full_field_ordering() {
        let app = build_test_app(false, true, false);
        let lines = app.build_info_lines(&base_snapshot());

        assert_eq!(lines[0], "astro@station");
        assert_eq!(
            lines[1..],
            [
                "OS:     Linux",
                "Kernel: 6.x",
                "Uptime: 1h 2m",
                "Shell:  bash",
                "Disk:   1G/2G (50%)",
                "CPU:    Test CPU",
                "RAM:    1.0GB / 2.0GB"
            ]
        );
        assert!(!lines.join("\n").contains('\x1b'));
    }

    #[test]
    fn test_build_info_lines_compact_field_ordering() {
        let app = build_test_app(true, true, false);
        let lines = app.build_info_lines(&base_snapshot());

        assert_eq!(
            lines,
            [
                "OS:     Linux",
                "Kernel: 6.x",
                "Uptime: 1h 2m",
                "Disk:   1G/2G (50%)",
                "CPU:    Test CPU",
                "RAM:    1.0GB / 2.0GB"
            ]
        );
    }

    #[test]
    fn test_build_info_lines_full_future_ordering_when_present() {
        let app = build_test_app(false, true, false);
        let mut snapshot = base_snapshot();
        snapshot
            .fields
            .insert("Packages".to_string(), "1234".to_string());
        snapshot
            .fields
            .insert("Resolution".to_string(), "1920x1080".to_string());
        snapshot
            .fields
            .insert("DE".to_string(), "GNOME".to_string());
        snapshot
            .fields
            .insert("WM".to_string(), "Mutter".to_string());
        snapshot
            .fields
            .insert("WM Theme".to_string(), "Adwaita".to_string());
        snapshot
            .fields
            .insert("GTK Theme".to_string(), "Adwaita".to_string());
        snapshot
            .fields
            .insert("Icon Theme".to_string(), "Adwaita".to_string());
        snapshot
            .fields
            .insert("Font".to_string(), "Noto Sans 11".to_string());
        snapshot
            .fields
            .insert("GPU".to_string(), "Test GPU".to_string());

        let lines = app.build_info_lines(&snapshot);

        assert_eq!(lines[0], "astro@station");
        assert_eq!(
            lines[1..],
            [
                "OS:         Linux",
                "Kernel:     6.x",
                "Uptime:     1h 2m",
                "Packages:   1234",
                "Shell:      bash",
                "Resolution: 1920x1080",
                "DE:         GNOME",
                "WM:         Mutter",
                "WM Theme:   Adwaita",
                "GTK Theme:  Adwaita",
                "Icon Theme: Adwaita",
                "Font:       Noto Sans 11",
                "Disk:       1G/2G (50%)",
                "CPU:        Test CPU",
                "GPU:        Test GPU",
                "RAM:        1.0GB / 2.0GB"
            ]
        );
    }

    #[test]
    fn test_build_info_lines_full_omits_missing_advanced_fields() {
        let app = build_test_app(false, true, false);
        let lines = app.build_info_lines(&base_snapshot());
        let joined = lines.join("\n");

        assert!(!joined.contains("Packages:"));
        assert!(!joined.contains("Resolution:"));
        assert!(!joined.contains("DE:"));
        assert!(!joined.contains("WM:"));
        assert!(!joined.contains("WM Theme:"));
        assert!(!joined.contains("GTK Theme:"));
        assert!(!joined.contains("Icon Theme:"));
        assert!(!joined.contains("Font:"));
        assert!(!joined.contains("GPU:"));
    }

    #[test]
    fn test_build_info_lines_compact_excludes_resolution_and_gpu_when_present() {
        let app = build_test_app(true, true, false);
        let mut snapshot = base_snapshot();
        snapshot
            .fields
            .insert("Resolution".to_string(), "1920x1080".to_string());
        snapshot
            .fields
            .insert("WM Theme".to_string(), "Adwaita".to_string());
        snapshot
            .fields
            .insert("GTK Theme".to_string(), "Yaru".to_string());
        snapshot
            .fields
            .insert("Icon Theme".to_string(), "Yaru".to_string());
        snapshot
            .fields
            .insert("Font".to_string(), "Cantarell 11".to_string());
        snapshot
            .fields
            .insert("GPU".to_string(), "Test GPU".to_string());

        let lines = app.build_info_lines(&snapshot);
        let joined = lines.join("\n");

        assert_eq!(
            lines,
            [
                "OS:     Linux",
                "Kernel: 6.x",
                "Uptime: 1h 2m",
                "Disk:   1G/2G (50%)",
                "CPU:    Test CPU",
                "RAM:    1.0GB / 2.0GB"
            ]
        );
        assert!(!joined.contains("Resolution:"));
        assert!(!joined.contains("WM Theme:"));
        assert!(!joined.contains("GTK Theme:"));
        assert!(!joined.contains("Icon Theme:"));
        assert!(!joined.contains("Font:"));
        assert!(!joined.contains("GPU:"));
    }

    #[test]
    fn test_build_info_lines_colorizes_header_labels_and_values() {
        let app = build_test_app(false, false, true);
        let lines = app.build_info_lines(&base_snapshot());

        assert_eq!(lines[0], "\x1b[93mastro@station\x1b[0m");
        assert_eq!(lines[1], "\x1b[94mOS:\x1b[0m     \x1b[97mLinux\x1b[0m");
        assert_eq!(lines[2], "\x1b[94mKernel:\x1b[0m \x1b[97m6.x\x1b[0m");
        assert!(lines.join("\n").contains("\x1b[0m"));
    }

    #[test]
    fn test_build_info_lines_no_color_overrides_colored_terminal() {
        let app = build_test_app(false, true, true);
        let lines = app.build_info_lines(&base_snapshot());

        assert_eq!(lines[0], "astro@station");
        assert_eq!(lines[1], "OS:     Linux");
        assert!(!lines.join("\n").contains('\x1b'));
    }

    #[test]
    fn test_compose_layout_keeps_info_ansi_from_affecting_art_padding() {
        let app = build_test_app(false, false, true);
        let info = app.build_info_lines(&base_snapshot());
        let art = vec!["**".to_string()];

        let result = crate::layout::compose_side_by_side(&art, &info, 6, 2);

        assert!(result[0].starts_with("**      \x1b[93mastro@station\x1b[0m"));
    }

    // ===== resolve_effective_renderer tests =====

    #[test]
    fn test_resolve_effective_renderer_galaxy_auto_halfblock() {
        for model in [
            EngineModel::Spiral,
            EngineModel::Elliptical,
            EngineModel::Cluster,
        ] {
            let result = App::resolve_effective_renderer(RendererChoice::Auto, model);
            assert_eq!(result.unwrap(), EffectiveRenderer::HalfBlock);
        }
    }

    #[test]
    fn test_resolve_effective_renderer_galaxy_halfblock() {
        for model in [
            EngineModel::Spiral,
            EngineModel::Elliptical,
            EngineModel::Cluster,
        ] {
            let result = App::resolve_effective_renderer(RendererChoice::HalfBlock, model);
            assert_eq!(result.unwrap(), EffectiveRenderer::HalfBlock);
        }
    }

    #[test]
    fn test_resolve_effective_renderer_galaxy_shade() {
        for model in [
            EngineModel::Spiral,
            EngineModel::Elliptical,
            EngineModel::Cluster,
        ] {
            let result = App::resolve_effective_renderer(RendererChoice::Shade, model);
            assert_eq!(result.unwrap(), EffectiveRenderer::Shade);
        }
    }

    #[test]
    fn test_resolve_effective_renderer_galaxy_ascii() {
        for model in [
            EngineModel::Spiral,
            EngineModel::Elliptical,
            EngineModel::Cluster,
        ] {
            let result = App::resolve_effective_renderer(RendererChoice::Ascii, model);
            assert_eq!(result.unwrap(), EffectiveRenderer::Ascii);
        }
    }

    #[test]
    fn test_resolve_effective_renderer_starfield_auto_starfield() {
        let result = App::resolve_effective_renderer(RendererChoice::Auto, EngineModel::Starfield);
        assert_eq!(result.unwrap(), EffectiveRenderer::Starfield);
    }

    #[test]
    fn test_resolve_effective_renderer_starfield_halfblock() {
        let result =
            App::resolve_effective_renderer(RendererChoice::HalfBlock, EngineModel::Starfield);
        assert_eq!(result.unwrap(), EffectiveRenderer::HalfBlock);
    }

    #[test]
    fn test_resolve_effective_renderer_starfield_shade() {
        let result = App::resolve_effective_renderer(RendererChoice::Shade, EngineModel::Starfield);
        assert_eq!(result.unwrap(), EffectiveRenderer::Shade);
    }

    #[test]
    fn test_resolve_effective_renderer_starfield_ascii() {
        let result = App::resolve_effective_renderer(RendererChoice::Ascii, EngineModel::Starfield);
        assert_eq!(result.unwrap(), EffectiveRenderer::Ascii);
    }

    #[test]
    fn test_resolve_effective_renderer_full_concrete_matrix() {
        let auto_defaults = [
            (EngineModel::Spiral, EffectiveRenderer::HalfBlock),
            (EngineModel::Elliptical, EffectiveRenderer::HalfBlock),
            (EngineModel::Cluster, EffectiveRenderer::HalfBlock),
            (EngineModel::Starfield, EffectiveRenderer::Starfield),
        ];
        for (model, auto_renderer) in auto_defaults {
            assert_eq!(
                App::resolve_effective_renderer(RendererChoice::Auto, model).unwrap(),
                auto_renderer,
                "Auto should keep the model-specific default for {model:?}"
            );
            assert_eq!(
                App::resolve_effective_renderer(RendererChoice::HalfBlock, model).unwrap(),
                EffectiveRenderer::HalfBlock,
                "HalfBlock should work for {model:?}"
            );
            assert_eq!(
                App::resolve_effective_renderer(RendererChoice::Shade, model).unwrap(),
                EffectiveRenderer::Shade,
                "Shade should work for {model:?}"
            );
            assert_eq!(
                App::resolve_effective_renderer(RendererChoice::Ascii, model).unwrap(),
                EffectiveRenderer::Ascii,
                "Ascii should work for {model:?}"
            );
        }
    }

    #[test]
    fn test_resolve_effective_renderer_random_error() {
        let result = App::resolve_effective_renderer(RendererChoice::Auto, EngineModel::Random);
        assert!(matches!(result, Err(AppError::Render(_))));
    }

    #[test]
    fn test_resolve_effective_renderer_spiral_quadrant() {
        let result = App::resolve_effective_renderer(RendererChoice::Quadrant, EngineModel::Spiral);
        assert_eq!(result.unwrap(), EffectiveRenderer::Quadrant);
    }

    #[test]
    fn test_resolve_effective_renderer_quadrant_unsupported_models_error() {
        for model in [
            EngineModel::Elliptical,
            EngineModel::Cluster,
            EngineModel::Starfield,
        ] {
            let result = App::resolve_effective_renderer(RendererChoice::Quadrant, model);
            match result {
                Err(AppError::Cli(msg)) => {
                    assert!(
                        msg.contains("quadrant"),
                        "error should mention quadrant: {msg}"
                    );
                    assert!(msg.contains("spiral"), "error should mention spiral: {msg}");
                }
                other => panic!("{model:?} + Quadrant must be a clear Cli error, got {other:?}"),
            }
        }
    }

    #[test]
    fn test_resolve_effective_renderer_auto_never_selects_quadrant() {
        for model in [
            EngineModel::Spiral,
            EngineModel::Elliptical,
            EngineModel::Cluster,
            EngineModel::Starfield,
        ] {
            let result = App::resolve_effective_renderer(RendererChoice::Auto, model).unwrap();
            assert_ne!(
                result,
                EffectiveRenderer::Quadrant,
                "Auto must never select Quadrant for {model:?}"
            );
        }
    }

    // ===== render_prepared_density tests =====

    #[test]
    fn test_render_prepared_density_accepts_all_effective_palettes_galaxy() {
        let canvas = vec![vec![0.5]];
        let prepared = PreparedDensity::Galaxy {
            density: DensityMap::from_rows(canvas).unwrap(),
            threshold: 0.1,
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);

        for palette in [
            ColorPalette::Nebula,
            ColorPalette::Cividis,
            ColorPalette::Amber,
            ColorPalette::Mono,
        ] {
            let result = App::render_prepared_density(
                prepared.clone(),
                EffectiveRenderer::HalfBlock,
                false,
                &terminal,
                palette,
            );
            assert!(
                result.is_ok(),
                "render_prepared_density should accept palette {:?}",
                palette
            );
        }
    }

    #[test]
    fn test_render_prepared_density_accepts_all_effective_palettes_starfield() {
        let canvas = vec![vec![0.20]];
        let prepared = PreparedDensity::Starfield {
            density: DensityMap::from_rows(canvas).unwrap(),
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);

        for palette in [
            ColorPalette::Nebula,
            ColorPalette::Cividis,
            ColorPalette::Amber,
            ColorPalette::Mono,
        ] {
            let result = App::render_prepared_density(
                prepared.clone(),
                EffectiveRenderer::Starfield,
                false,
                &terminal,
                palette,
            );
            assert!(
                result.is_ok(),
                "render_prepared_density should accept palette {:?}",
                palette
            );
        }
    }

    #[test]
    fn test_render_prepared_density_galaxy_halfblock_succeeds() {
        let canvas = vec![vec![0.5], vec![0.5]];
        let prepared = PreparedDensity::Galaxy {
            density: DensityMap::from_rows(canvas).unwrap(),
            threshold: 0.1,
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result = App::render_prepared_density(
            prepared,
            EffectiveRenderer::HalfBlock,
            false,
            &terminal,
            ColorPalette::Nebula,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_prepared_density_galaxy_shade_succeeds() {
        let canvas = vec![vec![0.5], vec![0.5]];
        let prepared = PreparedDensity::Galaxy {
            density: DensityMap::from_rows(canvas).unwrap(),
            threshold: 0.1,
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result = App::render_prepared_density(
            prepared,
            EffectiveRenderer::Shade,
            false,
            &terminal,
            ColorPalette::Nebula,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_prepared_density_galaxy_ascii_succeeds() {
        let canvas = vec![vec![0.5], vec![0.5]];
        let prepared = PreparedDensity::Galaxy {
            density: DensityMap::from_rows(canvas).unwrap(),
            threshold: 0.1,
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result = App::render_prepared_density(
            prepared,
            EffectiveRenderer::Ascii,
            false,
            &terminal,
            ColorPalette::Nebula,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_prepared_density_starfield_starfield_succeeds() {
        let canvas = vec![vec![0.0, 0.04, 0.10, 0.20]];
        let prepared = PreparedDensity::Starfield {
            density: DensityMap::from_rows(canvas).unwrap(),
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result = App::render_prepared_density(
            prepared,
            EffectiveRenderer::Starfield,
            false,
            &terminal,
            ColorPalette::Nebula,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_starfield_end_to_end_galaxy_renderers_fixed_seed() {
        let terminal = Terminal::with_colors(true, false);
        let scene = EngineModel::Starfield.generate_scene(20, 10, Some(42));

        let cases = [
            (EffectiveRenderer::HalfBlock, "▀▄█"),
            (EffectiveRenderer::Shade, "░▒▓█"),
            (EffectiveRenderer::Ascii, ".:-=+*#%@"),
        ];

        for (renderer, glyphs) in cases {
            let profile = RenderProfile::for_model_and_renderer(EngineModel::Starfield, renderer);
            let prepared = prepare_density(scene.density.clone(), profile);
            let lines = App::render_prepared_density(
                prepared,
                renderer,
                false,
                &terminal,
                ColorPalette::Nebula,
            )
            .unwrap_or_else(|err| panic!("starfield + {renderer:?} should render: {err}"));

            let mut saw_glyph = false;
            for line in &lines {
                for ch in line.chars() {
                    if ch.is_whitespace() {
                        continue;
                    }
                    assert!(
                        glyphs.contains(ch) || ".*+".contains(ch),
                        "unexpected glyph {ch:?} for {renderer:?}"
                    );
                    if glyphs.contains(ch) {
                        saw_glyph = true;
                    }
                }
            }
            assert!(saw_glyph, "{renderer:?} should produce visible structure");
        }
    }

    #[test]
    fn test_starfield_end_to_end_galaxy_renderers_deterministic() {
        let terminal = Terminal::with_colors(true, false);
        let renderers = [
            EffectiveRenderer::HalfBlock,
            EffectiveRenderer::Shade,
            EffectiveRenderer::Ascii,
        ];

        let mut previous: Vec<(EffectiveRenderer, Vec<String>)> = Vec::new();
        for _ in 0..2 {
            let scene = EngineModel::Starfield.generate_scene(20, 10, Some(42));
            for renderer in renderers {
                let profile =
                    RenderProfile::for_model_and_renderer(EngineModel::Starfield, renderer);
                let prepared = prepare_density(scene.density.clone(), profile);
                let lines = App::render_prepared_density(
                    prepared,
                    renderer,
                    false,
                    &terminal,
                    ColorPalette::Nebula,
                )
                .unwrap();
                match previous.iter_mut().find(|(r, _)| *r == renderer) {
                    Some((_, prev)) => {
                        assert_eq!(prev, &lines, "{renderer:?} should be deterministic")
                    }
                    None => previous.push((renderer, lines)),
                }
            }
        }
    }

    #[test]
    fn test_render_prepared_density_starfield_halfblock_error() {
        let canvas = vec![vec![0.5]];
        let prepared = PreparedDensity::Starfield {
            density: DensityMap::from_rows(canvas).unwrap(),
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result = App::render_prepared_density(
            prepared,
            EffectiveRenderer::HalfBlock,
            false,
            &terminal,
            ColorPalette::Nebula,
        );
        assert!(matches!(result, Err(AppError::Render(_))));
    }

    #[test]
    fn test_render_prepared_density_starfield_shade_error() {
        let canvas = vec![vec![0.5]];
        let prepared = PreparedDensity::Starfield {
            density: DensityMap::from_rows(canvas).unwrap(),
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result = App::render_prepared_density(
            prepared,
            EffectiveRenderer::Shade,
            false,
            &terminal,
            ColorPalette::Nebula,
        );
        assert!(matches!(result, Err(AppError::Render(_))));
    }

    #[test]
    fn test_render_prepared_density_galaxy_starfield_error() {
        let canvas = vec![vec![0.5], vec![0.5]];
        let prepared = PreparedDensity::Galaxy {
            density: DensityMap::from_rows(canvas).unwrap(),
            threshold: 0.1,
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result = App::render_prepared_density(
            prepared,
            EffectiveRenderer::Starfield,
            false,
            &terminal,
            ColorPalette::Nebula,
        );
        assert!(matches!(result, Err(AppError::Render(_))));
    }

    #[test]
    fn test_render_prepared_density_starfield_ascii_error() {
        let canvas = vec![vec![0.0, 0.04, 0.10, 0.20]];
        let prepared = PreparedDensity::Starfield {
            density: DensityMap::from_rows(canvas).unwrap(),
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result = App::render_prepared_density(
            prepared,
            EffectiveRenderer::Ascii,
            false,
            &terminal,
            ColorPalette::Nebula,
        );
        assert!(matches!(result, Err(AppError::Render(_))));
    }

    // ===== Phase 5B.3: Quadrant App integration =====

    #[test]
    fn test_render_art_random_quadrant_rejected_before_resolution() {
        // Seeds that resolve to different concrete models (frozen Random
        // table: 4 -> Spiral, 16 -> Cluster, 42 -> Starfield) must all fail
        // identically, proving the rejection happens before scene resolution.
        for seed in [Some(4_u64), Some(16), Some(42)] {
            let app = build_test_app_pipeline(
                ArtModel::Random,
                RendererChoice::Quadrant,
                seed,
                true,
                false,
            );
            let terminal = Terminal::with_colors(true, false);
            let result = app.render_art(&terminal, false, EngineModel::Random, 40, 20);
            match result {
                Err(AppError::Cli(msg)) => {
                    assert!(
                        msg.contains("random"),
                        "error should mention the random model: {msg}"
                    );
                }
                other => panic!(
                    "Random + Quadrant with seed {seed:?} must be a clear Cli error, got {other:?}"
                ),
            }
        }
    }

    #[test]
    fn test_render_art_random_existing_renderers_unchanged() {
        // Random with the existing renderers must still resolve and render.
        for renderer in [
            RendererChoice::Auto,
            RendererChoice::HalfBlock,
            RendererChoice::Shade,
            RendererChoice::Ascii,
        ] {
            let app = build_test_app_pipeline(ArtModel::Random, renderer, Some(42), true, false);
            let terminal = Terminal::with_colors(true, false);
            let lines = app
                .render_art(&terminal, false, EngineModel::Random, 40, 20)
                .unwrap_or_else(|err| panic!("Random + {renderer:?} should render: {err}"));
            assert_eq!(lines.len(), 20);
        }
    }

    #[test]
    fn test_render_art_spiral_quadrant_pipeline_dimensions() {
        let app = build_test_app_pipeline(
            ArtModel::Spiral,
            RendererChoice::Quadrant,
            Some(4),
            true,
            false,
        );
        let terminal = Terminal::with_colors(true, false);
        let lines = app
            .render_art(&terminal, false, EngineModel::Spiral, 40, 20)
            .unwrap();

        // Output is exactly W x H terminal cells.
        assert_eq!(lines.len(), 20);
        for line in &lines {
            assert_eq!(line.chars().count(), 40);
        }

        // Structural check: the pipeline must equal the pure renderer applied
        // to the 2W x 2H density prepared with QUADRANT occupancy.
        let resolved = EngineModel::Spiral.resolve_scene(Some(4));
        let density = EngineModel::Spiral.generate_density(
            &resolved,
            40,
            20,
            crate::render::topology::CellSamplingShape::QUADRANT,
        );
        assert_eq!((density.width, density.height), (80, 40));

        let profile =
            RenderProfile::for_model_and_renderer(EngineModel::Spiral, EffectiveRenderer::Quadrant);
        let prepared = prepare_density_with_shape(
            density,
            profile,
            crate::render::topology::CellSamplingShape::QUADRANT,
        );
        let PreparedDensity::Galaxy { density, threshold } = prepared else {
            panic!("Spiral + Quadrant must use galaxy density preparation");
        };
        let canvas = density.into_rows();
        assert_eq!(canvas.len(), 40);
        assert_eq!(canvas[0].len(), 80);
        assert_eq!(
            lines,
            render_quadrant_with_stars(&canvas, threshold, false, ColorPalette::Nebula)
        );
    }

    #[test]
    fn test_render_art_quadrant_with_colors_succeeds() {
        let app = build_test_app_pipeline(
            ArtModel::Spiral,
            RendererChoice::Quadrant,
            Some(4),
            false,
            true,
        );
        let terminal = Terminal::with_colors(true, true);
        let lines = app
            .render_art(&terminal, true, EngineModel::Spiral, 40, 20)
            .unwrap_or_else(|err| panic!("Quadrant with colors enabled should render: {err}"));
        assert_eq!(lines.len(), 20);
        // The colored path must actually emit foreground ANSI sequences.
        assert!(lines.join("\n").contains('\x1b'));
    }

    #[test]
    fn test_render_art_spiral_quadrant_colored_pipeline_matches_direct_renderer() {
        let app = build_test_app_pipeline(
            ArtModel::Spiral,
            RendererChoice::Quadrant,
            Some(4),
            false,
            true,
        );
        let terminal = Terminal::with_colors(true, true);
        let lines = app
            .render_art(&terminal, true, EngineModel::Spiral, 40, 20)
            .unwrap();

        // Structural check: the colored pipeline must equal the colored
        // renderer applied to the 2W x 2H density prepared with QUADRANT
        // occupancy.
        let resolved = EngineModel::Spiral.resolve_scene(Some(4));
        let density = EngineModel::Spiral.generate_density(
            &resolved,
            40,
            20,
            crate::render::topology::CellSamplingShape::QUADRANT,
        );
        let profile =
            RenderProfile::for_model_and_renderer(EngineModel::Spiral, EffectiveRenderer::Quadrant);
        let prepared = prepare_density_with_shape(
            density,
            profile,
            crate::render::topology::CellSamplingShape::QUADRANT,
        );
        let PreparedDensity::Galaxy { density, threshold } = prepared else {
            panic!("Spiral + Quadrant must use galaxy density preparation");
        };
        let canvas = density.into_rows();
        assert_eq!(
            lines,
            render_quadrant_with_stars(&canvas, threshold, true, ColorPalette::Nebula)
        );
    }

    #[test]
    fn test_render_art_quadrant_terminal_colors_disabled_uses_star_aware_renderer() {
        // App-level color state is enabled (no --no-color), but the terminal
        // reports colors disabled: the effective color state is false, so the
        // star-aware no-color renderer must be used (stars are uncolored).
        let app = build_test_app_pipeline(
            ArtModel::Spiral,
            RendererChoice::Quadrant,
            Some(4),
            false,
            true,
        );
        let terminal = Terminal::with_colors(true, false);
        let lines = app
            .render_art(&terminal, true, EngineModel::Spiral, 40, 20)
            .unwrap();

        let resolved = EngineModel::Spiral.resolve_scene(Some(4));
        let density = EngineModel::Spiral.generate_density(
            &resolved,
            40,
            20,
            crate::render::topology::CellSamplingShape::QUADRANT,
        );
        let profile =
            RenderProfile::for_model_and_renderer(EngineModel::Spiral, EffectiveRenderer::Quadrant);
        let prepared = prepare_density_with_shape(
            density,
            profile,
            crate::render::topology::CellSamplingShape::QUADRANT,
        );
        let PreparedDensity::Galaxy { density, threshold } = prepared else {
            panic!("Spiral + Quadrant must use galaxy density preparation");
        };
        let canvas = density.into_rows();
        assert_eq!(
            lines,
            render_quadrant_with_stars(&canvas, threshold, false, ColorPalette::Nebula)
        );
        assert!(!lines.join("\n").contains('\x1b'));
    }

    #[test]
    fn test_render_art_quadrant_no_color_succeeds() {
        let app = build_test_app_pipeline(
            ArtModel::Spiral,
            RendererChoice::Quadrant,
            Some(4),
            true,
            false,
        );
        let terminal = Terminal::with_colors(true, false);
        let lines = app
            .render_art(&terminal, false, EngineModel::Spiral, 40, 20)
            .unwrap();
        assert!(!lines.join("\n").contains('\x1b'));
    }

    /// Effective no-color application output: contains no ANSI and still
    /// includes the deterministic star overlay (matches the star-aware
    /// no-color renderer, which emits stars on visually-empty cells).
    #[test]
    fn test_render_art_quadrant_no_color_star_overlay() {
        let app = build_test_app_pipeline(
            ArtModel::Spiral,
            RendererChoice::Quadrant,
            Some(4),
            true,
            false,
        );
        let terminal = Terminal::with_colors(true, false);
        let lines = app
            .render_art(&terminal, false, EngineModel::Spiral, 40, 20)
            .unwrap();

        // No ANSI in the effective no-color output.
        assert!(
            !lines.join("\n").contains('\x1b'),
            "no-color output must be ANSI-free"
        );

        // Structural check: the pipeline equals the star-aware no-color
        // renderer applied to the QUADRANT-prepared density.
        let resolved = EngineModel::Spiral.resolve_scene(Some(4));
        let density = EngineModel::Spiral.generate_density(
            &resolved,
            40,
            20,
            crate::render::topology::CellSamplingShape::QUADRANT,
        );
        let profile =
            RenderProfile::for_model_and_renderer(EngineModel::Spiral, EffectiveRenderer::Quadrant);
        let prepared = prepare_density_with_shape(
            density,
            profile,
            crate::render::topology::CellSamplingShape::QUADRANT,
        );
        let PreparedDensity::Galaxy { density, threshold } = prepared else {
            panic!("Spiral + Quadrant must use galaxy density preparation");
        };
        let canvas = density.into_rows();
        let expected = render_quadrant_with_stars(&canvas, threshold, false, ColorPalette::Nebula);
        assert_eq!(lines, expected);

        // The deterministic star overlay must actually be present for this seed.
        let joined = lines.join("\n");
        assert!(
            joined.contains('.') || joined.contains('*') || joined.contains('+'),
            "expected the deterministic star overlay in the no-color output"
        );
    }

    #[test]
    fn test_render_art_existing_renderers_unaffected_by_color_gate() {
        for renderer in [
            RendererChoice::Auto,
            RendererChoice::HalfBlock,
            RendererChoice::Shade,
            RendererChoice::Ascii,
        ] {
            let app = build_test_app_pipeline(ArtModel::Spiral, renderer, Some(4), false, true);
            let terminal = Terminal::with_colors(true, true);
            let lines = app
                .render_art(&terminal, true, EngineModel::Spiral, 40, 20)
                .unwrap_or_else(|err| {
                    panic!("Spiral + {renderer:?} with colors should render: {err}")
                });
            assert_eq!(lines.len(), 20);
        }
    }

    #[test]
    fn test_render_art_spiral_quadrant_deterministic() {
        let terminal = Terminal::with_colors(true, false);
        let app = build_test_app_pipeline(
            ArtModel::Spiral,
            RendererChoice::Quadrant,
            Some(42),
            true,
            false,
        );
        let first = app
            .render_art(&terminal, false, EngineModel::Spiral, 40, 20)
            .unwrap();
        let second = app
            .render_art(&terminal, false, EngineModel::Spiral, 40, 20)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.join("\n").as_bytes(), second.join("\n").as_bytes());
    }

    #[test]
    fn test_render_prepared_density_galaxy_quadrant_succeeds() {
        let canvas = vec![vec![1.0, 1.0, 1.0, 0.0], vec![0.0, 0.0, 1.0, 0.0]];
        let prepared = PreparedDensity::Galaxy {
            density: DensityMap::from_rows(canvas).unwrap(),
            threshold: 0.1,
        };
        let terminal = Terminal::with_colors(true, false);
        let result = App::render_prepared_density(
            prepared,
            EffectiveRenderer::Quadrant,
            false,
            &terminal,
            ColorPalette::Nebula,
        );
        // Cell 0: TL+TR visible -> 0b0011 '▀'; cell 1: TL+BL visible -> 0b0101 '▌'
        assert_eq!(result.unwrap(), vec!["▀▌"]);
    }

    #[test]
    fn test_render_prepared_density_starfield_quadrant_error() {
        let canvas = vec![vec![0.0, 0.04, 0.10, 0.20]];
        let prepared = PreparedDensity::Starfield {
            density: DensityMap::from_rows(canvas).unwrap(),
        };
        let terminal = Terminal::with_colors(true, false);
        let result = App::render_prepared_density(
            prepared,
            EffectiveRenderer::Quadrant,
            false,
            &terminal,
            ColorPalette::Nebula,
        );
        assert!(matches!(result, Err(AppError::Render(_))));
    }

    // ===== resolve_color_palette tests =====

    #[test]
    fn test_resolve_color_palette_auto_to_nebula() {
        let palette = resolve_color_palette(crate::cli::PaletteChoice::Auto);
        assert_eq!(palette, ColorPalette::Nebula);
    }

    #[test]
    fn test_resolve_color_palette_nebula_to_nebula() {
        let palette = resolve_color_palette(crate::cli::PaletteChoice::Nebula);
        assert_eq!(palette, ColorPalette::Nebula);
    }

    #[test]
    fn test_resolve_color_palette_cividis_to_cividis() {
        let palette = resolve_color_palette(crate::cli::PaletteChoice::Cividis);
        assert_eq!(palette, ColorPalette::Cividis);
    }

    #[test]
    fn test_resolve_color_palette_amber_to_amber() {
        let palette = resolve_color_palette(crate::cli::PaletteChoice::Amber);
        assert_eq!(palette, ColorPalette::Amber);
    }

    #[test]
    fn test_resolve_color_palette_mono_to_mono() {
        let palette = resolve_color_palette(crate::cli::PaletteChoice::Mono);
        assert_eq!(palette, ColorPalette::Mono);
    }
    #[test]
    fn test_collection_profile_mapping_full() {
        let app = build_test_app(false, true, false);
        assert_eq!(app.collection_profile(), CollectionProfile::Full);
    }

    #[test]
    fn test_collection_profile_mapping_compact() {
        let app = build_test_app(true, true, false);
        assert_eq!(app.collection_profile(), CollectionProfile::Compact);
    }

    #[test]
    fn test_animation_frames_have_static_endpoints_and_stable_geometry() {
        let cases = [
            (ArtModel::Starfield, RendererChoice::Auto),
            (ArtModel::Spiral, RendererChoice::HalfBlock),
            (ArtModel::Spiral, RendererChoice::Shade),
            (ArtModel::Spiral, RendererChoice::Ascii),
            (ArtModel::Spiral, RendererChoice::Quadrant),
        ];

        for (model, renderer) in cases {
            let engine_model = match &model {
                ArtModel::Random => EngineModel::Random,
                ArtModel::Elliptical => EngineModel::Elliptical,
                ArtModel::Spiral => EngineModel::Spiral,
                ArtModel::Cluster => EngineModel::Cluster,
                ArtModel::Starfield => EngineModel::Starfield,
            };
            let app = build_test_app_pipeline(model.clone(), renderer, Some(42), true, false);
            let terminal = Terminal::with_colors(true, false);
            let prepared = app.prepare_art(false, engine_model, 40, 20).unwrap();
            let static_frame = app
                .render_art(&terminal, false, engine_model, 40, 20)
                .unwrap();
            let frames = app.render_animation_frames(&terminal, &prepared).unwrap();

            assert_eq!(
                frames.len(),
                crate::animation::intro_schedule().frame_count as usize
            );
            assert_eq!(frames.first(), Some(&static_frame));
            assert_eq!(frames.last(), Some(&static_frame));

            let line_count = static_frame.len();
            let widths: Vec<usize> = static_frame
                .iter()
                .map(|line| visible_width(line))
                .collect();
            for frame in &frames {
                assert_eq!(frame.len(), line_count);
                assert_eq!(
                    frame
                        .iter()
                        .map(|line| visible_width(line))
                        .collect::<Vec<_>>(),
                    widths
                );
            }
        }
    }

    #[test]
    fn test_animation_frames_change_only_prepared_star_presentation() {
        let app = build_test_app_pipeline(
            ArtModel::Starfield,
            RendererChoice::Auto,
            Some(42),
            true,
            false,
        );
        let terminal = Terminal::with_colors(true, false);
        let prepared = app
            .prepare_art(false, EngineModel::Starfield, 40, 20)
            .unwrap();
        let frames = app.render_animation_frames(&terminal, &prepared).unwrap();

        let changed = frames[1..frames.len() - 1]
            .iter()
            .any(|frame| frame != &frames[0]);
        assert!(changed, "the dedicated starfield must visibly twinkle");

        let star_count = |frame: &[String]| {
            frame
                .iter()
                .flat_map(|line| line.chars())
                .filter(|ch| matches!(ch, '.' | '*' | '+'))
                .count()
        };
        let star_positions = |frame: &[String]| {
            frame
                .iter()
                .enumerate()
                .flat_map(|(y, line)| {
                    line.chars()
                        .enumerate()
                        .filter_map(move |(x, ch)| matches!(ch, '.' | '*' | '+').then_some((x, y)))
                })
                .collect::<Vec<_>>()
        };

        let static_count = star_count(&frames[0]);
        let static_positions = star_positions(&frames[0]);
        let mut spatially_changed = false;
        for frame in &frames[1..frames.len() - 1] {
            assert_eq!(star_count(frame), static_count);
            assert_eq!(frame.len(), frames[0].len());
            assert_eq!(
                frame
                    .iter()
                    .map(|line| visible_width(line))
                    .collect::<Vec<_>>(),
                frames[0]
                    .iter()
                    .map(|line| visible_width(line))
                    .collect::<Vec<_>>()
            );
            spatially_changed |= star_positions(frame) != static_positions;
        }
        assert!(
            spatially_changed,
            "the dedicated starfield must include spatial micro-motion"
        );
    }

    #[test]
    fn test_animation_frame_sequence_reuses_one_prepared_scene_and_density() {
        let app = build_test_app_pipeline(
            ArtModel::Spiral,
            RendererChoice::Ascii,
            Some(42),
            true,
            false,
        );
        let terminal = Terminal::with_colors(true, false);
        let prepared = app.prepare_art(false, EngineModel::Spiral, 40, 20).unwrap();
        let initial_seed = prepared.scene_seed;
        let initial_density = match &prepared.prepared_density {
            PreparedArtDensity::Starfield { canvas }
            | PreparedArtDensity::Galaxy { canvas, .. } => canvas.clone(),
        };

        let first_sequence = app.render_animation_frames(&terminal, &prepared).unwrap();
        let second_sequence = app.render_animation_frames(&terminal, &prepared).unwrap();

        assert_eq!(initial_seed, 42);
        assert_eq!(first_sequence, second_sequence);
        assert_eq!(prepared.scene_seed, initial_seed);
        let final_density = match &prepared.prepared_density {
            PreparedArtDensity::Starfield { canvas }
            | PreparedArtDensity::Galaxy { canvas, .. } => canvas,
        };
        assert_eq!(final_density, &initial_density);
    }

    #[test]
    fn test_combined_animation_keeps_system_lines_identical() {
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
            .prepare_art(false, EngineModel::Starfield, 40, 20)
            .unwrap();
        let art_frames = app.render_animation_frames(&terminal, &prepared).unwrap();
        let outputs: Vec<Vec<String>> = art_frames
            .iter()
            .map(|art| {
                compose_layout(
                    art,
                    &info_lines,
                    40,
                    crate::display_plan::LayoutKind::Stacked,
                )
            })
            .collect();

        let art_height = art_frames[0].len();
        for output in &outputs {
            assert_eq!(&output[art_height + 1..], info_lines.as_slice());
        }
    }

    // ===== A5: subtle spiral phase motion =====

    #[test]
    fn test_a5_spiral_animation_endpoints_static_and_intermediates_differ() {
        // Byte-level A5 contract on real prepared scenes (barred seed 16 and
        // unbarred+dusty seed 4): the endpoints are the static render, every
        // intermediate frame drifts, and the visible geometry (line count
        // and per-line visible width) never changes.
        for (model, renderer, seed) in [
            (ArtModel::Spiral, RendererChoice::HalfBlock, 16_u64),
            (ArtModel::Spiral, RendererChoice::Quadrant, 4_u64),
        ] {
            let app = build_test_app_pipeline(model, renderer, Some(seed), true, false);
            let terminal = Terminal::with_colors(true, false);
            let prepared = app.prepare_art(false, EngineModel::Spiral, 40, 20).unwrap();
            let static_frame = app
                .render_art(&terminal, false, EngineModel::Spiral, 40, 20)
                .unwrap();
            let frames = app.render_animation_frames(&terminal, &prepared).unwrap();

            assert_eq!(frames.first(), Some(&static_frame));
            assert_eq!(frames.last(), Some(&static_frame));
            for frame in &frames[1..frames.len() - 1] {
                assert_ne!(frame, &static_frame, "intermediate frame must drift");
            }
            let widths: Vec<usize> = static_frame
                .iter()
                .map(|line| visible_width(line))
                .collect();
            for frame in &frames {
                assert_eq!(frame.len(), static_frame.len());
                assert_eq!(
                    frame
                        .iter()
                        .map(|line| visible_width(line))
                        .collect::<Vec<_>>(),
                    widths
                );
            }
        }
    }

    #[test]
    fn test_a5_spiral_scene_morphology_is_frozen_for_the_scene_seed() {
        // Scene identity: the prepared Spiral animation state holds exactly
        // one morphology derivation, and it is the same immutable scene a
        // fresh derivation from the concrete seed produces (no per-frame
        // RNG re-roll). All morphology fields are individually pinned.
        let app = build_test_app_pipeline(
            ArtModel::Spiral,
            RendererChoice::HalfBlock,
            Some(16),
            true,
            false,
        );
        let prepared = app.prepare_art(false, EngineModel::Spiral, 40, 20).unwrap();
        let Some(spiral) = prepared.spiral_animation.as_ref() else {
            panic!("Spiral preparation must build the A5 animation state");
        };

        let fresh = PreparedSpiralScene::for_scene_seed(prepared.scene_seed);
        assert_eq!(
            spiral.scene, fresh,
            "frozen scene must match the seed derivation"
        );
        assert_eq!(
            spiral.scene.config, fresh.config,
            "spiral configuration must be identical"
        );
        assert_eq!(
            spiral.scene.bar, fresh.bar,
            "bar configuration must be identical"
        );
        assert_eq!(
            spiral.scene.dust, fresh.dust,
            "dust configuration must be identical"
        );
        assert_eq!(
            spiral.scene.noise_seed, fresh.noise_seed,
            "noise seed must be identical"
        );
    }

    #[test]
    fn test_a5_non_spiral_models_keep_the_legacy_frame_path() {
        // Invariant 4: non-Spiral models must not build A5 state, and every
        // frame must equal the legacy per-frame render of the prepared art
        // (the pre-A5 behavior, byte for byte).
        for (cli_model, engine_model) in [
            (ArtModel::Elliptical, EngineModel::Elliptical),
            (ArtModel::Cluster, EngineModel::Cluster),
            (ArtModel::Starfield, EngineModel::Starfield),
        ] {
            let app = build_test_app_pipeline(
                cli_model.clone(),
                RendererChoice::Auto,
                Some(42),
                true,
                false,
            );
            let terminal = Terminal::with_colors(true, false);
            let prepared = app.prepare_art(false, engine_model, 40, 20).unwrap();
            assert!(
                prepared.spiral_animation.is_none(),
                "non-Spiral models must not build A5 state"
            );
            let frames = app.render_animation_frames(&terminal, &prepared).unwrap();
            let schedule = crate::animation::intro_schedule();
            for (frame_index, frame) in frames.iter().enumerate() {
                let is_static_endpoint = frame_index == 0
                    || frame_index == schedule.frame_count.saturating_sub(1) as usize;
                let context = if is_static_endpoint {
                    None
                } else {
                    Some(StarTwinkleFrame {
                        scene_seed: prepared.scene_seed,
                        frame_index: frame_index as u32,
                        frame_count: schedule.frame_count,
                    })
                };
                let legacy = App::render_prepared_art(&prepared, &terminal, context)
                    .expect("legacy frame render");
                assert_eq!(
                    frame, &legacy,
                    "{cli_model:?} frame {frame_index} must be legacy"
                );
            }
        }
    }

    #[test]
    fn test_a5_degenerate_bounds_fall_back_to_static_density() {
        // Degenerate robust-bounds fallback: when the static Spiral
        // normalization bounds are unavailable (None), the A5 dispatch must
        // skip the pinned per-phase preparation and render every frame from
        // the prepared static density. The pinned path panics on missing
        // bounds, so a panic-free run proves the `bounds.is_some()` guard
        // held; per-frame equality with the legacy static-density render
        // proves the intermediate frames consume the static prepared
        // density; endpoint equality keeps the first/final/static contract
        // intact.
        let app = build_test_app_pipeline(
            ArtModel::Spiral,
            RendererChoice::HalfBlock,
            Some(16),
            true,
            false,
        );
        let terminal = Terminal::with_colors(true, false);
        let mut prepared = app.prepare_art(false, EngineModel::Spiral, 40, 20).unwrap();
        let Some(prep) = prepared.spiral_animation.as_mut() else {
            panic!("Spiral preparation must build the A5 animation state");
        };
        assert!(
            prep.bounds.is_some(),
            "sanity: a real Spiral scene must have usable static bounds"
        );
        prep.bounds = None; // simulate a degenerate static normalization

        let schedule = crate::animation::intro_schedule();
        let frames = app
            .render_animation_frames(&terminal, &prepared)
            .expect("degenerate-bounds frames must render without panic");
        assert_eq!(frames.len(), schedule.frame_count as usize);

        for (frame_index, frame) in frames.iter().enumerate() {
            let is_static_endpoint =
                frame_index == 0 || frame_index == schedule.frame_count.saturating_sub(1) as usize;
            let context = if is_static_endpoint {
                None
            } else {
                Some(StarTwinkleFrame {
                    scene_seed: prepared.scene_seed,
                    frame_index: frame_index as u32,
                    frame_count: schedule.frame_count,
                })
            };
            let legacy = App::render_prepared_art(&prepared, &terminal, context)
                .expect("legacy static-density frame render");
            assert_eq!(
                frame, &legacy,
                "frame {frame_index}: missing bounds must fall back to the static prepared density"
            );
        }

        let static_frame =
            App::render_prepared_art(&prepared, &terminal, None).expect("static frame render");
        assert_eq!(&frames[0], &static_frame, "first frame must stay static");
        assert_eq!(
            &frames[frames.len() - 1],
            &static_frame,
            "final frame must stay static"
        );
    }

    #[test]
    fn test_a5_spiral_frame_density_keeps_static_threshold_and_scale() {
        // The per-phase prepared density reuses the static frame's threshold
        // and the static frame's robust normalization bounds: only the
        // angular pattern varies, the presentation scale does not.
        let app = build_test_app_pipeline(
            ArtModel::Spiral,
            RendererChoice::HalfBlock,
            Some(16),
            true,
            false,
        );
        let prepared = app.prepare_art(false, EngineModel::Spiral, 40, 20).unwrap();
        let PreparedArtDensity::Galaxy {
            canvas: static_canvas,
            threshold,
        } = &prepared.prepared_density
        else {
            panic!("Spiral must prepare a galaxy density");
        };
        let spiral = prepared.spiral_animation.as_ref().unwrap();

        // Bounds captured from the same raw density the static preparation
        // consumed: re-deriving them from the engine output must agree.
        let scene = PreparedSpiralScene::for_scene_seed(prepared.scene_seed);
        let raw = scene.density_at(40, 20, prepared.sampling_shape, 0.0);
        let profile = RenderProfile::for_model_and_renderer(
            prepared.resolved_model,
            prepared.effective_renderer,
        );
        assert_eq!(
            spiral.bounds,
            robust_normalization_bounds(&raw, profile.normalization)
        );

        for frame in 1..5_u32 {
            let phase = spiral_animation_phase_rad(frame, 6);
            let frame_density = App::spiral_frame_density(&prepared, &spiral.scene, phase);
            let PreparedArtDensity::Galaxy {
                canvas,
                threshold: frame_threshold,
            } = &frame_density
            else {
                panic!("per-phase density must be a galaxy density");
            };
            assert_eq!(
                *frame_threshold, *threshold,
                "threshold must be static-pinned"
            );
            assert_eq!(canvas.len(), static_canvas.len());
            for (frame_row, static_row) in canvas.iter().zip(static_canvas.iter()) {
                assert_eq!(frame_row.len(), static_row.len());
            }
        }
    }

    #[test]
    fn test_a5_spiral_animation_is_deterministic_across_independent_preparations() {
        // Same seed + same frame => byte-identical rendered frames, even
        // when the whole preparation pipeline runs independently twice.
        let render_sequence = |seed: u64| -> Vec<Vec<String>> {
            let app = build_test_app_pipeline(
                ArtModel::Spiral,
                RendererChoice::Ascii,
                Some(seed),
                true,
                false,
            );
            let terminal = Terminal::with_colors(true, false);
            let prepared = app.prepare_art(false, EngineModel::Spiral, 40, 20).unwrap();
            app.render_animation_frames(&terminal, &prepared)
                .expect("frame sequence")
        };
        assert_eq!(render_sequence(4), render_sequence(4));
        assert_eq!(render_sequence(42), render_sequence(42));
    }
}
