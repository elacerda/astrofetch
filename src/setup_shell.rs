use crate::cli::{SetupShellArgs, UninstallShellArgs};
use crate::error::AppError;
use clap::ValueEnum;
use std::env;
use std::fs;
use std::path::PathBuf;

const START_MARKER: &str = "# >>> astrofetch >>>";
const END_MARKER: &str = "# <<< astrofetch <<<";

/// Shells supported by the explicit startup integration command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SetupShell {
    /// Bash startup file (`~/.bashrc`).
    Bash,
    /// Zsh startup file (`~/.zshrc`).
    Zsh,
    /// Fish startup file (`~/.config/fish/config.fish`).
    Fish,
    /// PowerShell profile.
    Powershell,
}

impl SetupShell {
    fn display_name(self) -> &'static str {
        match self {
            SetupShell::Bash => "bash",
            SetupShell::Zsh => "zsh",
            SetupShell::Fish => "fish",
            SetupShell::Powershell => "powershell",
        }
    }
}

/// Result of applying the managed block to existing file content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedBlockAction {
    /// A new managed block was inserted.
    Inserted,
    /// An existing managed block was left untouched.
    AlreadyInstalled,
    /// An existing managed block was replaced.
    Replaced,
}

/// Updated content plus the action used to produce it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedBlockResult {
    /// File content after applying the requested operation.
    pub content: String,
    /// Whether the block was inserted, skipped, or replaced.
    pub action: ManagedBlockAction,
}

/// Result of removing startup integration from an existing file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallShellAction {
    /// At least one AstroFetch startup block was removed.
    Removed,
    /// No AstroFetch startup integration was found.
    NotInstalled,
}

/// Updated content plus the action used to produce it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallShellResult {
    /// File content after removing startup integration.
    pub content: String,
    /// Whether anything was removed.
    pub action: UninstallShellAction,
}

/// Runs the shell startup integration command.
pub fn run(args: &SetupShellArgs) -> Result<(), AppError> {
    let shell = resolve_shell(args.shell)?;
    let target_path = match &args.target_path {
        Some(path) => path.clone(),
        None => target_path(shell)?,
    };
    let block = shell_block(shell, args.compact);

    if args.dry_run {
        println!("Shell: {}", shell.display_name());
        println!("Target startup file: {}", target_path.display());
        println!("Managed block:");
        print!("{}", block);
        return Ok(());
    }

    let existing_content = match fs::read_to_string(&target_path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err.into()),
    };

    let result = insert_or_update_managed_block(&existing_content, &block, args.force)?;

    if result.action == ManagedBlockAction::AlreadyInstalled {
        println!("AstroFetch startup integration is already installed.");
        println!("Use --force to replace the managed block.");
        println!("Target startup file: {}", target_path.display());
        return Ok(());
    }

    if let Some(parent) = target_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(&target_path, result.content)?;

    match result.action {
        ManagedBlockAction::Inserted => {
            println!("AstroFetch startup integration installed.");
        }
        ManagedBlockAction::Replaced => {
            println!("AstroFetch startup integration updated.");
        }
        ManagedBlockAction::AlreadyInstalled => unreachable!("handled before write"),
    }
    println!("Target startup file: {}", target_path.display());
    println!("Open a new terminal or source the file to see AstroFetch on startup.");

    Ok(())
}

/// Runs the shell startup integration removal command.
pub fn uninstall(args: &UninstallShellArgs) -> Result<(), AppError> {
    let shell = resolve_shell(args.shell)?;
    let target_path = match &args.target_path {
        Some(path) => path.clone(),
        None => target_path(shell)?,
    };

    let existing_content = match fs::read_to_string(&target_path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err.into()),
    };

    let result = remove_startup_blocks(&existing_content)?;

    if args.dry_run {
        println!("Shell: {}", shell.display_name());
        println!("Target startup file: {}", target_path.display());
        match result.action {
            UninstallShellAction::Removed => {
                println!("AstroFetch startup integration would be removed.");
            }
            UninstallShellAction::NotInstalled => {
                println!("AstroFetch startup integration is not installed.");
            }
        }
        return Ok(());
    }

    if result.action == UninstallShellAction::NotInstalled {
        println!("AstroFetch startup integration is not installed.");
        println!("Target startup file: {}", target_path.display());
        return Ok(());
    }

    fs::write(&target_path, result.content)?;

    println!("AstroFetch startup integration removed.");
    println!("Target startup file: {}", target_path.display());

    Ok(())
}

/// Builds the managed startup block for a shell.
pub fn shell_block(shell: SetupShell, compact: bool) -> String {
    let command = if compact {
        "astrofetch --compact"
    } else {
        "astrofetch"
    };

    match shell {
        SetupShell::Bash => format!(
            "{START_MARKER}\nif [[ $- == *i* ]] && command -v astrofetch >/dev/null 2>&1; then\n    {command}\nfi\n{END_MARKER}\n"
        ),
        SetupShell::Zsh => format!(
            "{START_MARKER}\nif [[ -o interactive ]] && command -v astrofetch >/dev/null 2>&1; then\n    {command}\nfi\n{END_MARKER}\n"
        ),
        SetupShell::Fish => format!(
            "{START_MARKER}\nif status is-interactive; and command -q astrofetch\n    {command}\nend\n{END_MARKER}\n"
        ),
        SetupShell::Powershell => format!(
            "{START_MARKER}\nif ($Host.Name -eq \"ConsoleHost\" -and (Get-Command astrofetch -ErrorAction SilentlyContinue)) {{\n    {command}\n}}\n{END_MARKER}\n"
        ),
    }
}

/// Infers a Unix shell from a `SHELL` environment value.
#[cfg(any(unix, test))]
pub fn infer_unix_shell(shell_path: &str) -> Option<SetupShell> {
    let name = std::path::Path::new(shell_path).file_name()?.to_str()?;

    if name.ends_with("bash") {
        Some(SetupShell::Bash)
    } else if name.ends_with("zsh") {
        Some(SetupShell::Zsh)
    } else if name.ends_with("fish") {
        Some(SetupShell::Fish)
    } else {
        None
    }
}

/// Inserts or replaces the AstroFetch managed block in existing content.
pub fn insert_or_update_managed_block(
    existing_content: &str,
    block: &str,
    force: bool,
) -> Result<ManagedBlockResult, AppError> {
    let content_without_legacy = remove_legacy_startup_blocks(existing_content);
    let removed_legacy_blocks = content_without_legacy != existing_content;

    match find_managed_block(&content_without_legacy)? {
        Some((start, end))
            if force
                || removed_legacy_blocks
                || managed_block_body_is_empty(&content_without_legacy, start, end) =>
        {
            let mut content = String::new();
            content.push_str(&content_without_legacy[..start]);
            content.push_str(block);
            content.push_str(&content_without_legacy[end..]);
            Ok(ManagedBlockResult {
                content,
                action: ManagedBlockAction::Replaced,
            })
        }
        Some(_) => Ok(ManagedBlockResult {
            content: content_without_legacy,
            action: ManagedBlockAction::AlreadyInstalled,
        }),
        None => {
            let mut content = content_without_legacy;
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(block);
            Ok(ManagedBlockResult {
                content,
                action: ManagedBlockAction::Inserted,
            })
        }
    }
}

/// Removes managed and known legacy AstroFetch startup blocks from existing content.
pub fn remove_startup_blocks(existing_content: &str) -> Result<UninstallShellResult, AppError> {
    let mut content = existing_content.to_string();

    while let Some((start, end)) = find_managed_block(&content)? {
        content.replace_range(start..end, "");
    }

    content = remove_legacy_startup_blocks(&content);
    let action = if content == existing_content {
        UninstallShellAction::NotInstalled
    } else {
        UninstallShellAction::Removed
    };

    Ok(UninstallShellResult { content, action })
}

fn remove_legacy_startup_blocks(existing_content: &str) -> String {
    let mut content = existing_content.to_string();

    for legacy_block in legacy_startup_blocks() {
        content = content.replace(&legacy_block, "");
    }

    content
}

fn managed_block_body_is_empty(existing_content: &str, start: usize, end: usize) -> bool {
    let block = &existing_content[start..end];
    let Some(after_start) = block.strip_prefix(START_MARKER) else {
        return false;
    };

    let after_start = after_start.strip_prefix('\n').unwrap_or(after_start);
    let body = match after_start.find(END_MARKER) {
        Some(end_marker_start) => &after_start[..end_marker_start],
        None => after_start,
    };

    body.trim().is_empty()
}

fn legacy_startup_blocks() -> Vec<String> {
    let commands = ["astrofetch", "astrofetch --compact"];
    let mut blocks = Vec::new();

    for command in commands {
        blocks.push(format!(
            "if [[ $- == *i* ]] && command -v astrofetch >/dev/null 2>&1; then\n    {command}\nfi\n"
        ));
        blocks.push(format!(
            "if [[ -o interactive ]] && command -v astrofetch >/dev/null 2>&1; then\n    {command}\nfi\n"
        ));
        blocks.push(format!(
            "if status is-interactive; and command -q astrofetch\n    {command}\nend\n"
        ));
        blocks.push(format!(
            "if ($Host.Name -eq \"ConsoleHost\" -and (Get-Command astrofetch -ErrorAction SilentlyContinue)) {{\n    {command}\n}}\n"
        ));
    }

    blocks
}

fn resolve_shell(explicit_shell: Option<SetupShell>) -> Result<SetupShell, AppError> {
    if let Some(shell) = explicit_shell {
        return Ok(shell);
    }

    #[cfg(unix)]
    {
        if let Ok(shell_path) = env::var("SHELL") {
            if let Some(shell) = infer_unix_shell(&shell_path) {
                return Ok(shell);
            }
        }

        Err(AppError::Cli(
            "Could not infer your shell from SHELL. Please pass --shell bash, --shell zsh, or --shell fish.".to_string(),
        ))
    }

    #[cfg(windows)]
    {
        if powershell_profile_path().is_some() {
            return Ok(SetupShell::Powershell);
        }

        Err(AppError::Cli(
            "Could not determine a PowerShell profile path. Please pass --shell powershell or configure AstroFetch manually.".to_string(),
        ))
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(AppError::Cli(
            "Could not infer your shell. Please pass --shell explicitly.".to_string(),
        ))
    }
}

fn target_path(shell: SetupShell) -> Result<PathBuf, AppError> {
    match shell {
        SetupShell::Bash => home_path(".bashrc"),
        SetupShell::Zsh => home_path(".zshrc"),
        SetupShell::Fish => home_path(".config/fish/config.fish"),
        SetupShell::Powershell => powershell_profile_path().ok_or_else(|| {
            AppError::Cli(
                "Could not determine a PowerShell profile path. Configure AstroFetch manually or pass --target-path."
                    .to_string(),
            )
        }),
    }
}

fn home_path(relative_path: &str) -> Result<PathBuf, AppError> {
    let home = home_dir().ok_or_else(|| {
        AppError::Cli("Could not determine your home directory. Pass --target-path.".to_string())
    })?;

    Ok(home.join(relative_path))
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
}

fn powershell_profile_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .map(|home| home.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1"))
    }

    #[cfg(not(windows))]
    {
        home_dir().map(|home| home.join(".config/powershell/Microsoft.PowerShell_profile.ps1"))
    }
}

fn find_managed_block(existing_content: &str) -> Result<Option<(usize, usize)>, AppError> {
    let Some(start) = existing_content.find(START_MARKER) else {
        return Ok(None);
    };
    let search_after_start = start + START_MARKER.len();
    let Some(relative_end) = existing_content[search_after_start..].find(END_MARKER) else {
        return Err(AppError::Cli(
            "Found an AstroFetch start marker without a matching end marker. Please fix the file manually before running setup-shell."
                .to_string(),
        ));
    };
    let marker_end = search_after_start + relative_end + END_MARKER.len();
    let end = if existing_content[marker_end..].starts_with('\n') {
        marker_end + 1
    } else {
        marker_end
    };

    Ok(Some((start, end)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::SetupShellArgs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_generated_bash_block() {
        assert_eq!(
            shell_block(SetupShell::Bash, false),
            "# >>> astrofetch >>>\nif [[ $- == *i* ]] && command -v astrofetch >/dev/null 2>&1; then\n    astrofetch\nfi\n# <<< astrofetch <<<\n"
        );
    }

    #[test]
    fn test_generated_bash_compact_block() {
        assert_eq!(
            shell_block(SetupShell::Bash, true),
            "# >>> astrofetch >>>\nif [[ $- == *i* ]] && command -v astrofetch >/dev/null 2>&1; then\n    astrofetch --compact\nfi\n# <<< astrofetch <<<\n"
        );
    }

    #[test]
    fn test_generated_zsh_block() {
        assert_eq!(
            shell_block(SetupShell::Zsh, false),
            "# >>> astrofetch >>>\nif [[ -o interactive ]] && command -v astrofetch >/dev/null 2>&1; then\n    astrofetch\nfi\n# <<< astrofetch <<<\n"
        );
    }

    #[test]
    fn test_generated_fish_block() {
        assert_eq!(
            shell_block(SetupShell::Fish, false),
            "# >>> astrofetch >>>\nif status is-interactive; and command -q astrofetch\n    astrofetch\nend\n# <<< astrofetch <<<\n"
        );
    }

    #[test]
    fn test_generated_powershell_block() {
        assert_eq!(
            shell_block(SetupShell::Powershell, false),
            "# >>> astrofetch >>>\nif ($Host.Name -eq \"ConsoleHost\" -and (Get-Command astrofetch -ErrorAction SilentlyContinue)) {\n    astrofetch\n}\n# <<< astrofetch <<<\n"
        );
    }

    #[test]
    fn test_shell_inference_from_paths() {
        assert_eq!(infer_unix_shell("/bin/bash"), Some(SetupShell::Bash));
        assert_eq!(infer_unix_shell("/usr/bin/zsh"), Some(SetupShell::Zsh));
        assert_eq!(
            infer_unix_shell("/opt/homebrew/bin/fish"),
            Some(SetupShell::Fish)
        );
        assert_eq!(infer_unix_shell("/usr/bin/unknown"), None);
    }

    #[test]
    fn test_insert_block_into_empty_content() {
        let result = insert_or_update_managed_block("", "BLOCK\n", false).unwrap();

        assert_eq!(result.content, "BLOCK\n");
        assert_eq!(result.action, ManagedBlockAction::Inserted);
    }

    #[test]
    fn test_append_block_to_existing_content_with_correct_newlines() {
        let result =
            insert_or_update_managed_block("set -g fish_greeting ''\n", "BLOCK\n", false).unwrap();

        assert_eq!(result.content, "set -g fish_greeting ''\n\nBLOCK\n");
        assert_eq!(result.action, ManagedBlockAction::Inserted);
    }

    #[test]
    fn test_replace_legacy_bash_block_with_managed_block() {
        let existing = "before\nif [[ $- == *i* ]] && command -v astrofetch >/dev/null 2>&1; then\n    astrofetch\nfi\nafter\n";
        let result = insert_or_update_managed_block(existing, "BLOCK\n", false).unwrap();

        assert_eq!(result.content, "before\nafter\n\nBLOCK\n");
        assert_eq!(result.action, ManagedBlockAction::Inserted);
    }

    #[test]
    fn test_replace_managed_block_when_removing_legacy_startup_block() {
        let existing = "before\nif [[ $- == *i* ]] && command -v astrofetch >/dev/null 2>&1; then\n    astrofetch\nfi\n# >>> astrofetch >>>\nmanaged\n# <<< astrofetch <<<\nafter\n";
        let result = insert_or_update_managed_block(existing, "new\n", false).unwrap();

        assert_eq!(result.content, "before\nnew\nafter\n");
        assert_eq!(result.action, ManagedBlockAction::Replaced);
    }

    #[test]
    fn test_replace_legacy_compact_zsh_block_with_managed_block() {
        let existing = "if [[ -o interactive ]] && command -v astrofetch >/dev/null 2>&1; then\n    astrofetch --compact\nfi\n";
        let result = insert_or_update_managed_block(existing, "BLOCK\n", false).unwrap();

        assert_eq!(result.content, "BLOCK\n");
        assert_eq!(result.action, ManagedBlockAction::Inserted);
    }

    #[test]
    fn test_remove_managed_startup_block() {
        let existing = "before\n# >>> astrofetch >>>\nmanaged\n# <<< astrofetch <<<\nafter\n";
        let result = remove_startup_blocks(existing).unwrap();

        assert_eq!(result.content, "before\nafter\n");
        assert_eq!(result.action, UninstallShellAction::Removed);
    }

    #[test]
    fn test_remove_multiple_managed_startup_blocks() {
        let existing =
            "# >>> astrofetch >>>\none\n# <<< astrofetch <<<\nkeep\n# >>> astrofetch >>>\ntwo\n# <<< astrofetch <<<\n";
        let result = remove_startup_blocks(existing).unwrap();

        assert_eq!(result.content, "keep\n");
        assert_eq!(result.action, UninstallShellAction::Removed);
    }

    #[test]
    fn test_remove_legacy_bash_startup_block() {
        let existing = "before\nif [[ $- == *i* ]] && command -v astrofetch >/dev/null 2>&1; then\n    astrofetch\nfi\nafter\n";
        let result = remove_startup_blocks(existing).unwrap();

        assert_eq!(result.content, "before\nafter\n");
        assert_eq!(result.action, UninstallShellAction::Removed);
    }

    #[test]
    fn test_remove_startup_blocks_reports_not_installed() {
        let existing = "alias ll='ls -la'\n";
        let result = remove_startup_blocks(existing).unwrap();

        assert_eq!(result.content, existing);
        assert_eq!(result.action, UninstallShellAction::NotInstalled);
    }

    #[test]
    fn test_uninstall_target_path_override_removes_block() {
        let target_path = unique_temp_path();
        fs::write(&target_path, shell_block(SetupShell::Bash, false)).unwrap();

        let args = UninstallShellArgs {
            shell: Some(SetupShell::Bash),
            dry_run: false,
            target_path: Some(target_path.clone()),
        };

        uninstall(&args).unwrap();

        let content = fs::read_to_string(&target_path).unwrap();
        assert!(content.is_empty());

        fs::remove_file(target_path).unwrap();
    }

    #[test]
    fn test_do_not_duplicate_existing_managed_block_without_force() {
        let existing = "before\n# >>> astrofetch >>>\nold\n# <<< astrofetch <<<\nafter\n";
        let result = insert_or_update_managed_block(existing, "new\n", false).unwrap();

        assert_eq!(result.content, existing);
        assert_eq!(result.action, ManagedBlockAction::AlreadyInstalled);
    }

    #[test]
    fn test_replace_empty_managed_block_without_force() {
        let existing = "before\n# >>> astrofetch >>>\n# <<< astrofetch <<<\nafter\n";
        let result = insert_or_update_managed_block(existing, "new\n", false).unwrap();

        assert_eq!(result.content, "before\nnew\nafter\n");
        assert_eq!(result.action, ManagedBlockAction::Replaced);
    }

    #[test]
    fn test_replace_existing_managed_block_with_force() {
        let existing = "before\n# >>> astrofetch >>>\nold\n# <<< astrofetch <<<\nafter\n";
        let result = insert_or_update_managed_block(existing, "new\n", true).unwrap();

        assert_eq!(result.content, "before\nnew\nafter\n");
        assert_eq!(result.action, ManagedBlockAction::Replaced);
    }

    #[test]
    fn test_preserve_content_before_and_after_existing_managed_block() {
        let existing =
            "alpha\nbeta\n# >>> astrofetch >>>\nold\n# <<< astrofetch <<<\ngamma\ndelta\n";
        let result = insert_or_update_managed_block(existing, "new\n", true).unwrap();

        assert!(result.content.starts_with("alpha\nbeta\n"));
        assert!(result.content.ends_with("gamma\ndelta\n"));
        assert_eq!(result.content, "alpha\nbeta\nnew\ngamma\ndelta\n");
    }

    #[test]
    fn test_target_path_override_writes_only_requested_temp_file() {
        let target_path = unique_temp_path();
        let args = SetupShellArgs {
            shell: Some(SetupShell::Bash),
            compact: false,
            dry_run: false,
            force: false,
            target_path: Some(target_path.clone()),
        };

        run(&args).unwrap();

        let content = fs::read_to_string(&target_path).unwrap();
        assert_eq!(content, shell_block(SetupShell::Bash, false));

        fs::remove_file(target_path).unwrap();
    }

    fn unique_temp_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        env::temp_dir().join(format!(
            "astrofetch-setup-shell-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
