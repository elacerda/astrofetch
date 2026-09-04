//! Pure, cross-platform cache primitives for Patch 5E (Act 5E1).
//!
//! This module provides the typed cache keys, the canonical binary
//! `CacheScope` encoding, the bounded entry codec, the clock/TTL seam,
//! the `CacheStore` trait, and the pure cache-directory resolver.
//!
//! The Linux `FsCache` (5E2) is the production persistent filesystem
//! store over these primitives; the collector integration arrives in
//! 5E3. Everything else in this module is deterministic and
//! unit-tested without touching the real environment or the
//! filesystem.

#[cfg(test)]
use std::collections::HashMap;
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::fs::{self, File, OpenOptions};
#[cfg(target_os = "linux")]
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::AtomicBool;
#[cfg(target_os = "linux")]
use std::sync::atomic::AtomicU64;
#[cfg(any(target_os = "linux", test))]
use std::sync::atomic::Ordering;
#[cfg(test)]
use std::sync::Mutex;

use super::desktop::DesktopCosmetics;

// ---------------------------------------------------------------------------
// Wire format constants
// ---------------------------------------------------------------------------

/// Magic bytes identifying an AstroFetch cache entry.
const MAGIC: [u8; 4] = *b"ASTF";

/// Current on-disk format version.
const FORMAT_VERSION: u8 = 1;

/// Wire id for `StringCacheKey::Packages`.
const WIRE_PACKAGES: u8 = 0;
/// Wire id for `StringCacheKey::Resolution`.
const WIRE_RESOLUTION: u8 = 1;
/// Wire id for `StringCacheKey::Gpu`.
const WIRE_GPU: u8 = 2;
/// Wire id for the dedicated DesktopCosmetics entry.
const WIRE_COSMETICS: u8 = 3;

/// Payload kind: non-empty UTF-8 string.
const KIND_STRING: u8 = 0;
/// Payload kind: four optional length-prefixed UTF-8 fields.
const KIND_COSMETICS: u8 = 1;

/// Fixed header size in bytes:
/// magic(4) + version(1) + key(1) + kind(1) + created_at(8) + scope_len(4)
/// + payload_len(4).
const HEADER_SIZE: usize = 23;

/// Maximum encoded scope size in bytes: the widest scope
/// (DesktopCosmetics) is one required string (2 + 65535) plus six
/// optional strings (6 x (1 + 2 + 65535)).
const MAX_SCOPE_SIZE: usize = (2 + u16::MAX as usize) + 6 * (1 + 2 + u16::MAX as usize);

/// Maximum payload size in bytes: the largest payload is a cosmetics
/// entry with four fields at the u16 length maximum (4 x (2 + 65535)).
/// String payloads are bounded by the same limit.
const MAX_PAYLOAD_SIZE: usize = 4 * (2 + u16::MAX as usize);

/// Maximum total entry size: header + maximum scope + maximum payload.
const MAX_ENTRY_SIZE: usize = HEADER_SIZE + MAX_SCOPE_SIZE + MAX_PAYLOAD_SIZE;

// ---------------------------------------------------------------------------
// TTLs (seconds)
// ---------------------------------------------------------------------------

/// Time-to-live for the package count (30 minutes).
const PACKAGES_TTL: u64 = 30 * 60;
/// Time-to-live for the display resolution (5 minutes).
const RESOLUTION_TTL: u64 = 5 * 60;
/// Time-to-live for the GPU information (24 hours).
const GPU_TTL: u64 = 24 * 60 * 60;
/// Time-to-live for the desktop cosmetics (15 minutes).
const COSMETICS_TTL: u64 = 15 * 60;

// ---------------------------------------------------------------------------
// Typed keys
// ---------------------------------------------------------------------------

/// Typed keys for the single-string cache entries.
///
/// Wire ids and file names are static; arbitrary key strings are not
/// accepted anywhere in the cache layer. DesktopCosmetics is not a
/// `StringCacheKey`: it has dedicated operations and its own wire id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringCacheKey {
    /// Installed package count (`dpkg`).
    Packages,
    /// Display resolution (`xrandr`).
    Resolution,
    /// GPU model (`lspci`).
    Gpu,
}

impl StringCacheKey {
    /// Static file name of the dedicated DesktopCosmetics entry.
    pub(crate) const COSMETICS_FILE_NAME: &'static str = "cosmetics";

    /// Static wire id of this key.
    fn wire_id(&self) -> u8 {
        match self {
            StringCacheKey::Packages => WIRE_PACKAGES,
            StringCacheKey::Resolution => WIRE_RESOLUTION,
            StringCacheKey::Gpu => WIRE_GPU,
        }
    }

    /// Parses a wire id; unknown ids (including the cosmetics id) are
    /// rejected.
    #[cfg(test)]
    fn from_wire_id(id: u8) -> Option<Self> {
        match id {
            WIRE_PACKAGES => Some(StringCacheKey::Packages),
            WIRE_RESOLUTION => Some(StringCacheKey::Resolution),
            WIRE_GPU => Some(StringCacheKey::Gpu),
            _ => None,
        }
    }

    /// Static file name stem of this key (one file per key in 5E2).
    fn file_name(&self) -> &'static str {
        match self {
            StringCacheKey::Packages => "packages",
            StringCacheKey::Resolution => "resolution",
            StringCacheKey::Gpu => "gpu",
        }
    }
}

/// TTL in seconds for a typed string key.
pub(crate) fn ttl_for_key(key: StringCacheKey) -> u64 {
    match key {
        StringCacheKey::Packages => PACKAGES_TTL,
        StringCacheKey::Resolution => RESOLUTION_TTL,
        StringCacheKey::Gpu => GPU_TTL,
    }
}

/// TTL in seconds for the DesktopCosmetics entry.
pub(crate) fn ttl_for_cosmetics() -> u64 {
    COSMETICS_TTL
}

/// TTL hit/miss decision over Unix epoch seconds.
///
/// `created_at > now` is a miss (future timestamp); `age < ttl` is a
/// hit; `age >= ttl` is a miss.
pub(crate) fn is_fresh(created_at: u64, now: u64, ttl: u64) -> bool {
    if created_at > now {
        return false;
    }
    now - created_at < ttl
}

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

/// Private clock seam for TTL evaluation.
///
/// `System` reads the wall clock; `Fixed` is a test-only deterministic
/// clock. A failing system clock yields `None`, which callers treat as
/// a cache miss (read) or a skipped write.
pub(crate) enum Clock {
    /// Wall clock in Unix epoch seconds.
    System,
    /// Test-only fixed timestamp in Unix epoch seconds.
    #[cfg(test)]
    Fixed(u64),
}

impl Clock {
    /// Current time in Unix epoch seconds, or `None` on clock failure.
    pub(crate) fn now(&self) -> Option<u64> {
        match self {
            Clock::System => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs()),
            #[cfg(test)]
            Clock::Fixed(t) => Some(*t),
        }
    }
}

// ---------------------------------------------------------------------------
// CacheScope
// ---------------------------------------------------------------------------

/// Identity of the environment a cache entry was collected in.
///
/// An entry is only reusable when the stored scope bytes exactly match
/// the scope encoded for the current environment. `USER` is never part
/// of the scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheScope {
    /// Host name (same value the collector renders; supplied by it).
    host: String,
    /// `std::env::consts::OS`.
    os: String,
    /// `std::env::consts::ARCH`.
    arch: String,
    /// `DISPLAY`; empty values are normalized to `None`.
    display: Option<String>,
    /// `WAYLAND_DISPLAY`; empty values are normalized to `None`.
    wayland_display: Option<String>,
    /// `XDG_SESSION_TYPE`; empty values are normalized to `None`.
    xdg_session_type: Option<String>,
    /// `XDG_CURRENT_DESKTOP`; empty values are normalized to `None`.
    xdg_current_desktop: Option<String>,
    /// `DESKTOP_SESSION`; empty values are normalized to `None`.
    desktop_session: Option<String>,
    /// `XDG_SESSION_DESKTOP`; empty values are normalized to `None`.
    xdg_session_desktop: Option<String>,
}

impl CacheScope {
    /// Builds a scope from explicit values.
    ///
    /// `os` and `arch` are supplied explicitly so tests stay
    /// deterministic; production uses `from_environment`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        host: &str,
        os: &str,
        arch: &str,
        display: Option<&str>,
        wayland_display: Option<&str>,
        xdg_session_type: Option<&str>,
        xdg_current_desktop: Option<&str>,
        desktop_session: Option<&str>,
        xdg_session_desktop: Option<&str>,
    ) -> Self {
        Self {
            host: host.to_string(),
            os: os.to_string(),
            arch: arch.to_string(),
            display: display.map(str::to_string),
            wayland_display: wayland_display.map(str::to_string),
            xdg_session_type: xdg_session_type.map(str::to_string),
            xdg_current_desktop: xdg_current_desktop.map(str::to_string),
            desktop_session: desktop_session.map(str::to_string),
            xdg_session_desktop: xdg_session_desktop.map(str::to_string),
        }
    }

    /// Builds the scope for the current environment.
    ///
    /// `host` is supplied by the collector (the same value it renders);
    /// the graphical/session variables are read from the process
    /// environment and empty values are normalized to `None`.
    #[cfg(target_os = "linux")]
    pub(crate) fn from_environment(host: &str) -> Self {
        Self::new(
            host,
            std::env::consts::OS,
            std::env::consts::ARCH,
            normalize_env_value(std::env::var_os("DISPLAY")).as_deref(),
            normalize_env_value(std::env::var_os("WAYLAND_DISPLAY")).as_deref(),
            normalize_env_value(std::env::var_os("XDG_SESSION_TYPE")).as_deref(),
            normalize_env_value(std::env::var_os("XDG_CURRENT_DESKTOP")).as_deref(),
            normalize_env_value(std::env::var_os("DESKTOP_SESSION")).as_deref(),
            normalize_env_value(std::env::var_os("XDG_SESSION_DESKTOP")).as_deref(),
        )
    }

    /// Canonical scope bytes for a typed string key:
    /// Packages/GPU encode `host, os, arch`; Resolution encodes
    /// `host, display, wayland_display, xdg_session_type`.
    pub(crate) fn encode_for_string_key(&self, key: StringCacheKey) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        push_required(&mut out, &self.host)?;
        match key {
            StringCacheKey::Packages | StringCacheKey::Gpu => {
                push_required(&mut out, &self.os)?;
                push_required(&mut out, &self.arch)?;
            }
            StringCacheKey::Resolution => {
                push_optional(&mut out, &self.display)?;
                push_optional(&mut out, &self.wayland_display)?;
                push_optional(&mut out, &self.xdg_session_type)?;
            }
        }
        finish_scope(out)
    }

    /// Canonical scope bytes for the DesktopCosmetics entry:
    /// `host, display, wayland_display, xdg_session_type,
    /// xdg_current_desktop, desktop_session, xdg_session_desktop`.
    pub(crate) fn encode_for_cosmetics(&self) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        push_required(&mut out, &self.host)?;
        push_optional(&mut out, &self.display)?;
        push_optional(&mut out, &self.wayland_display)?;
        push_optional(&mut out, &self.xdg_session_type)?;
        push_optional(&mut out, &self.xdg_current_desktop)?;
        push_optional(&mut out, &self.desktop_session)?;
        push_optional(&mut out, &self.xdg_session_desktop)?;
        finish_scope(out)
    }
}

/// Normalizes an environment value: `None` stays `None`, an empty
/// string becomes `None`, anything else becomes `Some`.
fn normalize_env_value(value: Option<OsString>) -> Option<String> {
    value
        .map(|v| v.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
}

/// Appends a required scope string: u16 LE length + UTF-8 bytes.
fn push_required(out: &mut Vec<u8>, value: &str) -> Option<()> {
    let len = u16::try_from(value.len()).ok()?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(value.as_bytes());
    Some(())
}

/// Appends an optional scope string: presence byte, then the required
/// encoding when present.
fn push_optional(out: &mut Vec<u8>, value: &Option<String>) -> Option<()> {
    match value {
        None => {
            out.push(0);
            Some(())
        }
        Some(s) => {
            out.push(1);
            push_required(out, s)
        }
    }
}

/// Final scope bounds check (checked arithmetic).
fn finish_scope(out: Vec<u8>) -> Option<Vec<u8>> {
    if out.len() > MAX_SCOPE_SIZE {
        return None;
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Entry codec
// ---------------------------------------------------------------------------

/// Decoded single-string entry.
pub(crate) struct DecodedStringEntry {
    /// Unix epoch seconds when the entry was written.
    pub(crate) created_at: u64,
    /// Validated raw scope bytes (compared byte-for-byte on read).
    pub(crate) scope: Vec<u8>,
    /// Non-empty UTF-8 payload.
    pub(crate) value: String,
}

/// Decoded DesktopCosmetics entry.
pub(crate) struct DecodedCosmeticsEntry {
    /// Unix epoch seconds when the entry was written.
    pub(crate) created_at: u64,
    /// Validated raw scope bytes (compared byte-for-byte on read).
    pub(crate) scope: Vec<u8>,
    /// `wm_theme` field.
    pub(crate) wm_theme: Option<String>,
    /// `gtk_theme` field.
    pub(crate) gtk_theme: Option<String>,
    /// `icon_theme` field.
    pub(crate) icon_theme: Option<String>,
    /// `font` field.
    pub(crate) font: Option<String>,
}

/// Encodes a single-string entry.
///
/// Returns `None` on empty/oversized input or checked-arithmetic
/// failure; never panics. All integers are little-endian.
pub(crate) fn encode_string_entry(
    key: StringCacheKey,
    created_at: u64,
    scope: &[u8],
    value: &str,
) -> Option<Vec<u8>> {
    let payload = value.as_bytes();
    if payload.is_empty() || payload.len() > MAX_PAYLOAD_SIZE || scope.len() > MAX_SCOPE_SIZE {
        return None;
    }
    let mut out = Vec::with_capacity(HEADER_SIZE + scope.len() + payload.len());
    push_header(
        &mut out,
        key.wire_id(),
        KIND_STRING,
        created_at,
        scope.len(),
        payload.len(),
    )?;
    out.extend_from_slice(scope);
    out.extend_from_slice(payload);
    Some(out)
}

/// Encodes a DesktopCosmetics entry.
///
/// The payload is exactly four length-prefixed fields in fixed order
/// (`wm_theme`, `gtk_theme`, `icon_theme`, `font`); a zero length means
/// `None`. An all-`None` cosmetics set is not cacheable and returns
/// `None`. Never panics.
pub(crate) fn encode_cosmetics_entry(
    created_at: u64,
    scope: &[u8],
    cosmetics: &DesktopCosmetics,
) -> Option<Vec<u8>> {
    let fields = [
        cosmetics.wm_theme.as_deref(),
        cosmetics.gtk_theme.as_deref(),
        cosmetics.icon_theme.as_deref(),
        cosmetics.font.as_deref(),
    ];
    if fields.iter().all(Option::is_none) || scope.len() > MAX_SCOPE_SIZE {
        return None;
    }
    let mut payload = Vec::new();
    for field in &fields {
        match field {
            None => payload.extend_from_slice(&0u16.to_le_bytes()),
            Some(s) => {
                let len = u16::try_from(s.len()).ok()?;
                payload.extend_from_slice(&len.to_le_bytes());
                payload.extend_from_slice(s.as_bytes());
            }
        }
    }
    if payload.len() > MAX_PAYLOAD_SIZE {
        return None;
    }
    let mut out = Vec::with_capacity(HEADER_SIZE + scope.len() + payload.len());
    push_header(
        &mut out,
        WIRE_COSMETICS,
        KIND_COSMETICS,
        created_at,
        scope.len(),
        payload.len(),
    )?;
    out.extend_from_slice(scope);
    out.extend_from_slice(&payload);
    Some(out)
}

/// Writes the fixed 23-byte header; fails on checked-arithmetic
/// overflow (lengths above u32::MAX cannot occur given the MAX_*
/// bounds, but the check is kept explicit).
fn push_header(
    out: &mut Vec<u8>,
    wire_key: u8,
    kind: u8,
    created_at: u64,
    scope_len: usize,
    payload_len: usize,
) -> Option<()> {
    let scope_len = u32::try_from(scope_len).ok()?;
    let payload_len = u32::try_from(payload_len).ok()?;
    out.extend_from_slice(&MAGIC);
    out.push(FORMAT_VERSION);
    out.push(wire_key);
    out.push(kind);
    out.extend_from_slice(&created_at.to_le_bytes());
    out.extend_from_slice(&scope_len.to_le_bytes());
    out.extend_from_slice(&payload_len.to_le_bytes());
    Some(())
}

/// Decodes and validates a single-string entry for the expected typed
/// key.
///
/// Returns `None` (never panics) on wrong magic/version/key/kind,
/// truncation, trailing bytes, invalid UTF-8, oversized sections, or
/// checked-arithmetic failure.
pub(crate) fn decode_string_entry(key: StringCacheKey, bytes: &[u8]) -> Option<DecodedStringEntry> {
    let (created_at, scope, payload) =
        decode_header_and_sections(bytes, key.wire_id(), KIND_STRING)?;
    if payload.is_empty() {
        return None;
    }
    let value = std::str::from_utf8(payload).ok()?.to_string();
    Some(DecodedStringEntry {
        created_at,
        scope,
        value,
    })
}

/// Decodes and validates a DesktopCosmetics entry.
///
/// The payload must be exactly four length-prefixed fields in fixed
/// order; non-zero fields must be valid UTF-8; an all-`None` result is
/// rejected. Returns `None` (never panics) on any malformation.
pub(crate) fn decode_cosmetics_entry(bytes: &[u8]) -> Option<DecodedCosmeticsEntry> {
    let (created_at, scope, payload) =
        decode_header_and_sections(bytes, WIRE_COSMETICS, KIND_COSMETICS)?;
    let mut cur = Cursor::new(payload);
    let mut fields = [None, None, None, None];
    for field in fields.iter_mut() {
        let len = cur.take_u16()? as usize;
        if len == 0 {
            continue;
        }
        let raw = cur.take(len)?;
        *field = Some(std::str::from_utf8(raw).ok()?.to_string());
    }
    if !cur.at_end() {
        return None;
    }
    if fields.iter().all(Option::is_none) {
        return None;
    }
    let [wm_theme, gtk_theme, icon_theme, font] = fields;
    Some(DecodedCosmeticsEntry {
        created_at,
        scope,
        wm_theme,
        gtk_theme,
        icon_theme,
        font,
    })
}

/// Validates the header (magic, version, key, kind), the section
/// bounds, and the exact total length, then returns the validated
/// scope bytes and payload slice.
fn decode_header_and_sections(
    bytes: &[u8],
    expected_key: u8,
    expected_kind: u8,
) -> Option<(u64, Vec<u8>, &[u8])> {
    let mut cur = Cursor::new(bytes);
    if cur.take(4)? != &MAGIC[..] {
        return None;
    }
    if cur.take_u8()? != FORMAT_VERSION {
        return None;
    }
    if cur.take_u8()? != expected_key {
        return None;
    }
    if cur.take_u8()? != expected_kind {
        return None;
    }
    let created_at = cur.take_u64()?;
    let scope_len = cur.take_u32()? as usize;
    let payload_len = cur.take_u32()? as usize;
    if scope_len > MAX_SCOPE_SIZE || payload_len > MAX_PAYLOAD_SIZE {
        return None;
    }
    let total = HEADER_SIZE
        .checked_add(scope_len)?
        .checked_add(payload_len)?;
    if bytes.len() != total {
        return None;
    }
    let scope = cur.take(scope_len)?.to_vec();
    validate_scope(&scope, expected_key)?;
    let payload = cur.take(payload_len)?;
    Some((created_at, scope, payload))
}

/// Validates the canonical scope layout for a wire key: the exact
/// field count and order, well-formed length prefixes, and no trailing
/// bytes.
fn validate_scope(scope: &[u8], wire_key: u8) -> Option<()> {
    let mut cur = Cursor::new(scope);
    read_required(&mut cur)?;
    match wire_key {
        WIRE_PACKAGES | WIRE_GPU => {
            read_required(&mut cur)?;
            read_required(&mut cur)?;
        }
        WIRE_RESOLUTION => {
            read_optional(&mut cur)?;
            read_optional(&mut cur)?;
            read_optional(&mut cur)?;
        }
        WIRE_COSMETICS => {
            for _ in 0..6 {
                read_optional(&mut cur)?;
            }
        }
        _ => return None,
    }
    cur.at_end().then_some(())
}

/// Reads one required scope string (u16 LE length + bytes).
fn read_required(cur: &mut Cursor) -> Option<()> {
    let len = cur.take_u16()? as usize;
    cur.take(len)?;
    Some(())
}

/// Reads one optional scope string (presence byte + optional string).
fn read_optional(cur: &mut Cursor) -> Option<()> {
    match cur.take_u8()? {
        0 => Some(()),
        1 => read_required(cur),
        _ => None,
    }
}

/// Bounds-checked read cursor over an entry byte slice.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// Takes `n` bytes with checked bounds; `None` when truncated.
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        if end > self.bytes.len() {
            return None;
        }
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Some(slice)
    }

    fn take_u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn take_u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn take_u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn take_u64(&mut self) -> Option<u64> {
        self.take(8)
            .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    fn at_end(&self) -> bool {
        self.pos == self.bytes.len()
    }
}

// ---------------------------------------------------------------------------
// CacheStore
// ---------------------------------------------------------------------------

/// Storage backend for the 5E cache.
///
/// Implementations own the storage location and the clock; every
/// operation receives the current `CacheScope`, so a store never needs
/// to know host/session identity itself.
///
/// A `get` returns `None` for any miss reason (absent, malformed,
/// stale, scope mismatch). A `put` that cannot complete silently
/// abandons the update; callers always keep the live value.
pub(crate) trait CacheStore {
    /// Reads a typed string entry; `None` on any miss.
    fn get_string(&self, key: StringCacheKey, scope: &CacheScope) -> Option<String>;
    /// Writes a typed string entry. Empty values are not cached.
    fn put_string(&self, key: StringCacheKey, scope: &CacheScope, value: &str);
    /// Reads the DesktopCosmetics entry; `None` on any miss.
    fn get_cosmetics(&self, scope: &CacheScope) -> Option<DesktopCosmetics>;
    /// Writes the DesktopCosmetics entry. An all-`None` set is not cached.
    fn put_cosmetics(&self, scope: &CacheScope, cosmetics: &DesktopCosmetics);
}

// ---------------------------------------------------------------------------
// Cache directory resolver
// ---------------------------------------------------------------------------

/// Resolves the cache directory from an injected environment lookup.
///
/// Priority:
/// 1. `ASTROFETCH_DISABLE_CACHE` truthy (`1`, `true`, `yes`, `on`,
///    case-insensitive) -> `None`;
/// 2. absolute non-empty `ASTROFETCH_CACHE_DIR`;
/// 3. absolute `XDG_CACHE_HOME` joined with `astrofetch`;
/// 4. absolute `HOME` joined with `.cache/astrofetch`;
/// 5. otherwise `None`.
///
/// There is no CWD fallback; relative values fall through to the next
/// priority.
pub(crate) fn resolve_cache_dir_with(lookup: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    if lookup("ASTROFETCH_DISABLE_CACHE").is_some_and(|v| is_truthy(&v)) {
        return None;
    }
    if let Some(v) = lookup("ASTROFETCH_CACHE_DIR") {
        let path = Path::new(&v);
        if !v.is_empty() && path.is_absolute() {
            return Some(path.to_path_buf());
        }
    }
    if let Some(v) = lookup("XDG_CACHE_HOME") {
        let path = Path::new(&v);
        if !v.is_empty() && path.is_absolute() {
            return Some(path.join("astrofetch"));
        }
    }
    if let Some(v) = lookup("HOME") {
        let path = Path::new(&v);
        if !v.is_empty() && path.is_absolute() {
            return Some(path.join(".cache").join("astrofetch"));
        }
    }
    None
}

/// Production resolver over the real process environment.
#[cfg(target_os = "linux")]
pub(crate) fn resolve_cache_dir() -> Option<PathBuf> {
    resolve_cache_dir_with(&|key| std::env::var_os(key))
}

/// Case-insensitive truthy check for the disable flag.
fn is_truthy(value: &OsString) -> bool {
    matches!(
        value.to_string_lossy().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

// ---------------------------------------------------------------------------
// Test-only in-memory store
// ---------------------------------------------------------------------------

/// Test-only in-memory `CacheStore`.
///
/// Stores fully encoded entry bytes (the same bytes the 5E2 `FsCache`
/// will store as file contents) and reuses the real codec, scope
/// matching, and TTL logic, so 5E3 collector tests exercise the
/// production paths. Production code never depends on it.
#[cfg(test)]
pub(crate) struct FakeCache {
    entries: Mutex<HashMap<String, Vec<u8>>>,
    clock: Mutex<Clock>,
    fail_put: AtomicBool,
}

#[cfg(test)]
impl FakeCache {
    /// Creates an empty fake store with the given clock.
    pub(crate) fn new(clock: Clock) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            clock: Mutex::new(clock),
            fail_put: AtomicBool::new(false),
        }
    }

    /// Advances the fake clock to a fixed timestamp (test only).
    pub(crate) fn set_now(&self, t: u64) {
        *self.clock.lock().unwrap() = Clock::Fixed(t);
    }

    /// Forces subsequent `put`s to fail (simulates a write error).
    pub(crate) fn set_fail_put(&self, fail: bool) {
        self.fail_put.store(fail, Ordering::SeqCst);
    }

    /// Inserts a raw encoded entry under a file name (pre-population).
    pub(crate) fn insert_raw(&self, file_name: &str, entry: Vec<u8>) {
        self.entries
            .lock()
            .unwrap()
            .insert(file_name.to_string(), entry);
    }

    /// Number of stored entries.
    pub(crate) fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
}

#[cfg(test)]
impl CacheStore for FakeCache {
    fn get_string(&self, key: StringCacheKey, scope: &CacheScope) -> Option<String> {
        let now = self.clock.lock().unwrap().now()?;
        let entries = self.entries.lock().unwrap();
        let raw = entries.get(key.file_name())?;
        let decoded = decode_string_entry(key, raw)?;
        let expected_scope = scope.encode_for_string_key(key)?;
        if decoded.scope != expected_scope {
            return None;
        }
        if !is_fresh(decoded.created_at, now, ttl_for_key(key)) {
            return None;
        }
        Some(decoded.value)
    }

    fn put_string(&self, key: StringCacheKey, scope: &CacheScope, value: &str) {
        if value.is_empty() || self.fail_put.load(Ordering::SeqCst) {
            return;
        }
        let now = match self.clock.lock().unwrap().now() {
            Some(now) => now,
            None => return,
        };
        let scope_bytes = match scope.encode_for_string_key(key) {
            Some(bytes) => bytes,
            None => return,
        };
        let entry = match encode_string_entry(key, now, &scope_bytes, value) {
            Some(entry) => entry,
            None => return,
        };
        self.entries
            .lock()
            .unwrap()
            .insert(key.file_name().to_string(), entry);
    }

    fn get_cosmetics(&self, scope: &CacheScope) -> Option<DesktopCosmetics> {
        let now = self.clock.lock().unwrap().now()?;
        let entries = self.entries.lock().unwrap();
        let raw = entries.get(StringCacheKey::COSMETICS_FILE_NAME)?;
        let decoded = decode_cosmetics_entry(raw)?;
        let expected_scope = scope.encode_for_cosmetics()?;
        if decoded.scope != expected_scope {
            return None;
        }
        if !is_fresh(decoded.created_at, now, ttl_for_cosmetics()) {
            return None;
        }
        Some(DesktopCosmetics {
            wm_theme: decoded.wm_theme,
            gtk_theme: decoded.gtk_theme,
            icon_theme: decoded.icon_theme,
            font: decoded.font,
        })
    }

    fn put_cosmetics(&self, scope: &CacheScope, cosmetics: &DesktopCosmetics) {
        if self.fail_put.load(Ordering::SeqCst) {
            return;
        }
        let now = match self.clock.lock().unwrap().now() {
            Some(now) => now,
            None => return,
        };
        let scope_bytes = match scope.encode_for_cosmetics() {
            Some(bytes) => bytes,
            None => return,
        };
        let entry = match encode_cosmetics_entry(now, &scope_bytes, cosmetics) {
            Some(entry) => entry,
            None => return,
        };
        self.entries
            .lock()
            .unwrap()
            .insert(StringCacheKey::COSMETICS_FILE_NAME.to_string(), entry);
    }
}

// ---------------------------------------------------------------------------
// Linux filesystem store
// ---------------------------------------------------------------------------

/// Persistent filesystem `CacheStore` for Linux.
///
/// Owns only the cache directory and the clock; the `CacheScope` is
/// supplied by the caller on every operation, so this store never
/// reads host/session identity itself. Entries live as one file per
/// static key name inside the cache directory.
///
/// Reads are strictly bounded: only regular files are accepted
/// (symlinks, directories, and other non-regular entries are
/// rejected), the read is capped at `MAX_ENTRY_SIZE + 1` bytes, and
/// any anomaly (absent, empty, oversized, malformed, stale, scope
/// mismatch) is a miss.
///
/// Writes are best-effort: the entry is encoded, written to a unique
/// temp file, flushed, and atomically renamed over the final name.
/// Any failure removes only that temp file and abandons the update;
/// the final path is never written directly and the caller's live
/// value is never affected.
#[cfg(target_os = "linux")]
pub(crate) struct FsCache {
    /// Directory holding the cache entry files.
    cache_dir: PathBuf,
    /// Clock used for TTL evaluation.
    clock: Clock,
}

/// Per-process counter for unique temp file names.
#[cfg(target_os = "linux")]
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "linux")]
impl FsCache {
    /// Production constructor: resolves the cache directory from the
    /// process environment and uses the system clock.
    ///
    /// Returns `None` when the cache is disabled or no absolute cache
    /// directory can be resolved.
    pub(crate) fn from_environment() -> Option<Self> {
        Some(Self {
            cache_dir: resolve_cache_dir()?,
            clock: Clock::System,
        })
    }

    /// Test constructor with an injected directory and clock.
    #[cfg(test)]
    pub(crate) fn new_for_test(cache_dir: PathBuf, clock: Clock) -> Self {
        Self { cache_dir, clock }
    }

    /// Reads one entry file as raw bytes, or `None` on any anomaly.
    ///
    /// Uses `symlink_metadata` and accepts only regular files, so
    /// symlinks and non-regular entries are rejected. Empty or
    /// oversized metadata is rejected up front, and the read itself is
    /// hard-bounded to `MAX_ENTRY_SIZE + 1` bytes with an over-bound
    /// result rejected. Never panics.
    fn read_entry(&self, file_name: &str) -> Option<Vec<u8>> {
        let path = self.cache_dir.join(file_name);
        let meta = fs::symlink_metadata(&path).ok()?;
        if !meta.file_type().is_file() {
            return None;
        }
        let size = meta.len();
        if size == 0 || size > MAX_ENTRY_SIZE as u64 {
            return None;
        }
        let file = File::open(&path).ok()?;
        let mut buf = Vec::with_capacity(size as usize);
        let mut bounded = (&file).take(MAX_ENTRY_SIZE as u64 + 1);
        bounded.read_to_end(&mut buf).ok()?;
        (buf.len() <= MAX_ENTRY_SIZE).then_some(buf)
    }

    /// Best-effort atomic write of one encoded entry.
    ///
    /// Creates the cache directory if needed, builds a unique temp
    /// file name (`.name.bin.tmp.<pid>.<counter>`), then delegates to
    /// [`FsCache::write_entry_with_temp_path`]. The final path is
    /// never written directly.
    fn write_entry(&self, file_name: &str, entry: &[u8]) {
        if fs::create_dir_all(&self.cache_dir).is_err() {
            return;
        }
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = self.cache_dir.join(format!(
            ".{file_name}.bin.tmp.{}.{}",
            std::process::id(),
            counter
        ));
        self.write_entry_with_temp_path(file_name, entry, &temp_path);
    }

    /// Writes `entry` via `temp_path` and renames it to the final name.
    ///
    /// This process owns `temp_path` only after `create_new` succeeds.
    /// If `create_new` fails (for example because the path already
    /// exists and belongs to another writer), this method returns
    /// immediately without touching `temp_path`. Once ownership is
    /// established, a failed write, flush, or rename removes
    /// `temp_path` best-effort before returning; a successful rename
    /// needs no cleanup. The final path is never written directly.
    fn write_entry_with_temp_path(&self, file_name: &str, entry: &[u8], temp_path: &Path) {
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temp_path)
        {
            Ok(file) => file,
            // The path already exists: it belongs to another writer,
            // so it must not be removed.
            Err(_) => return,
        };
        if file.write_all(entry).is_err() || file.flush().is_err() {
            let _ = fs::remove_file(temp_path);
            return;
        }
        drop(file);
        let final_path = self.cache_dir.join(file_name);
        if fs::rename(temp_path, &final_path).is_err() {
            let _ = fs::remove_file(temp_path);
        }
    }
}

#[cfg(target_os = "linux")]
impl CacheStore for FsCache {
    fn get_string(&self, key: StringCacheKey, scope: &CacheScope) -> Option<String> {
        let now = self.clock.now()?;
        let raw = self.read_entry(key.file_name())?;
        let decoded = decode_string_entry(key, &raw)?;
        let expected_scope = scope.encode_for_string_key(key)?;
        if decoded.scope != expected_scope {
            return None;
        }
        if !is_fresh(decoded.created_at, now, ttl_for_key(key)) {
            return None;
        }
        Some(decoded.value)
    }

    fn put_string(&self, key: StringCacheKey, scope: &CacheScope, value: &str) {
        if value.is_empty() {
            return;
        }
        let now = match self.clock.now() {
            Some(now) => now,
            None => return,
        };
        let scope_bytes = match scope.encode_for_string_key(key) {
            Some(bytes) => bytes,
            None => return,
        };
        let entry = match encode_string_entry(key, now, &scope_bytes, value) {
            Some(entry) => entry,
            None => return,
        };
        self.write_entry(key.file_name(), &entry);
    }

    fn get_cosmetics(&self, scope: &CacheScope) -> Option<DesktopCosmetics> {
        let now = self.clock.now()?;
        let raw = self.read_entry(StringCacheKey::COSMETICS_FILE_NAME)?;
        let decoded = decode_cosmetics_entry(&raw)?;
        let expected_scope = scope.encode_for_cosmetics()?;
        if decoded.scope != expected_scope {
            return None;
        }
        if !is_fresh(decoded.created_at, now, ttl_for_cosmetics()) {
            return None;
        }
        Some(DesktopCosmetics {
            wm_theme: decoded.wm_theme,
            gtk_theme: decoded.gtk_theme,
            icon_theme: decoded.icon_theme,
            font: decoded.font,
        })
    }

    fn put_cosmetics(&self, scope: &CacheScope, cosmetics: &DesktopCosmetics) {
        let now = match self.clock.now() {
            Some(now) => now,
            None => return,
        };
        let scope_bytes = match scope.encode_for_cosmetics() {
            Some(bytes) => bytes,
            None => return,
        };
        let entry = match encode_cosmetics_entry(now, &scope_bytes, cosmetics) {
            Some(entry) => entry,
            None => return,
        };
        self.write_entry(StringCacheKey::COSMETICS_FILE_NAME, &entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_scope(host: &str) -> CacheScope {
        CacheScope::new(
            host,
            "linux",
            "x86_64",
            Some(":0"),
            Some("wayland-1"),
            Some("wayland"),
            Some("GNOME"),
            Some("gnome"),
            Some("GNOME"),
        )
    }

    fn bare_scope(host: &str) -> CacheScope {
        CacheScope::new(host, "linux", "x86_64", None, None, None, None, None, None)
    }

    fn cosmetics_all() -> DesktopCosmetics {
        DesktopCosmetics {
            wm_theme: Some("Adwaita".to_string()),
            gtk_theme: Some("Adwaita".to_string()),
            icon_theme: Some("Yaru".to_string()),
            font: Some("Noto Sans 11".to_string()),
        }
    }

    fn cosmetics_partial() -> DesktopCosmetics {
        DesktopCosmetics {
            wm_theme: Some("Adwaita".to_string()),
            gtk_theme: None,
            icon_theme: None,
            font: Some("Noto Sans 11".to_string()),
        }
    }

    fn string_scope_bytes(key: StringCacheKey, scope: &CacheScope) -> Vec<u8> {
        scope.encode_for_string_key(key).unwrap()
    }

    fn cosmetics_scope_bytes(scope: &CacheScope) -> Vec<u8> {
        scope.encode_for_cosmetics().unwrap()
    }

    fn string_entry(key: StringCacheKey, created_at: u64, scope: &[u8], value: &str) -> Vec<u8> {
        encode_string_entry(key, created_at, scope, value).unwrap()
    }

    fn cosmetics_entry(created_at: u64, scope: &[u8], cosmetics: &DesktopCosmetics) -> Vec<u8> {
        encode_cosmetics_entry(created_at, scope, cosmetics).unwrap()
    }

    fn enc_required(s: &str) -> Vec<u8> {
        let mut v = (s.len() as u16).to_le_bytes().to_vec();
        v.extend(s.bytes());
        v
    }

    fn enc_optional(v: Option<&str>) -> Vec<u8> {
        match v {
            None => vec![0],
            Some(s) => {
                let mut out = vec![1];
                out.extend(enc_required(s));
                out
            }
        }
    }

    fn lookup_from<'a>(pairs: Vec<(&'a str, String)>) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| OsString::from(v.clone()))
        }
    }

    fn abs(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn test_wire_key_ids_are_stable() {
        assert_eq!(WIRE_PACKAGES, 0);
        assert_eq!(WIRE_RESOLUTION, 1);
        assert_eq!(WIRE_GPU, 2);
        assert_eq!(WIRE_COSMETICS, 3);
        assert_eq!(StringCacheKey::Packages.wire_id(), WIRE_PACKAGES);
        assert_eq!(StringCacheKey::Resolution.wire_id(), WIRE_RESOLUTION);
        assert_eq!(StringCacheKey::Gpu.wire_id(), WIRE_GPU);
        assert_eq!(
            StringCacheKey::from_wire_id(0),
            Some(StringCacheKey::Packages)
        );
        assert_eq!(
            StringCacheKey::from_wire_id(1),
            Some(StringCacheKey::Resolution)
        );
        assert_eq!(StringCacheKey::from_wire_id(2), Some(StringCacheKey::Gpu));
        assert_eq!(StringCacheKey::from_wire_id(3), None);
        assert_eq!(StringCacheKey::from_wire_id(255), None);
    }

    #[test]
    fn test_ttl_values_are_seconds() {
        assert_eq!(ttl_for_key(StringCacheKey::Packages), 30 * 60);
        assert_eq!(ttl_for_key(StringCacheKey::Resolution), 5 * 60);
        assert_eq!(ttl_for_key(StringCacheKey::Gpu), 24 * 60 * 60);
        assert_eq!(ttl_for_cosmetics(), 15 * 60);
    }

    #[test]
    fn test_size_constants_are_coherent() {
        assert_eq!(HEADER_SIZE, 4 + 1 + 1 + 1 + 8 + 4 + 4);
        assert_eq!(MAX_SCOPE_SIZE, 65537 + 6 * 65538);
        assert_eq!(MAX_PAYLOAD_SIZE, 4 * 65537);
        assert_eq!(
            MAX_ENTRY_SIZE,
            HEADER_SIZE + MAX_SCOPE_SIZE + MAX_PAYLOAD_SIZE
        );
    }

    #[test]
    fn test_scope_encoding_packages_and_gpu_layout() {
        let scope = full_scope("host-a");
        let expected = enc_required("host-a")
            .into_iter()
            .chain(enc_required("linux"))
            .chain(enc_required("x86_64"))
            .collect::<Vec<u8>>();
        assert_eq!(
            scope
                .encode_for_string_key(StringCacheKey::Packages)
                .unwrap(),
            expected
        );
        assert_eq!(
            scope.encode_for_string_key(StringCacheKey::Gpu).unwrap(),
            expected
        );
    }

    #[test]
    fn test_scope_encoding_resolution_layout() {
        let scope = full_scope("host-a");
        let expected = enc_required("host-a")
            .into_iter()
            .chain(enc_optional(Some(":0")))
            .chain(enc_optional(Some("wayland-1")))
            .chain(enc_optional(Some("wayland")))
            .collect::<Vec<u8>>();
        assert_eq!(
            scope
                .encode_for_string_key(StringCacheKey::Resolution)
                .unwrap(),
            expected
        );
    }

    #[test]
    fn test_scope_encoding_cosmetics_layout() {
        let scope = full_scope("host-a");
        let expected = enc_required("host-a")
            .into_iter()
            .chain(enc_optional(Some(":0")))
            .chain(enc_optional(Some("wayland-1")))
            .chain(enc_optional(Some("wayland")))
            .chain(enc_optional(Some("GNOME")))
            .chain(enc_optional(Some("gnome")))
            .chain(enc_optional(Some("GNOME")))
            .collect::<Vec<u8>>();
        assert_eq!(scope.encode_for_cosmetics().unwrap(), expected);
    }

    #[test]
    fn test_scope_empty_optionals_encode_absent() {
        let scope = bare_scope("host-b");
        let expected = enc_required("host-b")
            .into_iter()
            .chain(std::iter::repeat_n(0u8, 6))
            .collect::<Vec<u8>>();
        assert_eq!(scope.encode_for_cosmetics().unwrap(), expected);
    }

    #[test]
    fn test_scope_field_longer_than_u16_is_rejected() {
        let scope = CacheScope::new(
            &"x".repeat(65536),
            "linux",
            "x86_64",
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(scope
            .encode_for_string_key(StringCacheKey::Packages)
            .is_none());
        assert!(scope.encode_for_cosmetics().is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_scope_from_environment_uses_consts() {
        let scope = CacheScope::from_environment("bench-host");
        assert_eq!(scope.host, "bench-host");
        assert_eq!(scope.os, std::env::consts::OS);
        assert_eq!(scope.arch, std::env::consts::ARCH);
    }

    #[test]
    fn test_normalize_env_value_empty_becomes_none() {
        assert_eq!(normalize_env_value(None), None);
        assert_eq!(normalize_env_value(Some(OsString::from(""))), None);
        assert_eq!(
            normalize_env_value(Some(OsString::from("wayland-1"))),
            Some("wayland-1".to_string())
        );
    }

    #[test]
    fn test_string_round_trip() {
        let scope = full_scope("host-a");
        let scope_bytes = string_scope_bytes(StringCacheKey::Packages, &scope);
        let bytes = string_entry(StringCacheKey::Packages, 1234, &scope_bytes, "1234");
        let decoded = decode_string_entry(StringCacheKey::Packages, &bytes).unwrap();
        assert_eq!(decoded.created_at, 1234);
        assert_eq!(decoded.value, "1234");
        assert_eq!(decoded.scope, scope_bytes);
    }

    #[test]
    fn test_cosmetics_full_round_trip() {
        let scope = full_scope("host-a");
        let scope_bytes = cosmetics_scope_bytes(&scope);
        let bytes = cosmetics_entry(4321, &scope_bytes, &cosmetics_all());
        let decoded = decode_cosmetics_entry(&bytes).unwrap();
        assert_eq!(decoded.created_at, 4321);
        assert_eq!(decoded.wm_theme.as_deref(), Some("Adwaita"));
        assert_eq!(decoded.gtk_theme.as_deref(), Some("Adwaita"));
        assert_eq!(decoded.icon_theme.as_deref(), Some("Yaru"));
        assert_eq!(decoded.font.as_deref(), Some("Noto Sans 11"));
        assert_eq!(decoded.scope, scope_bytes);
    }

    #[test]
    fn test_cosmetics_partial_round_trip() {
        let scope = full_scope("host-a");
        let bytes = cosmetics_entry(4321, &cosmetics_scope_bytes(&scope), &cosmetics_partial());
        let decoded = decode_cosmetics_entry(&bytes).unwrap();
        assert_eq!(decoded.wm_theme.as_deref(), Some("Adwaita"));
        assert_eq!(decoded.gtk_theme, None);
        assert_eq!(decoded.icon_theme, None);
        assert_eq!(decoded.font.as_deref(), Some("Noto Sans 11"));
    }

    #[test]
    fn test_utf8_newline_equal_sign_content_round_trip() {
        let scope = full_scope("host-a");
        let value = "ii  adduser=3.118\nii  apt=2.4.1\némoji 🚀\ttab";
        let scope_bytes = string_scope_bytes(StringCacheKey::Packages, &scope);
        let bytes = string_entry(StringCacheKey::Packages, 7, &scope_bytes, value);
        let decoded = decode_string_entry(StringCacheKey::Packages, &bytes).unwrap();
        assert_eq!(decoded.value, value);
    }

    #[test]
    fn test_decode_rejects_truncated_input() {
        let scope = full_scope("host-a");
        let bytes = string_entry(
            StringCacheKey::Gpu,
            1,
            &string_scope_bytes(StringCacheKey::Gpu, &scope),
            "GPU",
        );
        for len in 0..bytes.len() {
            assert!(
                decode_string_entry(StringCacheKey::Gpu, &bytes[..len]).is_none(),
                "truncation at {len} accepted"
            );
        }
    }

    #[test]
    fn test_decode_rejects_trailing_bytes() {
        let scope = full_scope("host-a");
        let mut bytes = string_entry(
            StringCacheKey::Gpu,
            1,
            &string_scope_bytes(StringCacheKey::Gpu, &scope),
            "GPU",
        );
        bytes.push(0);
        assert!(decode_string_entry(StringCacheKey::Gpu, &bytes).is_none());
        bytes.push(0xFF);
        assert!(decode_string_entry(StringCacheKey::Gpu, &bytes).is_none());
    }

    #[test]
    fn test_decode_rejects_wrong_magic() {
        let scope = full_scope("host-a");
        let mut bytes = string_entry(
            StringCacheKey::Gpu,
            1,
            &string_scope_bytes(StringCacheKey::Gpu, &scope),
            "GPU",
        );
        bytes[0] = b'X';
        assert!(decode_string_entry(StringCacheKey::Gpu, &bytes).is_none());
    }

    #[test]
    fn test_decode_rejects_wrong_version() {
        let scope = full_scope("host-a");
        let mut bytes = string_entry(
            StringCacheKey::Gpu,
            1,
            &string_scope_bytes(StringCacheKey::Gpu, &scope),
            "GPU",
        );
        bytes[4] = FORMAT_VERSION + 1;
        assert!(decode_string_entry(StringCacheKey::Gpu, &bytes).is_none());
    }

    #[test]
    fn test_decode_rejects_wrong_key() {
        let scope = full_scope("host-a");
        let bytes = string_entry(
            StringCacheKey::Packages,
            1,
            &string_scope_bytes(StringCacheKey::Packages, &scope),
            "42",
        );
        assert!(decode_string_entry(StringCacheKey::Resolution, &bytes).is_none());
        assert!(decode_string_entry(StringCacheKey::Gpu, &bytes).is_none());
    }

    #[test]
    fn test_decode_rejects_wrong_kind() {
        let scope = full_scope("host-a");
        let scope_bytes = string_scope_bytes(StringCacheKey::Gpu, &scope);
        let mut bytes = string_entry(StringCacheKey::Gpu, 1, &scope_bytes, "GPU");
        bytes[6] = KIND_COSMETICS;
        assert!(decode_string_entry(StringCacheKey::Gpu, &bytes).is_none());
        // A string entry is not a cosmetics entry either.
        let string_bytes = string_entry(StringCacheKey::Gpu, 1, &scope_bytes, "GPU");
        assert!(decode_cosmetics_entry(&string_bytes).is_none());
    }

    #[test]
    fn test_decode_rejects_invalid_utf8_string() {
        let scope = full_scope("host-a");
        let scope_bytes = string_scope_bytes(StringCacheKey::Gpu, &scope);
        let mut bytes = Vec::new();
        push_header(&mut bytes, WIRE_GPU, KIND_STRING, 1, scope_bytes.len(), 2).unwrap();
        bytes.extend_from_slice(&scope_bytes);
        bytes.extend_from_slice(&[0xFF, 0xFE]);
        assert!(decode_string_entry(StringCacheKey::Gpu, &bytes).is_none());
    }

    #[test]
    fn test_decode_rejects_invalid_utf8_cosmetics_field() {
        let scope = full_scope("host-a");
        let scope_bytes = cosmetics_scope_bytes(&scope);
        let mut payload = Vec::new();
        payload.extend_from_slice(&2u16.to_le_bytes());
        payload.extend_from_slice(&[0xFF, 0x28]);
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.extend_from_slice(&0u16.to_le_bytes());
        let mut bytes = Vec::new();
        push_header(
            &mut bytes,
            WIRE_COSMETICS,
            KIND_COSMETICS,
            1,
            scope_bytes.len(),
            payload.len(),
        )
        .unwrap();
        bytes.extend_from_slice(&scope_bytes);
        bytes.extend_from_slice(&payload);
        assert!(decode_cosmetics_entry(&bytes).is_none());
    }

    #[test]
    fn test_encode_rejects_oversized_payload() {
        let scope = full_scope("host-a");
        let scope_bytes = string_scope_bytes(StringCacheKey::Packages, &scope);
        let big = "a".repeat(MAX_PAYLOAD_SIZE + 1);
        assert!(encode_string_entry(StringCacheKey::Packages, 1, &scope_bytes, &big).is_none());
    }

    #[test]
    fn test_decode_rejects_oversized_scope_length() {
        let scope = full_scope("host-a");
        let mut bytes = string_entry(
            StringCacheKey::Gpu,
            1,
            &string_scope_bytes(StringCacheKey::Gpu, &scope),
            "GPU",
        );
        bytes[15..19].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_string_entry(StringCacheKey::Gpu, &bytes).is_none());
    }

    #[test]
    fn test_decode_rejects_oversized_payload_length() {
        let scope = full_scope("host-a");
        let mut bytes = string_entry(
            StringCacheKey::Gpu,
            1,
            &string_scope_bytes(StringCacheKey::Gpu, &scope),
            "GPU",
        );
        bytes[19..23].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_string_entry(StringCacheKey::Gpu, &bytes).is_none());
    }

    #[test]
    fn test_all_none_cosmetics_not_encodable() {
        let scope = full_scope("host-a");
        assert!(encode_cosmetics_entry(
            1,
            &cosmetics_scope_bytes(&scope),
            &DesktopCosmetics::default()
        )
        .is_none());
    }

    #[test]
    fn test_decode_rejects_all_none_cosmetics_payload() {
        let scope = full_scope("host-a");
        let scope_bytes = cosmetics_scope_bytes(&scope);
        let mut payload = Vec::new();
        for _ in 0..4 {
            payload.extend_from_slice(&0u16.to_le_bytes());
        }
        let mut bytes = Vec::new();
        push_header(
            &mut bytes,
            WIRE_COSMETICS,
            KIND_COSMETICS,
            1,
            scope_bytes.len(),
            payload.len(),
        )
        .unwrap();
        bytes.extend_from_slice(&scope_bytes);
        bytes.extend_from_slice(&payload);
        assert!(decode_cosmetics_entry(&bytes).is_none());
    }

    #[test]
    fn test_decode_rejects_wrong_scope_shape() {
        let scope = full_scope("host-a");
        // A packages-shaped scope (three required strings) is not a
        // valid cosmetics scope (host + six optionals).
        let pkg_scope = string_scope_bytes(StringCacheKey::Packages, &scope);
        let mut bytes = Vec::new();
        push_header(
            &mut bytes,
            WIRE_COSMETICS,
            KIND_COSMETICS,
            1,
            pkg_scope.len(),
            8,
        )
        .unwrap();
        bytes.extend_from_slice(&pkg_scope);
        for _ in 0..4 {
            bytes.extend_from_slice(&0u16.to_le_bytes());
        }
        assert!(decode_cosmetics_entry(&bytes).is_none());
    }

    #[test]
    fn test_decode_rejects_scope_trailing_bytes() {
        let scope = full_scope("host-a");
        let mut pkg_scope = string_scope_bytes(StringCacheKey::Packages, &scope);
        pkg_scope.push(0);
        let mut bytes = Vec::new();
        push_header(
            &mut bytes,
            WIRE_PACKAGES,
            KIND_STRING,
            1,
            pkg_scope.len(),
            2,
        )
        .unwrap();
        bytes.extend_from_slice(&pkg_scope);
        bytes.extend_from_slice(b"42");
        assert!(decode_string_entry(StringCacheKey::Packages, &bytes).is_none());
    }

    #[test]
    fn test_ttl_hit_before_boundary() {
        assert!(is_fresh(1000, 1000, PACKAGES_TTL));
        assert!(is_fresh(1000, 1000 + PACKAGES_TTL - 1, PACKAGES_TTL));
    }

    #[test]
    fn test_ttl_miss_exactly_at_boundary() {
        assert!(!is_fresh(1000, 1000 + PACKAGES_TTL, PACKAGES_TTL));
        assert!(!is_fresh(1000, 1000 + PACKAGES_TTL + 1, PACKAGES_TTL));
    }

    #[test]
    fn test_ttl_future_timestamp_miss() {
        assert!(!is_fresh(1001, 1000, PACKAGES_TTL));
    }

    #[test]
    fn test_clock_system_now_is_some() {
        assert!(Clock::System.now().is_some());
    }

    #[test]
    fn test_clock_fixed_now() {
        assert_eq!(Clock::Fixed(42).now(), Some(42));
    }

    #[test]
    fn test_fake_cache_cold_miss_and_warm_hit() {
        let cache = FakeCache::new(Clock::Fixed(1000));
        let scope = full_scope("host-a");
        assert!(cache.get_string(StringCacheKey::Packages, &scope).is_none());
        cache.put_string(StringCacheKey::Packages, &scope, "42");
        assert_eq!(
            cache
                .get_string(StringCacheKey::Packages, &scope)
                .as_deref(),
            Some("42")
        );
    }

    #[test]
    fn test_fake_cache_expiry_at_boundary() {
        let cache = FakeCache::new(Clock::Fixed(1000));
        let scope = full_scope("host-a");
        cache.put_string(StringCacheKey::Resolution, &scope, "1920x1080");
        cache.set_now(1000 + RESOLUTION_TTL - 1);
        assert_eq!(
            cache
                .get_string(StringCacheKey::Resolution, &scope)
                .as_deref(),
            Some("1920x1080")
        );
        cache.set_now(1000 + RESOLUTION_TTL);
        assert!(cache
            .get_string(StringCacheKey::Resolution, &scope)
            .is_none());
    }

    #[test]
    fn test_fake_cache_scope_mismatch_misses() {
        let cache = FakeCache::new(Clock::Fixed(1000));
        let scope_a = full_scope("host-a");
        let scope_b = full_scope("host-b");
        cache.put_string(StringCacheKey::Packages, &scope_a, "42");
        assert!(cache
            .get_string(StringCacheKey::Packages, &scope_b)
            .is_none());

        let scope_x11 = CacheScope::new(
            "host-a",
            "linux",
            "x86_64",
            Some(":0"),
            None,
            Some("x11"),
            None,
            None,
            None,
        );
        let scope_wayland = CacheScope::new(
            "host-a",
            "linux",
            "x86_64",
            None,
            Some("wayland-1"),
            Some("wayland"),
            None,
            None,
            None,
        );
        cache.put_string(StringCacheKey::Resolution, &scope_x11, "1920x1080");
        assert!(cache
            .get_string(StringCacheKey::Resolution, &scope_wayland)
            .is_none());
    }

    #[test]
    fn test_fake_cache_failed_put_preserves_miss() {
        let cache = FakeCache::new(Clock::Fixed(1000));
        let scope = full_scope("host-a");
        cache.set_fail_put(true);
        cache.put_string(StringCacheKey::Gpu, &scope, "GPU");
        cache.put_cosmetics(&scope, &cosmetics_all());
        assert!(cache.get_string(StringCacheKey::Gpu, &scope).is_none());
        assert!(cache.get_cosmetics(&scope).is_none());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_fake_cache_empty_string_not_cached() {
        let cache = FakeCache::new(Clock::Fixed(1000));
        let scope = full_scope("host-a");
        cache.put_string(StringCacheKey::Packages, &scope, "");
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_fake_cache_all_none_cosmetics_not_cached() {
        let cache = FakeCache::new(Clock::Fixed(1000));
        let scope = full_scope("host-a");
        cache.put_cosmetics(&scope, &DesktopCosmetics::default());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_fake_cache_cosmetics_round_trip() {
        let cache = FakeCache::new(Clock::Fixed(1000));
        let scope = full_scope("host-a");
        cache.put_cosmetics(&scope, &cosmetics_partial());
        let got = cache.get_cosmetics(&scope).unwrap();
        assert_eq!(got.wm_theme.as_deref(), Some("Adwaita"));
        assert_eq!(got.gtk_theme, None);
        assert_eq!(got.icon_theme, None);
        assert_eq!(got.font.as_deref(), Some("Noto Sans 11"));
    }

    #[test]
    fn test_resolver_no_source_is_disabled() {
        assert!(resolve_cache_dir_with(&lookup_from(Vec::new())).is_none());
    }

    #[test]
    fn test_resolver_priority_and_fall_through() {
        let direct = abs("direct");
        let xdg = abs("xdg");
        let home = abs("home");

        let got = resolve_cache_dir_with(&lookup_from(vec![
            (
                "ASTROFETCH_CACHE_DIR",
                direct.to_string_lossy().into_owned(),
            ),
            ("XDG_CACHE_HOME", xdg.to_string_lossy().into_owned()),
            ("HOME", home.to_string_lossy().into_owned()),
        ]));
        assert_eq!(got, Some(direct));

        let got = resolve_cache_dir_with(&lookup_from(vec![
            ("XDG_CACHE_HOME", xdg.to_string_lossy().into_owned()),
            ("HOME", home.to_string_lossy().into_owned()),
        ]));
        assert_eq!(got, Some(xdg.join("astrofetch")));

        let got = resolve_cache_dir_with(&lookup_from(vec![(
            "HOME",
            home.to_string_lossy().into_owned(),
        )]));
        assert_eq!(got, Some(home.join(".cache").join("astrofetch")));
    }

    #[test]
    fn test_resolver_relative_paths_fall_through() {
        let home = abs("home");
        let got = resolve_cache_dir_with(&lookup_from(vec![
            ("ASTROFETCH_CACHE_DIR", "relative/direct".to_string()),
            ("XDG_CACHE_HOME", "relative/xdg".to_string()),
            ("HOME", home.to_string_lossy().into_owned()),
        ]));
        assert_eq!(got, Some(home.join(".cache").join("astrofetch")));
    }

    #[test]
    fn test_resolver_disable_wins_over_everything() {
        let direct = abs("direct");
        let got = resolve_cache_dir_with(&lookup_from(vec![
            ("ASTROFETCH_DISABLE_CACHE", "1".to_string()),
            (
                "ASTROFETCH_CACHE_DIR",
                direct.to_string_lossy().into_owned(),
            ),
        ]));
        assert!(got.is_none());
    }

    #[test]
    fn test_resolver_disable_truthy_values() {
        for value in ["1", "true", "TRUE", "Yes", "yEs", "on", "ON"] {
            let got = resolve_cache_dir_with(&lookup_from(vec![
                ("ASTROFETCH_DISABLE_CACHE", value.to_string()),
                ("HOME", abs("home").to_string_lossy().into_owned()),
            ]));
            assert!(got.is_none(), "disable={value} should disable");
        }
    }

    #[test]
    fn test_resolver_disable_falsy_values_continue() {
        for value in ["0", "false", "no", "off", ""] {
            let home = abs("home");
            let got = resolve_cache_dir_with(&lookup_from(vec![
                ("ASTROFETCH_DISABLE_CACHE", value.to_string()),
                ("HOME", home.to_string_lossy().into_owned()),
            ]));
            assert_eq!(
                got,
                Some(home.join(".cache").join("astrofetch")),
                "disable={value} should not disable"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_resolver_production_wrapper_is_absolute_or_none() {
        if let Some(path) = resolve_cache_dir() {
            assert!(path.is_absolute());
        }
    }

    #[cfg(target_os = "linux")]
    mod fscache {
        use super::*;
        use std::fs;
        use std::os::unix::fs::symlink;
        use std::sync::{Arc, Barrier};
        use std::thread;

        /// Number of concurrent writers in the same-key tests.
        const CONCURRENT_WRITERS: usize = 8;

        /// Unique temp cache directory under `std::env::temp_dir()`
        /// with guaranteed cleanup on drop.
        struct CacheDir(PathBuf);

        impl CacheDir {
            fn new(tag: &str) -> Self {
                let path = std::env::temp_dir()
                    .join(format!("astrofetch-cache-5e2-{}-{tag}", std::process::id()));
                let _ = fs::remove_dir_all(&path);
                fs::create_dir_all(&path).expect("create temp cache dir");
                Self(path)
            }

            fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for CacheDir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        fn store(dir: &Path, clock: Clock) -> FsCache {
            FsCache::new_for_test(dir.to_path_buf(), clock)
        }

        #[test]
        fn test_fs_cold_miss() {
            let dir = CacheDir::new("cold-miss");
            let cache = store(dir.path(), Clock::Fixed(1000));
            let scope = full_scope("host-a");
            assert!(cache.get_string(StringCacheKey::Packages, &scope).is_none());
            assert!(cache
                .get_string(StringCacheKey::Resolution, &scope)
                .is_none());
            assert!(cache.get_string(StringCacheKey::Gpu, &scope).is_none());
            assert!(cache.get_cosmetics(&scope).is_none());
        }

        #[test]
        fn test_fs_put_string_warm_hit() {
            let dir = CacheDir::new("string-hit");
            let cache = store(dir.path(), Clock::Fixed(1000));
            let scope = full_scope("host-a");
            cache.put_string(StringCacheKey::Packages, &scope, "42");
            assert_eq!(
                cache
                    .get_string(StringCacheKey::Packages, &scope)
                    .as_deref(),
                Some("42")
            );
            // The other keys remain cold misses.
            assert!(cache.get_string(StringCacheKey::Gpu, &scope).is_none());
        }

        #[test]
        fn test_fs_put_partial_cosmetics_warm_hit() {
            let dir = CacheDir::new("cosmetics-hit");
            let cache = store(dir.path(), Clock::Fixed(1000));
            let scope = full_scope("host-a");
            cache.put_cosmetics(&scope, &cosmetics_partial());
            let got = cache.get_cosmetics(&scope).expect("cosmetics hit");
            assert_eq!(got.wm_theme.as_deref(), Some("Adwaita"));
            assert_eq!(got.gtk_theme, None);
            assert_eq!(got.icon_theme, None);
            assert_eq!(got.font.as_deref(), Some("Noto Sans 11"));
        }

        #[test]
        fn test_fs_expiry_with_fixed_clock() {
            let dir = CacheDir::new("expiry");
            let scope = full_scope("host-a");
            store(dir.path(), Clock::Fixed(1000)).put_string(
                StringCacheKey::Resolution,
                &scope,
                "1920x1080",
            );
            // Fresh one second before the TTL boundary.
            let fresh = store(dir.path(), Clock::Fixed(1000 + RESOLUTION_TTL - 1));
            assert_eq!(
                fresh
                    .get_string(StringCacheKey::Resolution, &scope)
                    .as_deref(),
                Some("1920x1080")
            );
            // Expired exactly at the TTL boundary.
            let stale = store(dir.path(), Clock::Fixed(1000 + RESOLUTION_TTL));
            assert!(stale
                .get_string(StringCacheKey::Resolution, &scope)
                .is_none());
        }

        #[test]
        fn test_fs_future_timestamp_miss() {
            let dir = CacheDir::new("future");
            let scope = full_scope("host-a");
            // Entry written at t=2000.
            store(dir.path(), Clock::Fixed(2000)).put_string(StringCacheKey::Gpu, &scope, "GPU");
            // A reader at t=1000 sees a future timestamp: miss.
            let reader = store(dir.path(), Clock::Fixed(1000));
            assert!(reader.get_string(StringCacheKey::Gpu, &scope).is_none());
        }

        #[test]
        fn test_fs_scope_mismatch_miss() {
            let dir = CacheDir::new("scope-mismatch");
            let scope_a = full_scope("host-a");
            let scope_b = full_scope("host-b");
            let cache = store(dir.path(), Clock::Fixed(1000));
            cache.put_string(StringCacheKey::Packages, &scope_a, "42");
            cache.put_cosmetics(&scope_a, &cosmetics_all());
            assert!(cache
                .get_string(StringCacheKey::Packages, &scope_b)
                .is_none());
            assert!(cache.get_cosmetics(&scope_b).is_none());
        }

        #[test]
        fn test_fs_oversized_entry_rejected() {
            let dir = CacheDir::new("oversized");
            let scope = full_scope("host-a");
            let cache = store(dir.path(), Clock::Fixed(1000));
            fs::write(dir.path().join("packages"), vec![b'X'; MAX_ENTRY_SIZE + 1])
                .expect("write oversized entry");
            assert!(cache.get_string(StringCacheKey::Packages, &scope).is_none());
        }

        #[test]
        fn test_fs_symlinked_final_entry_rejected() {
            let dir = CacheDir::new("symlink");
            let scope = full_scope("host-a");
            let cache = store(dir.path(), Clock::Fixed(1000));
            let real = dir.path().join("real-packages");
            let entry = string_entry(
                StringCacheKey::Packages,
                1000,
                &string_scope_bytes(StringCacheKey::Packages, &scope),
                "42",
            );
            fs::write(&real, entry).expect("write real entry");
            symlink(&real, dir.path().join("packages")).expect("create symlink");
            assert!(cache.get_string(StringCacheKey::Packages, &scope).is_none());
        }

        #[test]
        fn test_fs_non_regular_final_entry_rejected() {
            let dir = CacheDir::new("non-regular");
            let scope = full_scope("host-a");
            let cache = store(dir.path(), Clock::Fixed(1000));
            fs::create_dir(dir.path().join("packages")).expect("create dir entry");
            assert!(cache.get_string(StringCacheKey::Packages, &scope).is_none());
        }

        #[test]
        fn test_create_new_failure_leaves_foreign_temp_file_untouched() {
            let dir = CacheDir::new("create-new-failure");
            let file_name = StringCacheKey::Packages.file_name();
            // Pre-create the exact temp path with sentinel bytes, as a
            // concurrent writer would have, so `create_new` fails with
            // AlreadyExists.
            let temp_path = dir.path().join(format!(
                ".{file_name}.bin.tmp.{}.424242",
                std::process::id()
            ));
            let sentinel = b"foreign-writer-sentinel-5e2";
            fs::write(&temp_path, sentinel).expect("seed foreign temp file");

            let cache = store(dir.path(), Clock::Fixed(1000));
            cache.write_entry_with_temp_path(file_name, b"never-published", &temp_path);

            // The foreign temp file survives, byte for byte.
            assert_eq!(
                fs::read(&temp_path).expect("foreign temp file still exists"),
                sentinel
            );
            // No final entry was published by the failed attempt.
            assert!(!dir.path().join(file_name).exists());
        }

        #[test]
        fn test_fs_write_failure_invalid_path_shape() {
            let dir = CacheDir::new("write-fail");
            let scope = full_scope("host-a");
            // The "cache directory" is an existing regular file: every
            // write must fail silently and every read must miss.
            let blocker = dir.path().join("blocker");
            fs::write(&blocker, b"not a directory").expect("write blocker");
            let cache = store(&blocker, Clock::Fixed(1000));
            cache.put_string(StringCacheKey::Packages, &scope, "42");
            cache.put_cosmetics(&scope, &cosmetics_all());
            assert!(cache.get_string(StringCacheKey::Packages, &scope).is_none());
            assert!(cache.get_cosmetics(&scope).is_none());
            // No temp files leaked next to the blocker.
            let leaked = fs::read_dir(dir.path())
                .expect("read dir")
                .filter_map(|e| e.ok())
                .map(|e| e.file_name())
                .filter(|name| name != "blocker")
                .count();
            assert_eq!(leaked, 0);
        }

        #[test]
        fn test_fs_concurrent_writes_independent_keys() {
            let dir = CacheDir::new("concurrent-keys");
            let scope = full_scope("host-a");
            let barrier = Arc::new(Barrier::new(2));
            let dir_a = dir.path().to_path_buf();
            let dir_b = dir.path().to_path_buf();
            let barrier_a = barrier.clone();
            let handle_a = thread::spawn(move || {
                barrier_a.wait();
                store(&dir_a, Clock::Fixed(1000)).put_string(
                    StringCacheKey::Packages,
                    &full_scope("host-a"),
                    "42",
                );
            });
            let handle_b = thread::spawn(move || {
                barrier.wait();
                store(&dir_b, Clock::Fixed(1000)).put_string(
                    StringCacheKey::Gpu,
                    &full_scope("host-a"),
                    "GPU",
                );
            });
            handle_a.join().expect("writer a");
            handle_b.join().expect("writer b");
            let cache = store(dir.path(), Clock::Fixed(1000));
            assert_eq!(
                cache
                    .get_string(StringCacheKey::Packages, &scope)
                    .as_deref(),
                Some("42")
            );
            assert_eq!(
                cache.get_string(StringCacheKey::Gpu, &scope).as_deref(),
                Some("GPU")
            );
        }

        #[test]
        fn test_fs_concurrent_writes_same_key() {
            let dir = CacheDir::new("concurrent-same");
            let scope = full_scope("host-a");
            let barrier = Arc::new(Barrier::new(CONCURRENT_WRITERS));
            let handles = spawn_same_key_writers(dir.path(), barrier);
            join_all(handles);
            let cache = store(dir.path(), Clock::Fixed(1000));
            let got = cache
                .get_string(StringCacheKey::Packages, &scope)
                .expect("one complete valid written value");
            assert!(
                (0..CONCURRENT_WRITERS)
                    .map(|i| format!("value-{i}"))
                    .any(|value| value == got),
                "unexpected value {got:?}"
            );
        }

        #[test]
        fn test_fs_no_partial_final_entry_after_concurrent_writes() {
            let dir = CacheDir::new("concurrent-integrity");
            let barrier = Arc::new(Barrier::new(CONCURRENT_WRITERS));
            let handles = spawn_same_key_writers(dir.path(), barrier);
            join_all(handles);
            // The final file must exist, stay bounded, and decode as
            // one complete valid written value (no torn/corrupt mix).
            let raw = fs::read(dir.path().join("packages")).expect("final entry exists");
            assert!(!raw.is_empty());
            assert!(raw.len() <= MAX_ENTRY_SIZE);
            let decoded =
                decode_string_entry(StringCacheKey::Packages, &raw).expect("final entry decodes");
            assert!(
                (0..CONCURRENT_WRITERS)
                    .map(|i| format!("value-{i}"))
                    .any(|value| value == decoded.value),
                "unexpected value {:?}",
                decoded.value
            );
            // Successful writes leave no temp files behind.
            let temps = fs::read_dir(dir.path())
                .expect("read dir")
                .filter_map(|e| e.ok())
                .map(|e| e.file_name())
                .filter(|name| name.to_string_lossy().contains(".tmp."))
                .count();
            assert_eq!(temps, 0);
        }

        /// Spawns `CONCURRENT_WRITERS` threads that all write distinct
        /// values to the Packages key, forced to overlap by a barrier.
        fn spawn_same_key_writers(
            dir: &Path,
            barrier: Arc<Barrier>,
        ) -> Vec<thread::JoinHandle<()>> {
            (0..CONCURRENT_WRITERS)
                .map(|i| {
                    let dir = dir.to_path_buf();
                    let barrier = barrier.clone();
                    thread::spawn(move || {
                        barrier.wait();
                        let value = format!("value-{i}");
                        store(&dir, Clock::Fixed(1000)).put_string(
                            StringCacheKey::Packages,
                            &full_scope("host-a"),
                            &value,
                        );
                    })
                })
                .collect()
        }

        fn join_all(handles: Vec<thread::JoinHandle<()>>) {
            for handle in handles {
                handle.join().expect("writer thread");
            }
        }
    }
}
