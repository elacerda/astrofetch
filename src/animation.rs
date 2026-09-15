//! Minimal terminal frame runner for the opt-in `--animate` intro (Milestone A1).
//!
//! This module only presents already-rendered frames in place. It never
//! regenerates scene content, never clears the full terminal, never owns the
//! alternate screen or a scroll region, never enables raw mode, and runs the
//! frame loop on the calling thread (no background animation thread).
//!
//! Ctrl+C handling: `ctrlc` only allows installing one handler per process,
//! so the handler is installed once, when the intro starts, and then kept
//! for the rest of the process lifetime. An `active` atomic in
//! [`IntroState`] selects its behavior:
//! - while the intro is active, a Ctrl+C only sets the `interrupted` atomic
//!   (the callback never writes to the terminal) and the frame loop stops
//!   promptly; the normal execution path then restores the cursor and exits
//!   with conventional interrupted semantics (exit code 130);
//! - once the intro has ended, `active` is cleared (only after the cursor
//!   has been restored) and any further Ctrl+C makes the callback terminate
//!   the process with the same conventional code, so the handler never
//!   silently swallows later signals.

use crate::terminal::{validate_dimensions, visible_width, TerminalDimensions};
use std::fmt;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveUp, Show};
use crossterm::queue;
use crossterm::style::Print;
use crossterm::terminal::{Clear, ClearType};

/// Ticks used to poll the interrupt flag while sleeping.
const INTERRUPT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Conventional exit code for a process interrupted by Ctrl+C (128 + SIGINT).
const INTERRUPTED_EXIT_CODE: i32 = 130;

/// Fixed schedule for the intro: a short sequence of prepared frames.
///
/// The intro sleeps for `(frame_count - 1) * frame_interval`, which is
/// currently 5 * 120 ms = 600 ms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntroSchedule {
    /// Number of frames to draw.
    pub frame_count: u32,
    /// Pause between consecutive frames.
    pub frame_interval: Duration,
}

/// Returns the fixed intro schedule (6 frames, 120 ms interval).
pub fn intro_schedule() -> IntroSchedule {
    IntroSchedule {
        frame_count: 6,
        frame_interval: Duration::from_millis(120),
    }
}

/// Decides whether the `--animate` intro should run.
///
/// The animation is strictly opt-in and only runs when stdout is an
/// interactive terminal. Pipes and redirections always fall back to the
/// legacy single static frame.
pub fn should_animate(animate: bool, is_tty: bool) -> bool {
    animate && is_tty
}

/// Returns the current terminal size in columns and rows, or `None` when
/// the size cannot be determined (non-TTY stdout or a failed size query).
///
/// The dimensions pass through the same validation used by
/// `Terminal::dimensions` ([`validate_dimensions`]), so zero or implausible
/// sizes are reported as unknown.
pub fn query_terminal_size() -> Option<TerminalDimensions> {
    crossterm::terminal::size()
        .ok()
        .and_then(|(width, height)| validate_dimensions(width as usize, height as usize))
}

/// Pure predicate deciding whether `lines` fit `size` without wrapping or
/// vertical scrolling.
///
/// The check is deliberately conservative:
/// - a line whose visible width reaches the terminal width is rejected,
///   because a line touching the last column wraps onto a second physical
///   row;
/// - a frame whose line count reaches the terminal height is rejected,
///   because printing on the last row scrolls the viewport.
///
/// Line widths are measured with the ANSI-aware `visible_width` helper, so
/// escape sequences never count toward the visible width.
pub fn frame_fits(lines: &[String], size: TerminalDimensions) -> bool {
    lines.len() < size.height && lines.iter().all(|line| visible_width(line) < size.width)
}

/// Atomic state shared between the Ctrl+C handler thread and the frame
/// loop.
#[derive(Debug, Default)]
pub struct IntroState {
    /// True while the intro is running; the Ctrl+C callback only requests
    /// an interruption in this window.
    active: AtomicBool,
    /// Set by the Ctrl+C callback once an interruption is requested.
    interrupted: AtomicBool,
}

impl IntroState {
    /// Creates a new, inactive, non-interrupted state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables the interrupt-request behavior of the installed Ctrl+C
    /// callback.
    pub fn activate(&self) {
        self.active.store(true, Ordering::SeqCst);
    }

    /// Disables the interrupt-request behavior of the installed Ctrl+C
    /// callback.
    ///
    /// From now on a Ctrl+C makes the callback terminate the process with
    /// the conventional interrupted exit code, instead of being swallowed.
    /// This must only be called after the cursor visibility has been
    /// restored, so the signal state only becomes inactive after the
    /// terminal is back to a clean state.
    pub fn deactivate(&self) {
        self.active.store(false, Ordering::SeqCst);
    }

    /// Returns true while the intro is running and the Ctrl+C callback
    /// requests interruptions instead of terminating the process.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Returns true once Ctrl+C has been requested during the intro.
    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }

    /// Marks the intro as interrupted.
    ///
    /// This is the only operation performed by the Ctrl+C callback while
    /// the intro is active; it never performs I/O or terminal writes.
    pub fn mark_interrupted(&self) {
        self.interrupted.store(true, Ordering::SeqCst);
    }
}

/// Installs the process-wide Ctrl+C handler used by the intro.
///
/// The callback is signal-safe by construction: it only reads and stores
/// atomics (or, outside the intro window, terminates the process) and never
/// writes to the terminal.
///
/// `ctrlc` allows only one handler per process, so this is called at most
/// once, when the intro starts. After the intro ends, [`IntroState::
/// deactivate`] switches the callback to the default Ctrl+C behavior
/// (process termination with the conventional interrupted exit code), so
/// later Ctrl+C presses are never silently swallowed.
pub fn install_interrupt_handler(state: Arc<IntroState>) -> Result<(), ctrlc::Error> {
    ctrlc::set_handler(move || {
        if state.is_active() {
            state.mark_interrupted();
        } else {
            std::process::exit(INTERRUPTED_EXIT_CODE);
        }
    })
}

/// RAII guard that hides the terminal cursor on creation and restores
/// visibility on drop.
///
/// Guarantees the cursor is not left hidden when the frame loop exits
/// early, whether because of an error or a Ctrl+C interruption.
struct HideCursorGuard;

impl HideCursorGuard {
    /// Hides the cursor on the real stdout.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the hide sequence cannot be written; the
    /// cursor visibility is restored best-effort before returning.
    fn new() -> io::Result<Self> {
        let mut stdout = io::stdout();
        if let Err(err) = hide_cursor(&mut stdout) {
            let _ = show_cursor(&mut io::stdout());
            return Err(err);
        }
        Ok(Self)
    }
}

impl Drop for HideCursorGuard {
    fn drop(&mut self) {
        let _ = show_cursor(&mut io::stdout());
    }
}

/// Writes the terminal sequence that hides the cursor.
fn hide_cursor<W: io::Write>(w: &mut W) -> io::Result<()> {
    queue!(w, Hide)?;
    w.flush()
}

/// Writes the terminal sequence that restores cursor visibility.
fn show_cursor<W: io::Write>(w: &mut W) -> io::Result<()> {
    queue!(w, Show)?;
    w.flush()
}

/// Draws a single intro frame into `w`.
///
/// The first frame is written exactly like the legacy static output
/// (each line followed by a newline). Subsequent frames move the cursor
/// back to the top of the previously drawn frame and clear each line
/// before redrawing it, so the frame is replaced in place without ever
/// touching the rest of the terminal.
///
/// # Errors
///
/// Returns an I/O error if any terminal sequence cannot be written.
pub fn draw_frame<W: io::Write>(mut w: W, lines: &[String], first_frame: bool) -> io::Result<()> {
    if !first_frame && !lines.is_empty() {
        let rows: u16 = lines.len().try_into().unwrap_or(u16::MAX);
        queue!(w, MoveUp(rows))?;
    }
    for line in lines {
        if !first_frame {
            queue!(w, Clear(ClearType::CurrentLine))?;
        }
        queue!(w, Print(line.as_str()), Print("\n"))?;
    }
    Ok(())
}

/// Sleeps for up to `interval`, returning early as soon as `state` is
/// marked as interrupted.
pub fn interruptible_sleep(interval: Duration, state: &IntroState) {
    let deadline = Instant::now() + interval;
    while !state.is_interrupted() {
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        thread::sleep(INTERRUPT_POLL_INTERVAL.min(deadline - now));
    }
}

/// Outcome of an intro run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroOutcome {
    /// Every frame was drawn and the final frame is frozen on screen.
    Completed,
    /// Ctrl+C was requested; the loop stopped and the caller must exit
    /// with conventional interrupted semantics.
    Interrupted,
    /// The intro did not draw any frame because the frame does not safely
    /// fit the current terminal viewport (or its size is unknown). No
    /// terminal side effects were performed; the caller must fall back to
    /// the existing static output path.
    Skipped,
}

/// Errors that can occur while running the intro.
#[derive(Debug)]
pub enum IntroError {
    /// The Ctrl+C handler could not be installed. No terminal side effects
    /// have happened yet, so the caller can fall back to static output.
    HandlerInstall(ctrlc::Error),
    /// Terminal I/O failed while running the frame loop. The cursor has
    /// already been restored by the RAII guard.
    Io(io::Error),
}

impl fmt::Display for IntroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntroError::HandlerInstall(err) => {
                write!(f, "could not install the Ctrl+C handler: {err}")
            }
            IntroError::Io(err) => write!(f, "intro animation I/O failed: {err}"),
        }
    }
}

impl std::error::Error for IntroError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IntroError::HandlerInstall(err) => Some(err),
            IntroError::Io(err) => Some(err),
        }
    }
}

/// Runs the frame loop on the real stdout.
///
/// The cursor is hidden for the whole loop by a RAII guard, so it is always
/// restored when this function returns, however it returns.
fn run_frames(
    lines: &[String],
    state: &IntroState,
    initial_size: TerminalDimensions,
    size_probe: &dyn Fn() -> Option<TerminalDimensions>,
) -> io::Result<IntroOutcome> {
    let _cursor_guard = HideCursorGuard::new()?;
    let mut stdout = io::stdout();
    replay_frames(
        &mut stdout,
        lines,
        state,
        initial_size,
        intro_schedule(),
        size_probe,
    )
}

/// Runs a fixed sequence of prepared frames on the real stdout.
fn run_frame_sequence(
    frames: &[Vec<String>],
    state: &IntroState,
    initial_size: TerminalDimensions,
    size_probe: &dyn Fn() -> Option<TerminalDimensions>,
) -> io::Result<IntroOutcome> {
    let _cursor_guard = HideCursorGuard::new()?;
    let mut stdout = io::stdout();
    replay_frame_sequence(
        &mut stdout,
        frames,
        state,
        initial_size,
        intro_schedule(),
        size_probe,
    )
}

/// Runs the frame loop against an arbitrary writer.
///
/// This is the writer-agnostic core of [`run_frames`], separated from the
/// real stdout so the geometry gate can be unit-tested with an injected
/// size probe.
///
/// Immediately before every replay (every frame after the first), the
/// terminal size is re-queried through `size_probe`. If it differs from
/// `initial_size`, or cannot be queried at all, the loop stops immediately:
/// no further frame is drawn, no static frame is printed, and the
/// already-rendered frame is left to normal terminal reflow. A pending
/// Ctrl+C request still takes precedence and yields
/// [`IntroOutcome::Interrupted`].
fn replay_frames<W: io::Write>(
    w: W,
    lines: &[String],
    state: &IntroState,
    initial_size: TerminalDimensions,
    schedule: IntroSchedule,
    size_probe: &dyn Fn() -> Option<TerminalDimensions>,
) -> io::Result<IntroOutcome> {
    let frames = vec![lines.to_vec(); schedule.frame_count as usize];
    replay_frame_sequence(w, &frames, state, initial_size, schedule, size_probe)
}

/// Replays prepared frames while preserving the A1 geometry and interruption
/// behavior.
fn replay_frame_sequence<W: io::Write>(
    mut w: W,
    frames: &[Vec<String>],
    state: &IntroState,
    initial_size: TerminalDimensions,
    schedule: IntroSchedule,
    size_probe: &dyn Fn() -> Option<TerminalDimensions>,
) -> io::Result<IntroOutcome> {
    let frame_count = schedule.frame_count.min(frames.len() as u32) as usize;
    if frame_count == 0 {
        return Ok(IntroOutcome::Completed);
    }

    let mut frame = 0usize;
    loop {
        if state.is_interrupted() {
            break;
        }
        if frame > 0 && size_probe() != Some(initial_size) {
            // The terminal geometry changed (or became unknown) since the
            // intro started: stop before redrawing and let the terminal
            // reflow the already-rendered frame on its own.
            break;
        }
        draw_frame(&mut w, &frames[frame], frame == 0)?;
        w.flush()?;
        frame += 1;
        if state.is_interrupted() || frame == frame_count {
            break;
        }
        interruptible_sleep(schedule.frame_interval, state);
        if state.is_interrupted() {
            break;
        }
    }

    Ok(if state.is_interrupted() {
        IntroOutcome::Interrupted
    } else {
        IntroOutcome::Completed
    })
}

/// Runs the full `--animate` intro: checks the terminal geometry gate,
/// installs the Ctrl+C handler, replays `lines` in place, and returns the
/// terminal and signal state to a clean condition.
///
/// Ordering guarantees:
/// 1. The terminal size is queried and the frame-fit check runs before any
///    side effect. If the size is unknown or the frame does not fit, the
///    intro is skipped ([`IntroOutcome::Skipped`]) without writing
///    anything, installing the handler, or hiding the cursor.
/// 2. The Ctrl+C handler is installed before any terminal write happens.
/// 3. The cursor is restored (the RAII guard inside `run_frames` is
///    dropped) *before* the interrupt state is deactivated, so the signal
///    state only becomes inactive after the cursor is visible again.
/// 4. `deactivate` runs before returning, so a Ctrl+C received after the
///    intro terminates the process with the conventional interrupted exit
///    code instead of being swallowed.
///
/// # Errors
///
/// Returns an [`IntroError`] describing the first failure; on
/// [`IntroError::Io`] the cursor has already been restored.
pub fn run_intro(lines: &[String]) -> Result<IntroOutcome, IntroError> {
    // Geometry gate: the animation may only start when the whole frame
    // safely fits the current viewport without wrapping or scrolling.
    // Resizes during the intro are handled by the per-replay re-check in
    // `replay_frames`.
    let initial_size = match query_terminal_size() {
        Some(size) if frame_fits(lines, size) => size,
        _ => return Ok(IntroOutcome::Skipped),
    };

    let state = Arc::new(IntroState::new());
    install_interrupt_handler(state.clone()).map_err(IntroError::HandlerInstall)?;

    state.activate();
    let outcome = run_frames(lines, &state, initial_size, &query_terminal_size);
    // Cursor visibility has been restored at this point (the guard inside
    // run_frames has already been dropped). Only now may the signal state
    // be deactivated.
    state.deactivate();

    outcome.map_err(IntroError::Io)
}

/// Returns whether every prepared frame shares the same safe terminal geometry.
fn frame_sequence_fits(frames: &[Vec<String>], size: TerminalDimensions) -> bool {
    let Some(first) = frames.first() else {
        return false;
    };

    let first_widths: Vec<usize> = first.iter().map(|line| visible_width(line)).collect();
    frames.iter().all(|frame| {
        frame.len() == first.len()
            && frame
                .iter()
                .map(|line| visible_width(line))
                .eq(first_widths.iter().copied())
            && frame_fits(frame, size)
    })
}

/// Runs the opt-in intro over a fixed sequence of already-rendered frames.
///
/// The first frame is written as the legacy static payload and the final
/// frame is left frozen after the fixed schedule. The runner remains unaware
/// of scene generation and only presents the supplied strings.
pub fn run_intro_frames(frames: &[Vec<String>]) -> Result<IntroOutcome, IntroError> {
    let initial_size = match query_terminal_size() {
        Some(size) if frame_sequence_fits(frames, size) => size,
        _ => return Ok(IntroOutcome::Skipped),
    };

    let state = Arc::new(IntroState::new());
    install_interrupt_handler(state.clone()).map_err(IntroError::HandlerInstall)?;

    state.activate();
    let outcome = run_frame_sequence(frames, &state, initial_size, &query_terminal_size);
    // Cursor visibility is restored by the RAII guard before deactivation,
    // preserving the A1 Ctrl+C lifecycle contract.
    state.deactivate();

    outcome.map_err(IntroError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strips CSI escape sequences (`ESC [ ... final`) from `s`.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
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

    #[test]
    fn test_should_animate_matrix() {
        assert!(!should_animate(false, false));
        assert!(!should_animate(false, true));
        assert!(
            !should_animate(true, false),
            "non-TTY stdout must fall back to static output"
        );
        assert!(should_animate(true, true));
    }

    #[test]
    fn test_intro_schedule_is_short() {
        let schedule = intro_schedule();
        assert!(schedule.frame_count >= 2);
        assert!(schedule.frame_interval > Duration::ZERO);
        assert!(schedule.frame_interval <= Duration::from_millis(250));
        let total = schedule.frame_interval * (schedule.frame_count - 1);
        assert!(total >= Duration::from_millis(400));
        assert!(total <= Duration::from_millis(800));
    }

    #[test]
    fn test_intro_state_lifecycle() {
        let state = IntroState::new();
        assert!(!state.is_active());
        assert!(!state.is_interrupted());

        state.activate();
        assert!(state.is_active());
        assert!(!state.is_interrupted());

        state.mark_interrupted();
        assert!(state.is_interrupted());
        state.mark_interrupted();
        assert!(state.is_interrupted());

        state.deactivate();
        assert!(!state.is_active());
        assert!(state.is_interrupted());
    }

    #[test]
    fn test_draw_first_frame_matches_static_output_bytes() {
        let lines = vec!["line one".to_string(), "line two".to_string()];
        let mut buf: Vec<u8> = Vec::new();
        draw_frame(&mut buf, &lines, true).unwrap();
        // Byte-identical to the legacy Terminal::print_lines payload.
        assert_eq!(buf, b"line one\nline two\n");
    }

    #[test]
    fn test_draw_first_frame_preserves_ansi_colors() {
        let lines = vec![String::from("\x1b[93mhello\x1b[0m")];
        let mut buf: Vec<u8> = Vec::new();
        draw_frame(&mut buf, &lines, true).unwrap();
        assert_eq!(buf, b"\x1b[93mhello\x1b[0m\n");
    }

    #[test]
    fn test_draw_frame_replay_moves_cursor_and_clears_lines() {
        let lines = vec!["abc".to_string(), "def".to_string(), "ghi".to_string()];
        let mut buf: Vec<u8> = Vec::new();
        draw_frame(&mut buf, &lines, true).unwrap();
        draw_frame(&mut buf, &lines, false).unwrap();
        let out = String::from_utf8(buf).unwrap();

        assert!(out.contains("\x1b[3A"), "must move up by the frame height");
        assert_eq!(
            out.matches("\x1b[2K").count(),
            3,
            "must clear each line in place"
        );
        assert!(!out.contains("\x1b[2J"), "must never clear the full screen");
        assert!(
            !out.contains("\x1b[?1049"),
            "must not use the alternate screen"
        );
        assert!(
            !out.contains("\x1b[?47"),
            "must not use the alternate screen"
        );
        assert!(
            !out.contains("\x1b[?25"),
            "frame payload must not touch cursor visibility"
        );
    }

    #[test]
    fn test_replayed_frames_keep_identical_content() {
        let lines = vec!["aaa".to_string(), "bbb".to_string()];
        let schedule = intro_schedule();
        let mut buf: Vec<u8> = Vec::new();
        draw_frame(&mut buf, &lines, true).unwrap();
        for _ in 1..schedule.frame_count {
            draw_frame(&mut buf, &lines, false).unwrap();
        }
        let visible = strip_ansi(&String::from_utf8(buf).unwrap());
        assert_eq!(visible, "aaa\nbbb\n".repeat(schedule.frame_count as usize));
    }

    #[test]
    fn test_draw_frame_empty_lines_is_noop() {
        let mut buf: Vec<u8> = Vec::new();
        draw_frame(&mut buf, &[], true).unwrap();
        draw_frame(&mut buf, &[], false).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_hide_show_cursor_sequences() {
        let mut buf: Vec<u8> = Vec::new();
        hide_cursor(&mut buf).unwrap();
        assert_eq!(buf, b"\x1b[?25l");
        buf.clear();
        show_cursor(&mut buf).unwrap();
        assert_eq!(buf, b"\x1b[?25h");
    }

    #[test]
    fn test_interruptible_sleep_returns_immediately_when_interrupted() {
        let state = IntroState::new();
        state.mark_interrupted();
        let start = Instant::now();
        interruptible_sleep(Duration::from_millis(500), &state);
        assert!(start.elapsed() < Duration::from_millis(200));
    }

    #[test]
    fn test_interruptible_sleep_honors_interval() {
        let state = IntroState::new();
        let start = Instant::now();
        interruptible_sleep(Duration::from_millis(50), &state);
        assert!(start.elapsed() >= Duration::from_millis(40));
    }

    #[test]
    fn test_frame_fits_safely() {
        let lines = vec!["short".to_string(), "a bit longer line".to_string()];
        assert!(frame_fits(
            &lines,
            TerminalDimensions {
                width: 80,
                height: 24
            }
        ));
        // An empty frame vacuously fits any valid geometry.
        assert!(frame_fits(
            &[],
            TerminalDimensions {
                width: 1,
                height: 1
            }
        ));
    }

    #[test]
    fn test_frame_rejects_line_exactly_terminal_width() {
        // A line touching the last column wraps to a second physical row.
        let lines = vec!["0123456789".to_string()];
        assert!(!frame_fits(
            &lines,
            TerminalDimensions {
                width: 10,
                height: 24
            }
        ));
        // One extra column of room is enough.
        assert!(frame_fits(
            &lines,
            TerminalDimensions {
                width: 11,
                height: 24
            }
        ));
    }

    #[test]
    fn test_frame_rejects_line_wider_than_terminal() {
        let lines = vec!["0123456789x".to_string()];
        assert!(!frame_fits(
            &lines,
            TerminalDimensions {
                width: 10,
                height: 24
            }
        ));
    }

    #[test]
    fn test_frame_rejects_height_exactly_terminal_height() {
        // Printing on the last row scrolls the viewport.
        let lines = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert!(!frame_fits(
            &lines,
            TerminalDimensions {
                width: 80,
                height: 3
            }
        ));
        assert!(frame_fits(
            &lines,
            TerminalDimensions {
                width: 80,
                height: 4
            }
        ));
    }

    #[test]
    fn test_frame_fits_ignores_ansi_sequences() {
        // Visible width is 4 while the raw length is far larger: ANSI
        // sequences must not count toward the visible width.
        let line = "\x1b[1;31mabcd\x1b[0m";
        assert_eq!(visible_width(line), 4);
        assert!(frame_fits(
            &[line.to_string()],
            TerminalDimensions {
                width: 5,
                height: 24
            }
        ));
        assert!(!frame_fits(
            &[line.to_string()],
            TerminalDimensions {
                width: 4,
                height: 24
            }
        ));
    }

    #[test]
    fn test_geometry_change_stops_replay() {
        use std::cell::Cell;

        let lines = vec!["first".to_string(), "second".to_string()];
        let initial = TerminalDimensions {
            width: 80,
            height: 24,
        };
        // The probe reports the captured geometry and then a resized one,
        // simulating a terminal resize during the intro.
        let size = Cell::new(Some(initial));
        let schedule = IntroSchedule {
            frame_count: 4,
            frame_interval: Duration::from_millis(1),
        };
        let state = IntroState::new();
        let mut buf: Vec<u8> = Vec::new();

        let outcome = replay_frames(&mut buf, &lines, &state, initial, schedule, &|| {
            size.set(Some(TerminalDimensions {
                width: 40,
                height: 12,
            }));
            size.get()
        })
        .unwrap();

        assert_eq!(outcome, IntroOutcome::Completed);
        // Only the first frame was written: no replay (no MoveUp / 2K)
        // happened, so the terminal reflows the rendered frame itself.
        assert_eq!(buf, b"first\nsecond\n");
        assert!(
            !buf.contains(&0x1b),
            "no control sequences may follow a resize"
        );
    }

    #[test]
    fn test_unknown_size_stops_replay() {
        let lines = vec!["first".to_string(), "second".to_string()];
        let initial = TerminalDimensions {
            width: 80,
            height: 24,
        };
        let schedule = IntroSchedule {
            frame_count: 4,
            frame_interval: Duration::from_millis(1),
        };
        let state = IntroState::new();
        let mut buf: Vec<u8> = Vec::new();

        let outcome = replay_frames(&mut buf, &lines, &state, initial, schedule, &|| None).unwrap();

        assert_eq!(outcome, IntroOutcome::Completed);
        // Same contract as a resize: the first frame stays, nothing else
        // is drawn.
        assert_eq!(buf, b"first\nsecond\n");
    }

    #[test]
    fn test_stable_geometry_replays_all_frames() {
        let lines = vec!["aaa".to_string(), "bbb".to_string()];
        let initial = TerminalDimensions {
            width: 80,
            height: 24,
        };
        let schedule = IntroSchedule {
            frame_count: 3,
            frame_interval: Duration::from_millis(1),
        };
        let state = IntroState::new();
        let mut buf: Vec<u8> = Vec::new();

        let outcome = replay_frames(&mut buf, &lines, &state, initial, schedule, &|| {
            Some(initial)
        })
        .unwrap();

        assert_eq!(outcome, IntroOutcome::Completed);
        let out = String::from_utf8(buf).unwrap();
        // Two replays: each moves up 2 rows and clears both lines.
        assert_eq!(out.matches("\x1b[2A").count(), 2);
        assert_eq!(out.matches("\x1b[2K").count(), 4);
        assert!(!out.contains("\x1b[2J"));
    }
}
