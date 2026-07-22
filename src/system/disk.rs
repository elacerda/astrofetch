#[cfg(target_os = "linux")]
use std::collections::BTreeMap;

#[cfg(any(target_os = "linux", test))]
use std::collections::BTreeSet;

use super::fields::SystemField;
#[cfg(any(target_os = "linux", test))]
use super::format;

/// Representação de uso de disco para um storage device específico.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(any(target_os = "linux", test))]
pub(crate) struct DiskUsageEntry {
    pub(crate) key: String,
    pub(crate) total_bytes: u64,
    pub(crate) available_bytes: u64,
}

/// Identidade do mount point no Linux, derivada do `/proc/self/mountinfo`.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinuxMountIdentity {
    pub(crate) key: String,
    pub(crate) fs_type: String,
}

/// FormataUsage de disco para um único filesystem no padrão screenFetch.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn format_disk_usage_bytes(used_bytes: u64, total_bytes: u64) -> String {
    let used_formatted = format::format_bytes(used_bytes);
    let total_formatted = format::format_bytes(total_bytes);
    let percent = ((used_bytes as f64 / total_bytes as f64) * 100.0).round() as u8;

    format!("{} / {} ({}%)", used_formatted, total_formatted, percent)
}

/// Formata uma lista de entradas de disco numa única string agregada.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn format_disk_usage_entries(entries: &[DiskUsageEntry]) -> String {
    let Some((used_bytes, total_bytes)) = aggregate_unique_disk_usage(entries) else {
        return "N/A".to_string();
    };

    format_disk_usage_bytes(used_bytes, total_bytes)
}

/// Agraga uso de disco único (excluindo duplicatas do mesmo device subjacente).
/// Retorna `(used_bytes, total_bytes)` ou `None` se não houver filesystem útil.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn aggregate_unique_disk_usage(entries: &[DiskUsageEntry]) -> Option<(u64, u64)> {
    let mut seen = BTreeSet::new();
    let mut total_bytes: u64 = 0;
    let mut used_bytes: u64 = 0;

    for entry in entries {
        if entry.total_bytes == 0 || !seen.insert(entry.key.clone()) {
            continue;
        }

        total_bytes = total_bytes.saturating_add(entry.total_bytes);
        used_bytes =
            used_bytes.saturating_add(entry.total_bytes.saturating_sub(entry.available_bytes));
    }

    if total_bytes == 0 {
        None
    } else {
        Some((used_bytes, total_bytes))
    }
}

/// Retorna `true` quando o tipo de filesystem é virtual e não deve ser contado na métrica de disco.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn is_ignored_filesystem(fs_type: &str) -> bool {
    matches!(
        fs_type,
        "tmpfs"
            | "devtmpfs"
            | "proc"
            | "sysfs"
            | "cgroup"
            | "cgroup2"
            | "squashfs"
            | "overlay"
            | "debugfs"
            | "tracefs"
            | "efivarfs"
            | "fusectl"
            | "ramfs"
            | "autofs"
            | "devpts"
            | "hugetlbfs"
            | "mqueue"
            | "configfs"
            | "securityfs"
            | "pstore"
            | "bpf"
            | "binfmt_misc"
            | "nsfs"
    ) || fs_type.starts_with("fuse.")
}

/// Remove sufixos de subvolume/[BTRFS] do nome do source, ex: `/dev/sda1[/@home]` → `/dev/sda1`.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn normalize_disk_source(source: &str) -> String {
    let source = source.trim();
    let source = source.split_once('[').map_or(source, |(base, _)| base);
    source.trim().to_string()
}

/// Chave de identidade fallback quando não há leitura do `mountinfo`.
#[cfg(target_os = "linux")]
fn fallback_disk_identity_key(fs_type: &str, source: &str, mount_point: &str) -> String {
    let source = normalize_disk_source(source);

    if source.is_empty() || source == "none" {
        format!("mount:{fs_type}:{mount_point}")
    } else {
        format!("source:{fs_type}:{source}")
    }
}

/// Coleta todas as entradas de disco com base numa lista de dispositivos `sysinfo`.
#[cfg(target_os = "linux")]
pub(crate) fn collect_linux_disk_usage_entries(disks: &sysinfo::Disks) -> Vec<DiskUsageEntry> {
    let mount_identities = linux_mount_identities_by_mount_point();

    disks
        .iter()
        .filter_map(|disk| linux_disk_usage_entry(disk, &mount_identities))
        .collect()
}

/// Compõe uma entrada de disco a partir de um `sysinfo::Disk` e o mapa de identidades.
#[cfg(target_os = "linux")]
pub(crate) fn linux_disk_usage_entry(
    disk: &sysinfo::Disk,
    mount_identities: &BTreeMap<String, LinuxMountIdentity>,
) -> Option<DiskUsageEntry> {
    let total_bytes = disk.total_space();

    if total_bytes == 0 {
        return None;
    }

    let mount_point = disk.mount_point().to_string_lossy().to_string();
    let sysinfo_fs_type = disk.file_system().to_string_lossy().to_ascii_lowercase();
    let source = disk.name().to_string_lossy();

    let (fs_type, key) = mount_identities
        .get(&mount_point)
        .map(|identity| (identity.fs_type.as_str(), identity.key.clone()))
        .unwrap_or_else(|| {
            (
                sysinfo_fs_type.as_str(),
                fallback_disk_identity_key(&sysinfo_fs_type, &source, &mount_point),
            )
        });

    if is_ignored_filesystem(fs_type) {
        return None;
    }

    Some(DiskUsageEntry {
        key,
        total_bytes,
        available_bytes: disk.available_space(),
    })
}

/// Lê `/proc/self/mountinfo` e retorna o mapa de identidade de disco por mount point.
#[cfg(target_os = "linux")]
pub(crate) fn linux_mount_identities_by_mount_point() -> BTreeMap<String, LinuxMountIdentity> {
    let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return BTreeMap::new();
    };

    parse_linux_mountinfo_identities(&text)
}

/// Faz parse do texto do `/proc/self/mountinfo` e retorna o mapa de identidades.
#[cfg(target_os = "linux")]
pub(crate) fn parse_linux_mountinfo_identities(text: &str) -> BTreeMap<String, LinuxMountIdentity> {
    let mut identities = BTreeMap::new();

    for line in text.lines() {
        if let Some((mount_point, identity)) = parse_linux_mountinfo_line(line) {
            identities.insert(mount_point, identity);
        }
    }

    identities
}

/// Faz parse de uma única linha do `/proc/self/mountinfo`, retornando a identidade do mount point.
#[cfg(target_os = "linux")]
pub(crate) fn parse_linux_mountinfo_line(line: &str) -> Option<(String, LinuxMountIdentity)> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < 10 {
        return None;
    }

    let device_id = parts[2];
    let mount_point = decode_mountinfo_path(parts[4]);
    let separator_index = parts.iter().position(|part| *part == "-")?;

    if parts.len() <= separator_index + 1 {
        return None;
    }

    let fs_type = parts[separator_index + 1].to_ascii_lowercase();
    let key = format!("mountinfo:{fs_type}:{device_id}");

    Some((mount_point, LinuxMountIdentity { key, fs_type }))
}

/// Decodifica sequências de escape de path no `mountinfo`: `\040`→` `, `\011`→`\t`, `\012`→`\n`, `\134`→`\`.
#[cfg(target_os = "linux")]
fn decode_mountinfo_path(path: &str) -> String {
    path.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

/// Obtém informações de disco usando sysinfo.
#[cfg(target_os = "linux")]
pub(crate) fn get_disk_info() -> String {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let entries = collect_linux_disk_usage_entries(&disks);
    format_disk_usage_entries(&entries)
}

/// Obtém informações de disco no macOS.
#[cfg(target_os = "macos")]
pub(crate) fn get_disk_info() -> String {
    use super::parsers::parse_df_size;

    // Tenta usar df para obter informações do disco raiz
    if let Ok(output) = std::process::Command::new("df").arg("-h").arg("/").output() {
        if let Ok(text) = String::from_utf8(output.stdout) {
            // Parse a saída do df
            let lines: Vec<&str> = text.lines().collect();
            if lines.len() >= 2 {
                let parts: Vec<&str> = lines[1].split_whitespace().collect();
                if parts.len() >= 4 {
                    let used = parts[2];
                    let total = parts[1];
                    // Tenta calcular percentual
                    if let Some(used_val) = parse_df_size(used) {
                        if let Some(total_val) = parse_df_size(total) {
                            if total_val > 0 {
                                let percent =
                                    ((used_val as f64 / total_val as f64) * 100.0).round() as u8;
                                return format!("{} / {} ({}%)", used, total, percent);
                            }
                        }
                    }
                }
            }
        }
    }
    "N/A".to_string()
}

/// Obtém informações de disco no Windows.
#[cfg(target_os = "windows")]
pub(crate) fn get_disk_info() -> String {
    // Tenta usar wmic para obter informações do disco
    if let Ok(output) = std::process::Command::new("wmic")
        .arg("logicaldisk")
        .arg("where")
        .arg("DeviceID='C:'")
        .arg("get")
        .arg("Size,FreeSpace")
        .output()
    {
        if let Ok(text) = String::from_utf8(output.stdout) {
            // Parse a saída
            let lines: Vec<&str> = text.lines().collect();
            if lines.len() >= 2 {
                let parts: Vec<&str> = lines[1].split_whitespace().collect();
                if parts.len() >= 2 {
                    let total = parts[0];
                    let free = parts[1];
                    if let (Ok(total_val), Ok(free_val)) =
                        (total.parse::<u64>(), free.parse::<u64>())
                    {
                        let used = total_val - free_val;
                        let total_gb = total_val as f64 / (1024.0 * 1024.0 * 1024.0);
                        let used_gb = used as f64 / (1024.0 * 1024.0 * 1024.0);
                        let percent = if total_val > 0 {
                            ((used as f64 / total_val as f64) * 100.0).round() as u8
                        } else {
                            0
                        };
                        return format!("{:.1}GB / {:.1}GB ({}%)", used_gb, total_gb, percent);
                    }
                }
            }
        }
    }
    "N/A".to_string()
}

/// Obtém informações de disco em plataformas desconhecidas.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) fn get_disk_info() -> String {
    "N/A".to_string()
}

/// Obtém detalhes de disco por filesystem contado.
///
/// Currently discloses individual partitions on Linux. Em outras
/// plataformas, a flag `--disk-details` mantém a saída padrão sem linhas extras.
pub fn get_disk_detail_fields() -> Vec<SystemField> {
    #[cfg(target_os = "linux")]
    {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let mount_identities = linux_mount_identities_by_mount_point();
        let mut seen = BTreeSet::new();
        let mut details = Vec::new();

        for disk in disks.iter() {
            let mount_point = disk.mount_point().to_string_lossy().to_string();

            let Some(entry) = linux_disk_usage_entry(disk, &mount_identities) else {
                continue;
            };

            if !seen.insert(entry.key) {
                continue;
            }

            let used_bytes = entry.total_bytes.saturating_sub(entry.available_bytes);
            details.push(SystemField::new(
                format!("Disk {}", mount_point),
                format_disk_usage_bytes(used_bytes, entry.total_bytes),
            ));
        }

        details
    }

    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregate_unique_disk_usage_single_filesystem() {
        let entries = vec![DiskUsageEntry {
            key: "mountinfo:btrfs:254:0".to_string(),
            total_bytes: 100,
            available_bytes: 40,
        }];

        assert_eq!(aggregate_unique_disk_usage(&entries), Some((60, 100)));
        assert_eq!(format_disk_usage_entries(&entries), "60B / 100B (60%)");
    }

    #[test]
    fn test_aggregate_unique_disk_usage_multiple_unique_filesystems() {
        let entries = vec![
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:0".to_string(),
                total_bytes: 100,
                available_bytes: 40,
            },
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:1".to_string(),
                total_bytes: 300,
                available_bytes: 100,
            },
        ];

        assert_eq!(aggregate_unique_disk_usage(&entries), Some((260, 400)));
        assert_eq!(format_disk_usage_entries(&entries), "260B / 400B (65%)");
    }

    #[test]
    fn test_aggregate_unique_disk_usage_deduplicates_btrfs_subvolumes() {
        let entries = vec![
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:0".to_string(),
                total_bytes: 235,
                available_bytes: 116,
            },
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:0".to_string(),
                total_bytes: 235,
                available_bytes: 116,
            },
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:0".to_string(),
                total_bytes: 235,
                available_bytes: 116,
            },
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:1".to_string(),
                total_bytes: 932,
                available_bytes: 42,
            },
        ];

        assert_eq!(aggregate_unique_disk_usage(&entries), Some((1009, 1167)));
    }

    #[test]
    fn test_format_disk_usage_entries_returns_na_for_no_counted_filesystems() {
        let entries = vec![
            DiskUsageEntry {
                key: "mountinfo:tmpfs:0:42".to_string(),
                total_bytes: 0,
                available_bytes: 0,
            },
            DiskUsageEntry {
                key: "mountinfo:proc:0:43".to_string(),
                total_bytes: 0,
                available_bytes: 0,
            },
        ];

        assert_eq!(aggregate_unique_disk_usage(&entries), None);
        assert_eq!(format_disk_usage_entries(&entries), "N/A");
    }

    #[test]
    fn test_is_ignored_filesystem_filters_virtual_and_transient_filesystems() {
        for fs_type in [
            "tmpfs",
            "devtmpfs",
            "proc",
            "sysfs",
            "cgroup2",
            "overlay",
            "squashfs",
            "fuse.portal",
            "fuse.gvfsd-fuse",
        ] {
            assert!(
                is_ignored_filesystem(fs_type),
                "{fs_type} should be ignored"
            );
        }

        for fs_type in ["btrfs", "ext4", "xfs", "vfat", "ntfs", "exfat"] {
            assert!(
                !is_ignored_filesystem(fs_type),
                "{fs_type} should be counted"
            );
        }
    }

    #[test]
    fn test_normalize_disk_source_removes_btrfs_subvolume_suffix() {
        assert_eq!(
            normalize_disk_source("/dev/mapper/luks-abc[/@home]"),
            "/dev/mapper/luks-abc"
        );
        assert_eq!(
            normalize_disk_source("/dev/mapper/luks-abc[/@cache]"),
            "/dev/mapper/luks-abc"
        );
        assert_eq!(normalize_disk_source("/dev/sda1"), "/dev/sda1");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_linux_mountinfo_deduplicates_btrfs_subvolume_identity() {
        let text = "\
25 1 254:0 /@ / rw,relatime - btrfs /dev/mapper/root rw,subvol=/@
26 1 254:0 /@home /home rw,relatime - btrfs /dev/mapper/root rw,subvol=/@home
27 1 254:1 /@data /mnt/Data rw,relatime - btrfs /dev/mapper/data rw,subvol=/@data
28 1 259:2 / /boot rw,relatime - vfat /dev/nvme0n1p2 rw
";

        let identities = parse_linux_mountinfo_identities(text);

        assert_eq!(identities["/"].key, "mountinfo:btrfs:254:0");
        assert_eq!(identities["/home"].key, "mountinfo:btrfs:254:0");
        assert_eq!(identities["/mnt/Data"].key, "mountinfo:btrfs:254:1");
        assert_eq!(identities["/boot"].key, "mountinfo:vfat:259:2");
    }
}
