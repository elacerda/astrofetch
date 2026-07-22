use std::sync::Mutex;

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

#[cfg(test)]
mod tests {
    use super::*;
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
}
