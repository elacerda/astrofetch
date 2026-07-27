use std::{
    io::{self, Read},
    process::{Child, ExitStatus, Stdio},
    sync::{mpsc, Mutex},
    thread::{self, sleep},
    time::{Duration, Instant},
};

/// Poll interval for wait_with_timeout.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Default timeout for command execution.
const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// Grace period for the stdout reader thread after child lifecycle.
const DEFAULT_READER_GRACE: Duration = Duration::from_millis(500);

/// Outcome of a bounded wait on a child process.
#[derive(Debug)]
enum WaitOutcome {
    /// The child exited before the deadline.
    Exited(ExitStatus),
    /// The deadline elapsed. The child was terminated (or had already exited)
    /// and reaped via wait(). The ExitStatus is the authoritative final status.
    #[allow(dead_code)]
    TimedOut(ExitStatus),
}

/// Wait for a spawned child process to exit, with a finite timeout.
///
/// Polls `child.try_wait()` at intervals until the child exits or the
/// deadline passes. On timeout, calls `child.kill()` followed by
/// `child.wait()` to terminate and reap the direct child.
///
/// # Arguments
/// * `child` - Mutable reference to the spawned child process.
/// * `timeout` - Maximum duration to wait. `Duration::ZERO` performs
///   a single poll: if the child has not yet exited, the timeout path
///   executes immediately.
///
/// # Returns
/// * `Ok(WaitOutcome::Exited(status))` — child exited before deadline.
/// * `Ok(WaitOutcome::TimedOut(status))` — deadline elapsed; `kill()` and
///   `wait()` were called. `status` is the final exit status from `wait()`.
///   The child may have exited naturally before `kill()` was attempted.
/// * `Err(e)` — I/O error from `try_wait()`, `kill()`, or `wait()`.
///   On error, the caller still owns `&mut Child` and is responsible
///   for any further recovery (e.g., calling `wait()` to reap).
///
/// # Notes
/// - Only the direct child is terminated. Descendants are not affected.
/// - `kill()` errors are propagated; `wait()` is NOT called after a
///   `kill()` error, because the child may still be running and
///   `wait()` would block indefinitely.
/// - The actual return time may exceed `timeout` due to scheduler delay,
///   process termination latency, and wait/reap latency. No strict
///   maximum overshoot is guaranteed by the standard library.
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> io::Result<WaitOutcome> {
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(WaitOutcome::Exited(status)),
            Ok(None) => {}
            Err(e) => return Err(e),
        }

        let elapsed = started_at.elapsed();
        if elapsed >= timeout {
            break;
        }

        let remaining = timeout.saturating_sub(elapsed);
        let sleep_for = remaining.min(POLL_INTERVAL);
        sleep(sleep_for);
    }

    // Timeout path: kill and reap.
    child.kill()?;
    let status = child.wait()?;
    Ok(WaitOutcome::TimedOut(status))
}
/// Global mutex protecting tests that mutate environment variables.
/// Prevents race conditions when tests run in parallel.
#[allow(dead_code)]
pub(crate) static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// Executes an external command safely in a best-effort manner.
///
/// # Arguments
/// * `cmd` - Command name (e.g., "uname", "hostname")
/// * `args` - Command arguments as string slices
///
/// # Returns
/// * `Some(String)` - Command succeeded and stdout is non-empty
/// * `None` - Spawn failure, timeout, non-zero exit, invalid UTF-8,
///   empty stdout, or output exceeded the limit
///
/// # Limitations
/// - The production timeout is currently 10 seconds.
/// - Only the direct child is terminated; descendants are not terminated.
/// - Stdout is retained up to 64 KiB; reading continues after overflow
///   to drain the pipe, but output exceeding the limit is rejected.
/// - Stderr is discarded.
/// - Stdin is closed (null) rather than inherited.
/// - If a descendant process retains the stdout writer, the reader thread
///   may be detached after the reader grace period expires.
/// - The external contract remains best-effort `Option<String>`.
///
/// # Examples
/// ```ignore
/// let os = run_command_best_effort("uname", &["-s"]);
/// let hostname = run_command_best_effort("hostname", &[]);
/// ```
#[allow(dead_code)]
pub fn run_command_best_effort(cmd: &str, args: &[&str]) -> Option<String> {
    run_command_best_effort_with_limit(cmd, args, 64 * 1024)
}

/// Executes an external command with a configurable output size limit.
/// Used for commands that may produce large output (e.g., package listings).
///
/// # Arguments
/// * `cmd` - Command name
/// * `args` - Command arguments
/// * `max_output_size` - Maximum output size in bytes
///
/// # Returns
/// * `Some(String)` - Command succeeded and stdout is non-empty
/// * `None` - Spawn failure, timeout, non-zero exit, invalid UTF-8,
///   empty stdout, or output exceeded the limit
///
/// # Limitations
/// - The production timeout is currently 10 seconds.
/// - Only the direct child is terminated; descendants are not terminated.
/// - Stdout is retained up to `max_output_size` bytes; reading continues
///   after overflow to drain the pipe, but output exceeding the limit is
///   rejected with `None`.
/// - Stderr is discarded.
/// - Stdin is closed (null) rather than inherited.
/// - If a descendant process retains the stdout writer, the reader thread
///   may be detached after the reader grace period expires.
///
/// # Memory
/// Retained output content is at most `max_output_size` bytes.
/// `read_bounded` uses one fixed 8 KiB buffer.
/// Vec capacity, reader-thread stack, channel state, thread handle, and OS
/// pipe resources are additional implementation-dependent overheads.
/// Retained memory does not grow with total drained output.
#[allow(dead_code)]
pub(crate) fn run_command_best_effort_with_limit(
    cmd: &str,
    args: &[&str],
    max_output_size: usize,
) -> Option<String> {
    let mut command = std::process::Command::new(cmd);
    command.args(args);

    run_prepared_command_best_effort(
        command,
        max_output_size,
        DEFAULT_COMMAND_TIMEOUT,
        DEFAULT_READER_GRACE,
    )
}

/// Best-effort cleanup for a potentially still-running child process.
///
/// Attempts to kill the child; if successful, reaps it via `wait()`.
/// Does nothing if `kill()` fails (the child may still be running).
fn cleanup_child(child: &mut Child) {
    if child.kill().is_ok() {
        let _ = child.wait();
    }
}

/// Executes a pre-configured [`Command`] with bounded stdout capture.
///
/// Spawns the command, drains stdout in a dedicated reader thread with a
/// hard byte limit, and waits for the child to exit within a timeout.
/// Returns trimmed stdout only on full success; `None` for any failure.
///
/// # Arguments
/// * `command` - Pre-configured `Command` to execute.
/// * `max_output_size` - Maximum bytes of stdout to retain.
/// * `timeout` - Maximum duration to wait for the child to exit.
/// * `reader_grace` - Additional time to wait for the reader thread
///   after the child lifecycle completes.
///
/// # Returns
/// * `Some(String)` — trimmed, non-empty stdout on successful exit.
/// * `None` — any failure (spawn, timeout, non-zero exit, overflow,
///   I/O error, invalid UTF-8, empty output, reader detachment).
fn run_prepared_command_best_effort(
    mut command: std::process::Command,
    max_output_size: usize,
    timeout: Duration,
    reader_grace: Duration,
) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = command.spawn().ok()?;

    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            cleanup_child(&mut child);
            return None;
        }
    };

    let (tx, rx) = mpsc::sync_channel(1);
    let reader_handle = match thread::Builder::new()
        .name("stdout-reader".into())
        .spawn(move || {
            let result = read_bounded(stdout, max_output_size);
            let _ = tx.send(result);
        }) {
        Ok(handle) => handle,
        Err(_) => {
            cleanup_child(&mut child);
            return None;
        }
    };

    let lifecycle_result = wait_with_timeout(&mut child, timeout);
    if lifecycle_result.is_err() {
        cleanup_child(&mut child);
    }

    let reader_result = match rx.recv_timeout(reader_grace) {
        Ok(result) => result,
        Err(_) => {
            drop(reader_handle);
            return None;
        }
    };

    drop(reader_handle);

    let bytes = match (lifecycle_result, reader_result) {
        (Ok(WaitOutcome::Exited(status)), Ok(BoundedRead::Complete(b))) if status.success() => b,
        _ => return None,
    };

    let output = String::from_utf8(bytes).ok()?;
    let trimmed = output.trim();

    if trimmed.is_empty() {
        return None;
    }

    Some(trimmed.to_string())
}

/// Result of a bounded read operation.
///
/// # Invariants
/// - `Complete(bytes)`: `bytes.len() <= max_bytes` was guaranteed at call time.
/// - `Exceeded`: the input stream produced more than `max_bytes` bytes;
///   bytes beyond the limit are not retained as output content.
/// - In both cases, `Vec` capacity and allocator overhead may exceed `output.len()`.
/// - Memory does not grow with the amount drained after overflow.
/// - This helper has no timeout and may block until EOF or error.
#[derive(Debug, PartialEq, Eq)]
enum BoundedRead {
    Complete(Vec<u8>),
    Exceeded,
}

/// Reads from any `Read` source with a hard byte limit.
///
/// Reads incrementally using a fixed 8 KiB buffer allocated once before the loop.
/// Detects overflow when the current read would exceed the remaining capacity.
/// After overflow, continues reading and discarding until EOF.
///
/// # Invariants
/// - `output.len() <= max_bytes` on `Complete`.
/// - Bytes beyond `max_bytes` are not retained as output content.
/// - `Vec` capacity and allocator overhead may exceed `output.len()`.
/// - Memory does not grow with the amount drained after overflow.
/// - This helper has no timeout and may block until EOF or error.
///
/// # Errors
/// - `ErrorKind::Interrupted` is retried transparently.
/// - All other I/O errors are propagated immediately.
/// - An I/O error after overflow returns `Err` rather than `Exceeded`.
fn read_bounded<R: Read>(reader: R, max_bytes: usize) -> io::Result<BoundedRead> {
    const BUF_SIZE: usize = 8 * 1024;

    let mut reader = reader;
    let mut buf = [0u8; BUF_SIZE];
    let mut output = Vec::with_capacity(max_bytes.min(BUF_SIZE));
    let mut exceeded = false;

    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if exceeded {
                    continue;
                }
                let remaining = max_bytes - output.len();
                let retained = remaining.min(n);
                output.extend_from_slice(&buf[..retained]);
                if retained < n {
                    exceeded = true;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }

    if exceeded {
        Ok(BoundedRead::Exceeded)
    } else {
        Ok(BoundedRead::Complete(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    // ── Fixture process for lifecycle tests ──

    /// Fixture process: invoked when the test executable runs itself as a child.
    /// Uses ASTROFETCH_TEST_* environment variables to control behavior.
    #[test]
    fn wait_with_timeout_fixture_process() {
        match std::env::var("ASTROFETCH_TEST_FIXTURE") {
            Ok(val) if val == "1" => {
                // Depth-0: spawn a real descendant, then exit immediately.
                if std::env::var("ASTROFETCH_TEST_HOLD_PIPE").is_ok()
                    && std::env::var("ASTROFETCH_TEST_HOLD_DEPTH").is_err()
                {
                    let hold_pipe = std::env::var("ASTROFETCH_TEST_HOLD_PIPE")
                        .expect("ASTROFETCH_TEST_HOLD_PIPE must be set")
                        .parse::<u64>()
                        .expect("ASTROFETCH_TEST_HOLD_PIPE must be a valid u64");

                    let mut child_cmd = fixture_command(&[
                        ("ASTROFETCH_TEST_HOLD_PIPE", &hold_pipe.to_string()),
                        ("ASTROFETCH_TEST_HOLD_DEPTH", "1"),
                    ]);
                    child_cmd.stdout(Stdio::inherit());
                    child_cmd
                        .spawn()
                        .expect("failed to spawn pipe-holding child");
                    std::process::exit(0);
                }

                // Depth-1: hold the pipe for the requested duration.
                if let Ok(depth) = std::env::var("ASTROFETCH_TEST_HOLD_DEPTH") {
                    if depth == "1" {
                        let hold_pipe = std::env::var("ASTROFETCH_TEST_HOLD_PIPE")
                            .expect("ASTROFETCH_TEST_HOLD_PIPE must be set")
                            .parse::<u64>()
                            .expect("ASTROFETCH_TEST_HOLD_PIPE must be a valid u64");
                        std::thread::sleep(Duration::from_millis(hold_pipe));
                        std::process::exit(0);
                    } else {
                        panic!("ASTROFETCH_TEST_HOLD_DEPTH has unexpected value: {depth:?}");
                    }
                }

                // Ordinary fixture path: write output, sleep, exit.
                use std::io::Write;
                let mut stdout = std::io::stdout();

                if let Ok(n) = std::env::var("ASTROFETCH_TEST_OUTPUT_ASCII") {
                    let count: usize = n
                        .parse::<usize>()
                        .expect("ASTROFETCH_TEST_OUTPUT_ASCII must be a valid usize");
                    let buf = vec![b'A'; count];
                    stdout.write_all(&buf).expect("fixture stdout write failed");
                }

                if let Ok(n) = std::env::var("ASTROFETCH_TEST_OUTPUT_RAW") {
                    let count: usize = n
                        .parse::<usize>()
                        .expect("ASTROFETCH_TEST_OUTPUT_RAW must be a valid usize");
                    let buf = vec![0xAAu8; count];
                    stdout.write_all(&buf).expect("fixture stdout write failed");
                }

                stdout.flush().expect("fixture stdout flush failed");

                if let Ok(ms) = std::env::var("ASTROFETCH_TEST_SLEEP_MS") {
                    let ms: u64 = ms
                        .parse::<u64>()
                        .expect("ASTROFETCH_TEST_SLEEP_MS must be a valid u64");
                    if ms > 0 {
                        std::thread::sleep(Duration::from_millis(ms));
                    }
                }

                let exit_code = if let Ok(code) = std::env::var("ASTROFETCH_TEST_EXIT_CODE") {
                    code.parse::<i32>()
                        .expect("ASTROFETCH_TEST_EXIT_CODE must be a valid i32")
                } else {
                    0
                };
                std::process::exit(exit_code);
            }
            _ => {}
        }
        // No-op when run as part of the normal test suite.
    }

    /// Fully qualified name of the fixture test.
    const FIXTURE_TEST_NAME: &str = "system::command::tests::wait_with_timeout_fixture_process";

    /// Build a Rust-only fixture command using the current test executable.
    fn fixture_command(env_vars: &[(&str, &str)]) -> Command {
        let mut cmd =
            Command::new(std::env::current_exe().expect("cannot determine test executable path"));
        cmd.env("ASTROFETCH_TEST_FIXTURE", "1")
            .arg("--exact")
            .arg(FIXTURE_TEST_NAME)
            .arg("--nocapture")
            .arg("--quiet")
            .stdin(Stdio::null())
            .stderr(Stdio::null());

        // Remove optional fixture variables before applying explicit values.
        cmd.env_remove("ASTROFETCH_TEST_OUTPUT_ASCII")
            .env_remove("ASTROFETCH_TEST_OUTPUT_RAW")
            .env_remove("ASTROFETCH_TEST_SLEEP_MS")
            .env_remove("ASTROFETCH_TEST_EXIT_CODE")
            .env_remove("ASTROFETCH_TEST_HOLD_PIPE")
            .env_remove("ASTROFETCH_TEST_HOLD_DEPTH");

        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        cmd
    }

    /// Spawn the fixture as a child process (stdout suppressed).
    fn spawn_fixture(env_vars: &[(&str, &str)]) -> Child {
        fixture_command(env_vars)
            .stdout(Stdio::null())
            .spawn()
            .expect("failed to spawn fixture process")
    }

    // ── Synthetic readers for read_bounded tests ──

    struct RepeatingReader {
        byte: u8,
        remaining: usize,
        max_chunk: usize,
    }

    impl Read for RepeatingReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            if self.remaining == 0 {
                return Ok(0);
            }
            let n = self.remaining.min(self.max_chunk).min(buf.len());
            for buf_byte in buf.iter_mut().take(n) {
                *buf_byte = self.byte;
            }
            self.remaining -= n;
            Ok(n)
        }
    }

    struct ShortReadReader {
        data: Vec<u8>,
        pos: usize,
    }

    impl Read for ShortReadReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let remaining = self.data.len() - self.pos;
            if remaining == 0 {
                return Ok(0);
            }
            let chunk = remaining.min(3);
            let to_copy = chunk.min(buf.len());
            buf[..to_copy].copy_from_slice(&self.data[self.pos..self.pos + to_copy]);
            self.pos += to_copy;
            Ok(to_copy)
        }
    }

    struct ErrorReader {
        error: io::ErrorKind,
    }

    impl Read for ErrorReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(self.error, "test error"))
        }
    }

    struct ErrorAfterReader {
        data: Vec<u8>,
        pos: usize,
        reads_left: usize,
        error: io::ErrorKind,
    }

    impl Read for ErrorAfterReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            if self.reads_left == 0 {
                return Err(io::Error::new(self.error, "test error"));
            }
            let remaining = self.data.len() - self.pos;
            if remaining == 0 {
                return Ok(0);
            }
            let to_copy = remaining.min(buf.len());
            buf[..to_copy].copy_from_slice(&self.data[self.pos..self.pos + to_copy]);
            self.pos += to_copy;
            self.reads_left -= 1;
            Ok(to_copy)
        }
    }

    struct InterruptedReader {
        data: Vec<u8>,
        pos: usize,
        interrupted: bool,
    }

    impl Read for InterruptedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            let remaining = self.data.len() - self.pos;
            if remaining == 0 {
                return Ok(0);
            }
            let to_copy = remaining.min(buf.len());
            buf[..to_copy].copy_from_slice(&self.data[self.pos..self.pos + to_copy]);
            self.pos += to_copy;
            Ok(to_copy)
        }
    }

    // ── read_bounded tests ──

    #[test]
    fn test_read_bounded_empty_zero_limit() {
        let reader = std::io::empty();
        let result = read_bounded(reader, 0).unwrap();
        assert_eq!(result, BoundedRead::Complete(Vec::new()));
    }

    #[test]
    fn test_read_bounded_empty_nonzero_limit() {
        let reader = std::io::empty();
        let result = read_bounded(reader, 1024).unwrap();
        assert_eq!(result, BoundedRead::Complete(Vec::new()));
    }

    #[test]
    fn test_read_bounded_empty_max_limit() {
        let reader = std::io::empty();
        let result = read_bounded(reader, usize::MAX).unwrap();
        assert_eq!(result, BoundedRead::Complete(Vec::new()));
    }

    #[test]
    fn test_read_bounded_below_limit() {
        let data = b"hello".to_vec();
        let reader = data.as_slice();
        let result = read_bounded(reader, 1024).unwrap();
        assert_eq!(result, BoundedRead::Complete(b"hello".to_vec()));
    }

    #[test]
    fn test_read_bounded_exact_limit() {
        let data = b"hello".to_vec();
        let reader = data.as_slice();
        let result = read_bounded(reader, 5).unwrap();
        assert_eq!(result, BoundedRead::Complete(b"hello".to_vec()));
    }

    #[test]
    fn test_read_bounded_one_byte_over() {
        let data = b"hello!".to_vec();
        let reader = data.as_slice();
        let result = read_bounded(reader, 5).unwrap();
        assert_eq!(result, BoundedRead::Exceeded);
    }

    #[test]
    fn test_read_bounded_nonempty_zero_limit() {
        let data = b"hello".to_vec();
        let reader = data.as_slice();
        let result = read_bounded(reader, 0).unwrap();
        assert_eq!(result, BoundedRead::Exceeded);
    }

    #[test]
    fn test_read_bounded_multichunk_overflow() {
        let reader = &mut RepeatingReader {
            byte: 0xAB,
            remaining: 20,
            max_chunk: 3,
        };
        let result = read_bounded(&mut *reader, 10).unwrap();
        assert_eq!(result, BoundedRead::Exceeded);
        assert_eq!(reader.remaining, 0);
    }

    #[test]
    fn test_read_bounded_large_exceeded_and_drained() {
        let reader = &mut RepeatingReader {
            byte: 0xFF,
            remaining: 1024 * 1024,
            max_chunk: 1024,
        };
        let result = read_bounded(&mut *reader, 1500).unwrap();
        assert_eq!(result, BoundedRead::Exceeded);
        assert_eq!(reader.remaining, 0);
    }

    #[test]
    fn test_read_bounded_immediate_error() {
        let reader = ErrorReader {
            error: io::ErrorKind::Other,
        };
        assert!(read_bounded(reader, 1024).is_err());
    }

    #[test]
    fn test_read_bounded_short_reads() {
        let data = b"hello".to_vec();
        let reader = ShortReadReader { data, pos: 0 };
        let result = read_bounded(reader, 1024).unwrap();
        assert_eq!(result, BoundedRead::Complete(b"hello".to_vec()));
    }

    #[test]
    fn test_read_bounded_six_bytes_limit_five_drained() {
        let data = b"abcdef".to_vec();
        let reader = data.as_slice();
        let result = read_bounded(reader, 5).unwrap();
        assert_eq!(result, BoundedRead::Exceeded);
    }

    #[test]
    fn test_read_bounded_error_after_overflow_wins() {
        let data = b"hello!".to_vec();
        let error = io::ErrorKind::Other;
        let reader = ErrorAfterReader {
            data,
            pos: 0,
            reads_left: 1,
            error,
        };
        assert!(read_bounded(reader, 5).is_err());
    }

    #[test]
    fn test_read_bounded_interrupted_retried() {
        let data = b"hello".to_vec();
        let reader = InterruptedReader {
            data,
            pos: 0,
            interrupted: false,
        };
        let result = read_bounded(reader, 1024).unwrap();
        assert_eq!(result, BoundedRead::Complete(b"hello".to_vec()));
    }
    #[test]
    fn test_run_command_best_effort_nonexistent_command() {
        // Comando inexistente deve retornar None
        let result = run_command_best_effort("nonexistent_command_xyz123", &[]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_run_command_best_effort_empty_output() {
        // Comando que produz output vazio deve retornar None
        // Usamos 'echo -n' para produzir output vazio
        #[cfg(target_os = "linux")]
        {
            let result = run_command_best_effort("echo", &["-n", ""]);
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_run_command_best_effort_simple_command() {
        // Comando simples que deve funcionar na maioria dos sistemas
        // Usamos 'true' que sai com código 0 e produz output vazio
        // Então usamos 'echo' que deve funcionar
        #[cfg(target_os = "linux")]
        {
            let result = run_command_best_effort("echo", &["hello"]);
            assert_eq!(result, Some("hello".to_string()));
        }
    }

    #[test]
    fn test_run_command_best_effort_trims_whitespace() {
        // Comando que produz output com whitespace deve ser trimado
        #[cfg(target_os = "linux")]
        {
            let result = run_command_best_effort("echo", &["  hello  "]);
            assert_eq!(result, Some("hello".to_string()));
        }
    }

    #[test]
    fn test_run_command_best_effort_non_zero_exit() {
        // Comando que sai com código diferente de zero deve retornar None
        #[cfg(target_os = "linux")]
        {
            let result = run_command_best_effort("sh", &["-c", "exit 1"]);
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_run_command_best_effort_with_args() {
        // Comando com múltiplos argumentos
        #[cfg(target_os = "linux")]
        {
            let result = run_command_best_effort("echo", &["a", "b", "c"]);
            assert_eq!(result, Some("a b c".to_string()));
        }
    }

    #[test]
    fn test_run_command_best_effort_output_size_limit() {
        // Testa que output muito grande é detectado como truncado e retorna None
        // Usamos printf para gerar output grande
        #[cfg(target_os = "linux")]
        {
            // Cria uma string grande (maior que 64KB)
            let large_output: String = "x".repeat(70 * 1024); // 70KB

            // Cria um script que imprime output grande
            let result = run_command_best_effort(
                "sh",
                &[
                    "-c",
                    &format!("printf '%{}s' {}", large_output.len(), large_output),
                ],
            );

            // O resultado deve ser None porque o output foi truncado (excedeu 64KB)
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_run_command_best_effort_with_limit_truncation_detection() {
        // Testa que output truncado é detectado e retorna None
        // Usamos printf para gerar output grande
        #[cfg(target_os = "linux")]
        {
            // Cria uma string grande (maior que 1KB)
            let large_output: String = "x".repeat(2 * 1024); // 2KB

            // Tenta com limite pequeno (512 bytes) - deve ser truncado e retornar None
            let result = run_command_best_effort_with_limit(
                "sh",
                &[
                    "-c",
                    &format!("printf '%{}s' {}", large_output.len(), large_output),
                ],
                512, // Limite pequeno para forçar truncamento
            );

            // O resultado deve ser None porque o output foi truncado
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_run_command_best_effort_with_limit_accepts_valid_output() {
        // Testa que output válido dentro do limite é aceito
        #[cfg(target_os = "linux")]
        {
            let small_output = "hello world";

            let result = run_command_best_effort_with_limit(
                "echo",
                &[small_output],
                64 * 1024, // Limite grande o suficiente
            );

            // O resultado deve ser Some("hello world")
            assert_eq!(result, Some(small_output.to_string()));
        }
    }

    // ── Lifecycle tests for wait_with_timeout ──

    #[test]
    fn test_wait_with_timeout_immediate_exit() {
        let mut child = spawn_fixture(&[]);
        let result = wait_with_timeout(&mut child, Duration::from_secs(5)).unwrap();
        match result {
            WaitOutcome::Exited(status) => assert!(status.success()),
            WaitOutcome::TimedOut(_) => panic!("expected Exited, got TimedOut"),
        }
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn test_wait_with_timeout_immediate_nonzero_exit() {
        let mut child = spawn_fixture(&[("ASTROFETCH_TEST_EXIT_CODE", "42")]);
        let result = wait_with_timeout(&mut child, Duration::from_secs(5)).unwrap();
        match result {
            WaitOutcome::Exited(status) => {
                assert_eq!(status.code(), Some(42));
                assert!(!status.success());
            }
            WaitOutcome::TimedOut(_) => panic!("expected Exited, got TimedOut"),
        }
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn test_wait_with_timeout_delayed_exit_before_timeout() {
        let mut child = spawn_fixture(&[("ASTROFETCH_TEST_SLEEP_MS", "100")]);
        let result = wait_with_timeout(&mut child, Duration::from_secs(5)).unwrap();
        match result {
            WaitOutcome::Exited(status) => assert!(status.success()),
            WaitOutcome::TimedOut(_) => panic!("expected Exited, got TimedOut"),
        }
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn test_wait_with_timeout_timeout_then_terminate() {
        let mut child = spawn_fixture(&[("ASTROFETCH_TEST_SLEEP_MS", "60000")]);
        let result = wait_with_timeout(&mut child, Duration::from_millis(200)).unwrap();
        match result {
            WaitOutcome::TimedOut(_) => {}
            WaitOutcome::Exited(_) => panic!("expected TimedOut, got Exited"),
        }
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn test_wait_with_timeout_zero_timeout_running_child() {
        let mut child = spawn_fixture(&[("ASTROFETCH_TEST_SLEEP_MS", "60000")]);
        let result = wait_with_timeout(&mut child, Duration::ZERO).unwrap();
        match result {
            WaitOutcome::TimedOut(_) => {}
            WaitOutcome::Exited(_) => panic!("expected TimedOut, got Exited"),
        }
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn test_wait_with_timeout_race_tolerance() {
        let mut child = spawn_fixture(&[("ASTROFETCH_TEST_SLEEP_MS", "50")]);
        let result = wait_with_timeout(&mut child, Duration::from_millis(50)).unwrap();
        match result {
            WaitOutcome::Exited(status) => assert!(status.success()),
            WaitOutcome::TimedOut(_) => {}
        }
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn test_wait_with_timeout_duration_max_no_overflow() {
        let mut child = spawn_fixture(&[]);
        let result = wait_with_timeout(&mut child, Duration::MAX).unwrap();
        match result {
            WaitOutcome::Exited(status) => assert!(status.success()),
            WaitOutcome::TimedOut(_) => panic!("expected Exited, got TimedOut"),
        }
        assert!(child.try_wait().unwrap().is_some());
    }
    // ── Integration tests for run_prepared_command_best_effort ──

    #[test]
    fn test_wrapper_success() {
        let cmd = fixture_command(&[("ASTROFETCH_TEST_OUTPUT_ASCII", "5")]);
        let result = run_prepared_command_best_effort(
            cmd,
            64 * 1024,
            Duration::from_secs(5),
            Duration::from_secs(2),
        );
        let output = result.expect("expected successful output");
        assert!(
            output.ends_with("AAAAA"),
            "output should end with 5 'A' chars, got: {output:?}"
        );
    }

    #[test]
    fn test_wrapper_under_limit() {
        let cmd = fixture_command(&[("ASTROFETCH_TEST_OUTPUT_ASCII", "5")]);
        let result = run_prepared_command_best_effort(
            cmd,
            100,
            Duration::from_secs(5),
            Duration::from_secs(2),
        );
        let output = result.expect("expected successful output");
        assert!(
            output.ends_with("AAAAA"),
            "output should end with 5 'A' chars, got: {output:?}"
        );
    }

    #[test]
    fn test_wrapper_overflow() {
        let cmd = fixture_command(&[("ASTROFETCH_TEST_OUTPUT_ASCII", "2048")]);
        let result = run_prepared_command_best_effort(
            cmd,
            1024,
            Duration::from_secs(5),
            Duration::from_secs(2),
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_wrapper_nonzero_exit() {
        let cmd = fixture_command(&[
            ("ASTROFETCH_TEST_OUTPUT_ASCII", "4"),
            ("ASTROFETCH_TEST_EXIT_CODE", "1"),
        ]);
        let result = run_prepared_command_best_effort(
            cmd,
            64 * 1024,
            Duration::from_secs(5),
            Duration::from_secs(2),
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_wrapper_timeout() {
        let cmd = fixture_command(&[("ASTROFETCH_TEST_SLEEP_MS", "60000")]);
        let result = run_prepared_command_best_effort(
            cmd,
            64 * 1024,
            Duration::from_millis(200),
            Duration::from_millis(100),
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_wrapper_output_then_delayed_exit() {
        let cmd = fixture_command(&[
            ("ASTROFETCH_TEST_OUTPUT_ASCII", "7"),
            ("ASTROFETCH_TEST_SLEEP_MS", "50"),
        ]);
        let result = run_prepared_command_best_effort(
            cmd,
            64 * 1024,
            Duration::from_secs(5),
            Duration::from_secs(2),
        );
        let output = result.expect("expected successful output");
        assert!(
            output.ends_with("AAAAAAA"),
            "output should end with 7 'A' chars, got: {output:?}"
        );
    }

    #[test]
    fn test_wrapper_invalid_utf8() {
        let cmd = fixture_command(&[("ASTROFETCH_TEST_OUTPUT_RAW", "3")]);
        let result = run_prepared_command_best_effort(
            cmd,
            64 * 1024,
            Duration::from_secs(5),
            Duration::from_secs(2),
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_wrapper_zero_limit() {
        let cmd = fixture_command(&[("ASTROFETCH_TEST_OUTPUT_ASCII", "1")]);
        let result = run_prepared_command_best_effort(
            cmd,
            0,
            Duration::from_secs(5),
            Duration::from_secs(2),
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_wrapper_descendant_pipe_hold() {
        let start = Instant::now();
        let cmd = fixture_command(&[("ASTROFETCH_TEST_HOLD_PIPE", "500")]);
        let result = run_prepared_command_best_effort(
            cmd,
            64 * 1024,
            Duration::from_secs(5),
            Duration::from_millis(100),
        );
        let elapsed = start.elapsed();
        assert_eq!(result, None);
        assert!(
            elapsed < Duration::from_secs(2),
            "elapsed {:?} exceeds 2s bound",
            elapsed
        );
    }
}
