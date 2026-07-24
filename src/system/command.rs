use std::{
    io::{self, Read},
    process::{Child, ExitStatus},
    sync::Mutex,
    thread::sleep,
    time::{Duration, Instant},
};

/// Poll interval for wait_with_timeout.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Outcome of a bounded wait on a child process.
#[cfg_attr(not(test), allow(dead_code))]
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
#[cfg_attr(not(test), allow(dead_code))]
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
/// Mutex global para proteger testes que mutam variáveis de ambiente.
/// Isso evita race conditions quando os testes rodam em paralelo.
#[allow(dead_code)]
pub(crate) static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// Executa um comando externo de forma segura e best-effort.
///
/// # Arguments
/// * `cmd` - Nome do comando (ex: "uname", "hostname")
/// * `args` - Argumentos do comando como fatias de strings
///
/// # Returns
/// * `Some(String)` - Comando executado com sucesso e stdout não vazio
/// * `None` - Comando falhou, saiu com código diferente de zero,
///   stdout é inválido UTF-8, ou stdout está vazio
///
/// # Limitações
/// * Não há timeout implementado ainda (TODO: adicionar timeout antes de usar
///   para comandos potencialmente lentos)
/// * Output limitado a 64KB para evitar strings muito grandes
///
/// # Exemplos
/// ```ignore
/// let os = run_command_best_effort("uname", &["-s"]);
/// let hostname = run_command_best_effort("hostname", &[]);
/// ```
#[allow(dead_code)]
pub fn run_command_best_effort(cmd: &str, args: &[&str]) -> Option<String> {
    run_command_best_effort_with_limit(cmd, args, 64 * 1024)
}

/// Executa um comando externo com limite de tamanho customizável.
/// Usado para comandos que podem gerar output grande (ex: listagem de pacotes).
///
/// # Arguments
/// * `cmd` - Nome do comando
/// * `args` - Argumentos do comando
/// * `max_output_size` - Tamanho máximo do output em bytes
///
/// # Returns
/// * `Some(String)` - Comando executado com sucesso e stdout não vazio
/// * `None` - Comando falhou, saiu com código diferente de zero,
///   stdout é inválido UTF-8, stdout está vazio, ou output foi truncado
#[allow(dead_code)]
pub(crate) fn run_command_best_effort_with_limit(
    cmd: &str,
    args: &[&str],
    max_output_size: usize,
) -> Option<String> {
    let mut command = std::process::Command::new(cmd);
    command.args(args);

    // Executa o comando e captura o output
    let output = command.output().ok()?;

    // Verifica se o comando saiu com código de sucesso (0)
    if !output.status.success() {
        return None;
    }

    // Converte stdout para String, retornando None se não for UTF-8 válido
    let stdout = String::from_utf8(output.stdout).ok()?;

    // Trimming da saída
    let trimmed = stdout.trim();

    // Retorna None se output estiver vazio após trim
    if trimmed.is_empty() {
        return None;
    }

    // Verifica se o output foi truncado (excedeu o limite)
    // Se o output original era maior que max_output_size, não podemos confiar no resultado
    if stdout.len() > max_output_size {
        return None;
    }

    // Limita o tamanho do output para evitar strings muito grandes
    if trimmed.len() > max_output_size {
        return Some(trimmed[..max_output_size].to_string());
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
#[cfg_attr(not(test), allow(dead_code))]
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
#[cfg_attr(not(test), allow(dead_code))]
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

    #[test]
    fn wait_with_timeout_fixture_process() {
        match std::env::var("ASTROFETCH_TEST_FIXTURE") {
            Ok(val) if val == "1" => {
                if let Ok(ms) = std::env::var("ASTROFETCH_TEST_SLEEP_MS") {
                    let ms: u64 = ms
                        .parse::<u64>()
                        .expect("ASTROFETCH_TEST_SLEEP_MS must be a valid u64");
                    if ms > 0 {
                        std::thread::sleep(Duration::from_millis(ms));
                    }
                }
                if let Ok(code) = std::env::var("ASTROFETCH_TEST_EXIT_CODE") {
                    let code: i32 = code
                        .parse::<i32>()
                        .expect("ASTROFETCH_TEST_EXIT_CODE must be a valid i32");
                    std::process::exit(code);
                }
            }
            _ => {}
        }
        // No-op when run as part of the normal test suite.
    }

    /// Fully qualified name of the fixture test.
    /// Discovered via: cargo test -- --list | rg 'wait_with_timeout_fixture_process'
    const FIXTURE_TEST_NAME: &str = "system::command::tests::wait_with_timeout_fixture_process";

    /// Spawn the libtest executable as a fixture child process.
    /// All stdio is set to null.
    fn spawn_fixture(env_vars: &[(&str, &str)]) -> Child {
        let executable =
            std::env::current_exe().expect("failed to locate current libtest executable");

        let mut cmd = Command::new(executable);
        cmd.env("ASTROFETCH_TEST_FIXTURE", "1")
            .arg("--exact")
            .arg(FIXTURE_TEST_NAME)
            .arg("--nocapture")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        // Remove inherited optional fixture variables so only explicit env_vars apply.
        cmd.env_remove("ASTROFETCH_TEST_SLEEP_MS")
            .env_remove("ASTROFETCH_TEST_EXIT_CODE");

        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        cmd.spawn().expect("failed to spawn fixture process")
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
}
