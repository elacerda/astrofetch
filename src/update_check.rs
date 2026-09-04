//! Passive update notifications for AstroFetch.
//!
//! GitHub Releases is the version authority. The check is performed by the
//! [`update_informer`] crate, throttled to at most once every 24 hours using a
//! persistent, platform-appropriate cache file.
//!
//! Design goals:
//! - Completely silent on any failure (network, API, cache, parsing, timeout).
//! - No output when the installed version is already current.
//! - Never runs during tests.
//! - No network or cache work at all when disabled.
//! - Never performs an automatic self-update; it only prints a notice.

use std::path::Path;
use std::time::Duration;

use is_terminal::IsTerminal;
use update_informer::Check;

/// Repository that hosts the authoritative GitHub Releases.
const REPO: &str = "elacerda/astrofetch";
/// Fallback shown when the installation method cannot be inferred.
const RELEASES_URL: &str = "https://github.com/elacerda/astrofetch/releases/latest";
/// Official installer command (installs in place into `~/.local/bin`).
const INSTALL_SCRIPT_CMD: &str =
    "curl -fsSL https://raw.githubusercontent.com/elacerda/astrofetch/main/install.sh | sh";
/// Environment variable that disables the update check when set to a truthy value.
pub const NO_UPDATE_CHECK_ENV: &str = "ASTROFETCH_NO_UPDATE_CHECK";
/// How often an actual network check is performed.
#[cfg_attr(test, allow(dead_code))]
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Short network timeout so shell startup is never noticeably slowed.
#[cfg_attr(test, allow(dead_code))]
const CHECK_TIMEOUT: Duration = Duration::from_millis(500);

/// How AstroFetch was installed, inferred from local evidence only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMethod {
    /// Installed via Homebrew.
    Homebrew,
    /// Installed via the official install script into `~/.local/bin`.
    InstallScript,
    /// The installation method could not be determined.
    Unknown,
}

/// Detects the installation method from the running executable path.
///
/// Detection is conservative and uses only local evidence (no network). When
/// the path does not match a known layout, [`InstallMethod::Unknown`] is
/// returned so the caller falls back to the release URL.
///
/// # Arguments
///
/// * `exe` - Path to the running executable (see [`std::env::current_exe`]).
pub fn detect_install_method(exe: &Path) -> InstallMethod {
    let path = exe.to_string_lossy();
    // Homebrew installs under a `Cellar` directory on macOS (Intel and
    // Apple Silicon) and on Linux (`/home/linuxbrew/.linuxbrew/Cellar`).
    if path.contains("/Cellar/") {
        return InstallMethod::Homebrew;
    }
    // The official install script defaults to `~/.local/bin`.
    if path.contains("/.local/bin/") {
        return InstallMethod::InstallScript;
    }
    InstallMethod::Unknown
}

/// Returns the update command for a detected installation method.
///
/// The returned command never creates a parallel installation:
/// - Homebrew installs are updated with `brew upgrade astrofetch`.
/// - Install-script installs are updated by re-running the official installer,
///   which installs in place into `~/.local/bin`.
/// - Unknown installs fall back to the release URL (no automatic install).
pub fn update_command(method: InstallMethod) -> &'static str {
    match method {
        InstallMethod::Homebrew => "brew upgrade astrofetch",
        InstallMethod::InstallScript => INSTALL_SCRIPT_CMD,
        InstallMethod::Unknown => RELEASES_URL,
    }
}

/// Formats the two-line update notice printed to stderr.
///
/// # Arguments
///
/// * `current` - Installed version, e.g. `0.4.0`.
/// * `latest` - Newer release version, e.g. `0.4.1`.
/// * `method` - Detected installation method (selects the update command).
pub fn format_notice(current: &str, latest: &str, method: InstallMethod) -> String {
    format!(
        "AstroFetch {latest} is available (current: {current})\nUpdate: {}",
        update_command(method)
    )
}

/// Returns `latest` when it is a valid SemVer strictly newer than `current`.
///
/// Returns `None` when `latest` is equal to, older than, or not parseable as
/// SemVer. This is what keeps the notice silent for same/older versions.
///
/// # Arguments
///
/// * `current` - Installed version, e.g. `0.4.0`.
/// * `latest` - Candidate release version, e.g. `0.4.1`.
pub fn newer_version(current: &str, latest: &str) -> Option<String> {
    let current = semver::Version::parse(current).ok()?;
    let latest = semver::Version::parse(latest).ok()?;
    (latest > current).then(|| latest.to_string())
}

/// Returns `true` when the update check is disabled.
///
/// The check is disabled by the `--no-update-check` flag or by the
/// [`NO_UPDATE_CHECK_ENV`] environment variable set to `1` or `true`
/// (case-insensitive).
pub fn is_disabled(no_update_check: bool, env_value: Option<&str>) -> bool {
    if no_update_check {
        return true;
    }
    match env_value {
        Some("1") => true,
        Some(v) => v.eq_ignore_ascii_case("true"),
        None => false,
    }
}

/// Decides whether the passive update check should run at all.
///
/// Returns `false` (skip, performing no network or cache work) when the check
/// is disabled (see [`is_disabled`]) or when stderr is not an interactive TTY.
///
/// # Arguments
///
/// * `no_update_check` - Value of the `--no-update-check` flag.
/// * `env_value` - Value of [`NO_UPDATE_CHECK_ENV`], if set.
/// * `stderr_is_tty` - Whether stderr is an interactive terminal.
pub fn should_check(no_update_check: bool, env_value: Option<&str>, stderr_is_tty: bool) -> bool {
    !is_disabled(no_update_check, env_value) && stderr_is_tty
}

/// Runs a version check via the provided informer and returns the latest
/// version string when it is a newer SemVer release.
///
/// All errors (network, API, cache, parsing, timeout) are swallowed so the
/// check is completely silent on failure.
fn check_latest<C: Check>(informer: C, current: &str) -> Option<String> {
    let latest_opt = informer.check_version().ok()?;
    let latest = latest_opt.as_ref()?;
    let latest_str = latest.semver().to_string();
    newer_version(current, &latest_str)
}

/// Performs the passive update check, printing a notice to stderr when a
/// newer release exists.
///
/// This is a no-op (no network, no cache work) when:
/// - disabled via `--no-update-check` or [`NO_UPDATE_CHECK_ENV`];
/// - stderr is not an interactive TTY;
/// - running under the test harness.
///
/// # Arguments
///
/// * `no_update_check` - Value of the `--no-update-check` flag.
pub fn maybe_check(no_update_check: bool) {
    let env_value = std::env::var(NO_UPDATE_CHECK_ENV).ok();
    let stderr_is_tty = std::io::stderr().is_terminal();
    if !should_check(no_update_check, env_value.as_deref(), stderr_is_tty) {
        return;
    }
    perform_check();
}

/// Performs the actual (network) version check and prints a notice to stderr
/// when a newer release exists.
///
/// The network work is compiled out during tests so the check never runs under
/// the test harness.
fn perform_check() {
    #[cfg(not(test))]
    {
        let current = env!("CARGO_PKG_VERSION");
        let informer = update_informer::new(update_informer::registry::GitHub, REPO, current)
            .interval(CHECK_INTERVAL)
            .timeout(CHECK_TIMEOUT);

        let Some(latest) = check_latest(informer, current) else {
            return;
        };

        let exe = std::env::current_exe().unwrap_or_default();
        let method = detect_install_method(&exe);
        eprintln!("{}", format_notice(current, &latest, method));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use update_informer::Version;

    /// A `Check` implementation that always fails, to simulate network or
    /// registry errors without touching the network.
    struct FailingInformer;

    impl Check for FailingInformer {
        fn check_version(self) -> update_informer::Result<Option<Version>> {
            Err("simulated network failure".into())
        }
    }

    fn fake(new_version: &'static str) -> update_informer::FakeUpdateInformer<&'static str> {
        update_informer::fake(
            update_informer::registry::GitHub,
            REPO,
            "0.4.0",
            new_version,
        )
    }

    // ===== newer / same / older version =====

    #[test]
    fn newer_release_produces_notice() {
        let latest = check_latest(fake("0.4.1"), "0.4.0").expect("newer version detected");
        assert_eq!(latest, "0.4.1");
        let notice = format_notice("0.4.0", &latest, InstallMethod::Unknown);
        assert!(notice.contains("AstroFetch 0.4.1 is available (current: 0.4.0)"));
    }

    #[test]
    fn same_version_is_silent() {
        assert_eq!(check_latest(fake("0.4.0"), "0.4.0"), None);
        assert_eq!(newer_version("0.4.0", "0.4.0"), None);
    }

    #[test]
    fn older_remote_version_is_silent() {
        assert_eq!(check_latest(fake("0.3.0"), "0.4.0"), None);
        assert_eq!(newer_version("0.4.0", "0.3.0"), None);
    }

    #[test]
    fn unparsable_version_is_silent() {
        assert_eq!(newer_version("0.4.0", "not-a-version"), None);
    }

    // ===== failure silence =====

    #[test]
    fn check_failure_is_silent() {
        assert_eq!(check_latest(FailingInformer, "0.4.0"), None);
    }

    // ===== disabling =====

    #[test]
    fn disabled_via_cli_flag() {
        assert!(is_disabled(true, None));
        assert!(!should_check(true, None, true));
    }

    #[test]
    fn disabled_via_env_var() {
        assert!(is_disabled(false, Some("1")));
        assert!(is_disabled(false, Some("true")));
        assert!(is_disabled(false, Some("TRUE")));
        assert!(!is_disabled(false, Some("0")));
        assert!(!is_disabled(false, None));
        assert!(!should_check(false, Some("1"), true));
    }

    // ===== non-interactive stderr =====

    #[test]
    fn non_interactive_stderr_skips_check() {
        assert!(!should_check(false, None, false));
        assert!(should_check(false, None, true));
    }

    // ===== installation method detection =====

    #[test]
    fn detects_homebrew_on_macos_intel() {
        let exe = Path::new("/usr/local/Cellar/astrofetch/0.4.0/bin/astrofetch");
        assert_eq!(detect_install_method(exe), InstallMethod::Homebrew);
        assert_eq!(
            update_command(InstallMethod::Homebrew),
            "brew upgrade astrofetch"
        );
    }

    #[test]
    fn detects_homebrew_on_macos_arm() {
        let exe = Path::new("/opt/homebrew/Cellar/astrofetch/0.4.0/bin/astrofetch");
        assert_eq!(detect_install_method(exe), InstallMethod::Homebrew);
    }

    #[test]
    fn detects_homebrew_on_linux() {
        let exe = Path::new("/home/linuxbrew/.linuxbrew/Cellar/astrofetch/0.4.0/bin/astrofetch");
        assert_eq!(detect_install_method(exe), InstallMethod::Homebrew);
    }

    #[test]
    fn detects_install_script() {
        let exe = Path::new("/home/user/.local/bin/astrofetch");
        assert_eq!(detect_install_method(exe), InstallMethod::InstallScript);
        assert_eq!(
            update_command(InstallMethod::InstallScript),
            INSTALL_SCRIPT_CMD
        );
    }

    #[test]
    fn unknown_install_falls_back_to_release_url() {
        let exe = Path::new("/opt/custom/astrofetch");
        assert_eq!(detect_install_method(exe), InstallMethod::Unknown);
        assert_eq!(update_command(InstallMethod::Unknown), RELEASES_URL);
    }

    #[test]
    fn missing_exe_path_is_unknown() {
        assert_eq!(detect_install_method(Path::new("")), InstallMethod::Unknown);
    }

    // ===== no competing installation =====

    #[test]
    fn homebrew_notice_never_suggests_install_script() {
        let notice = format_notice("0.4.0", "0.4.1", InstallMethod::Homebrew);
        assert!(notice.contains("brew upgrade astrofetch"));
        assert!(!notice.contains("install.sh"));
    }

    #[test]
    fn install_script_notice_never_suggests_brew() {
        let notice = format_notice("0.4.0", "0.4.1", InstallMethod::InstallScript);
        assert!(notice.contains(INSTALL_SCRIPT_CMD));
        assert!(!notice.contains("brew upgrade"));
    }

    #[test]
    fn unknown_notice_is_release_url_only() {
        let notice = format_notice("0.4.0", "0.4.1", InstallMethod::Unknown);
        assert!(notice.contains(RELEASES_URL));
        assert!(!notice.contains("brew upgrade"));
        assert!(!notice.contains("install.sh"));
    }
}
