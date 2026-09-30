use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationItem {
    pub id: u32,
    pub build_date: String,
    pub nixos_version: String,
    pub kernel_version: String,
    pub is_current: bool,      // Génération active actuellement en mémoire (/run/current-system)
    pub is_boot_default: bool, // Génération par défaut configurée au boot (/nix/var/nix/profiles/system)
    pub store_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationsListResponse {
    pub generations: Vec<GenerationItem>,
    pub total_count: usize,
    pub current_id: u32,
    pub boot_default_id: u32,
    pub store_used_bytes: u64,
    pub store_free_bytes: u64,
    pub store_total_bytes: u64,
    pub store_used_human: String,
    pub store_free_human: String,
}

#[derive(Debug, Deserialize)]
pub struct SetBootRequest {
    pub generation_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CleanupRequest {
    KeepLast { count: u32 },
    OnlyCurrent,
    Custom { generation_ids: Vec<u32> },
}

#[derive(Debug, Serialize)]
pub struct CleanupResponse {
    pub success: bool,
    pub deleted_count: usize,
    pub deleted_ids: Vec<u32>,
    pub freed_bytes: u64,
    pub freed_space_human: String,
    pub error: Option<String>,
}

fn find_bin(candidates: &[&str]) -> String {
    for c in candidates {
        if Path::new(c).exists() {
            return c.to_string();
        }
    }
    candidates[0].to_string()
}

pub fn nixos_rebuild_bin() -> String {
    find_bin(&[
        "/run/current-system/sw/bin/nixos-rebuild",
        "/nix/var/nix/profiles/default/bin/nixos-rebuild",
        "nixos-rebuild",
    ])
}

pub fn nix_env_bin() -> String {
    find_bin(&[
        "/run/current-system/sw/bin/nix-env",
        "/nix/var/nix/profiles/default/bin/nix-env",
        "nix-env",
    ])
}

pub fn nix_collect_garbage_bin() -> String {
    find_bin(&[
        "/run/current-system/sw/bin/nix-collect-garbage",
        "/nix/var/nix/profiles/default/bin/nix-collect-garbage",
        "nix-collect-garbage",
    ])
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} To", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} Go", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} Mo", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} Ko", bytes as f64 / KB as f64)
    } else {
        format!("{} o", bytes)
    }
}

pub fn get_nix_store_disk_space() -> (u64, u64, u64) {
    if let Ok(output) = Command::new("df").args(["-B1", "/nix/store"]).output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let total = parts[1].parse::<u64>().unwrap_or(0);
                    let used = parts[2].parse::<u64>().unwrap_or(0);
                    let free = parts[3].parse::<u64>().unwrap_or(0);
                    return (total, used, free);
                }
            }
        }
    }
    (0, 0, 0)
}

pub fn get_boot_default_id() -> u32 {
    let system_symlink = Path::new("/nix/var/nix/profiles/system");
    if let Ok(target) = fs::read_link(system_symlink) {
        let name = target.to_string_lossy();
        if let Some(id_str) = name.strip_prefix("system-").and_then(|s| s.strip_suffix("-link")) {
            if let Ok(id) = id_str.parse::<u32>() {
                return id;
            }
        }
    }
    0
}

pub fn get_current_generation_id() -> u32 {
    let current_store = fs::canonicalize("/run/current-system")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    if !current_store.is_empty() {
        if let Ok(entries) = fs::read_dir("/nix/var/nix/profiles") {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname.starts_with("system-") && fname.ends_with("-link") {
                    if let Ok(target) = fs::canonicalize(entry.path()) {
                        if target.to_string_lossy() == current_store {
                            if let Some(id_str) = fname.strip_prefix("system-").and_then(|s| s.strip_suffix("-link")) {
                                if let Ok(id) = id_str.parse::<u32>() {
                                    return id;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    0
}

pub fn list_generations() -> Result<GenerationsListResponse, String> {
    let rebuild_bin = nixos_rebuild_bin();
    let output = Command::new(&rebuild_bin)
        .arg("list-generations")
        .output()
        .map_err(|e| format!("Impossible d'exécuter {} list-generations : {}", rebuild_bin, e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Erreur lors du listage des générations : {}", err));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut generations = Vec::new();
    let boot_default_id = get_boot_default_id();
    let mut detected_current_id = get_current_generation_id();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Generation") {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }

        let id = match parts[0].parse::<u32>() {
            Ok(v) => v,
            Err(_) => continue,
        };

        let build_date = if parts.len() >= 3 {
            format!("{} {}", parts[1], parts[2])
        } else {
            parts[1].to_string()
        };

        let nixos_version = if parts.len() >= 4 {
            parts[3].to_string()
        } else {
            "Inconnu".to_string()
        };

        let kernel_version = if parts.len() >= 5 {
            parts[4].to_string()
        } else {
            "Inconnu".to_string()
        };

        let is_current = if let Some(last) = parts.last() {
            *last == "True"
        } else {
            false
        };

        if is_current && detected_current_id == 0 {
            detected_current_id = id;
        }

        let is_boot_default = id == boot_default_id;

        let link_path = format!("/nix/var/nix/profiles/system-{}-link", id);
        let store_path = fs::canonicalize(&link_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| link_path);

        generations.push(GenerationItem {
            id,
            build_date,
            nixos_version,
            kernel_version,
            is_current,
            is_boot_default,
            store_path,
        });
    }

    // Tri décroissant (plus récente en premier)
    generations.sort_by(|a, b| b.id.cmp(&a.id));

    let (store_total, store_used, store_free) = get_nix_store_disk_space();

    Ok(GenerationsListResponse {
        total_count: generations.len(),
        current_id: detected_current_id,
        boot_default_id,
        store_used_bytes: store_used,
        store_free_bytes: store_free,
        store_total_bytes: store_total,
        store_used_human: format_bytes(store_used),
        store_free_human: format_bytes(store_free),
        generations,
    })
}

pub fn set_boot_generation(generation_id: u32) -> Result<String, String> {
    let link_path = format!("/nix/var/nix/profiles/system-{}-link", generation_id);
    if !Path::new(&link_path).exists() {
        return Err(format!("La génération {} n'existe pas sur le système.", generation_id));
    }

    let nix_env = nix_env_bin();
    let switch_out = Command::new(&nix_env)
        .args([
            "--profile",
            "/nix/var/nix/profiles/system",
            "--switch-generation",
            &generation_id.to_string(),
        ])
        .output()
        .map_err(|e| format!("Échec de switch-generation via nix-env : {}", e))?;

    if !switch_out.status.success() {
        let err = String::from_utf8_lossy(&switch_out.stderr);
        return Err(format!("Erreur lors de la bascule de génération : {}", err));
    }

    // Actualiser le chargeur de démarrage (systemd-boot) pour booter sur cette génération
    let switch_to_cfg = format!("{}/bin/switch-to-configuration", link_path);
    let boot_bin = if Path::new(&switch_to_cfg).exists() {
        switch_to_cfg
    } else {
        "/nix/var/nix/profiles/system/bin/switch-to-configuration".to_string()
    };

    let boot_out = Command::new(&boot_bin)
        .arg("boot")
        .output()
        .map_err(|e| format!("Échec de switch-to-configuration boot : {}", e))?;

    if !boot_out.status.success() {
        let err = String::from_utf8_lossy(&boot_out.stderr);
        return Err(format!("Avertissement bootloader : {}", err));
    }

    Ok(format!(
        "La génération #{} démarrera par défaut au prochain reboot du NAS.",
        generation_id
    ))
}

pub fn cleanup_generations(req: CleanupRequest) -> Result<CleanupResponse, String> {
    let list_res = list_generations()?;
    let current_id = list_res.current_id;
    let existing_ids: Vec<u32> = list_res.generations.iter().map(|g| g.id).collect();

    let to_delete: Vec<u32> = match req {
        CleanupRequest::KeepLast { count } => {
            let keep_count = count.max(1) as usize;
            if existing_ids.len() <= keep_count {
                Vec::new()
            } else {
                existing_ids[keep_count..]
                    .iter()
                    .copied()
                    .filter(|&id| id != current_id)
                    .collect()
            }
        }
        CleanupRequest::OnlyCurrent => existing_ids
            .into_iter()
            .filter(|&id| id != current_id)
            .collect(),
        CleanupRequest::Custom { generation_ids } => generation_ids
            .into_iter()
            .filter(|&id| id != current_id && existing_ids.contains(&id))
            .collect(),
    };

    if to_delete.is_empty() {
        return Ok(CleanupResponse {
            success: true,
            deleted_count: 0,
            deleted_ids: Vec::new(),
            freed_bytes: 0,
            freed_space_human: "0 o (aucune génération à supprimer)".to_string(),
            error: None,
        });
    }

    let (_, _, free_before) = get_nix_store_disk_space();

    // 1. Suppression des générations ciblées via nix-env
    let nix_env = nix_env_bin();
    let mut args = vec!["--profile", "/nix/var/nix/profiles/system", "--delete-generations"];
    let id_strings: Vec<String> = to_delete.iter().map(|id| id.to_string()).collect();
    for id_s in &id_strings {
        args.push(id_s);
    }

    let delete_out = Command::new(&nix_env)
        .args(&args)
        .output()
        .map_err(|e| format!("Échec lors de l'exécution de nix-env : {}", e))?;

    if !delete_out.status.success() {
        let err = String::from_utf8_lossy(&delete_out.stderr);
        return Err(format!("Erreur lors de la suppression des générations : {}", err));
    }

    // 2. Nettoyage du Nix Store via nix-collect-garbage
    let gc_bin = nix_collect_garbage_bin();
    let _ = Command::new(&gc_bin).output();

    // 3. Réactualisation du chargeur de démarrage UEFI / systemd-boot
    let switch_to_cfg = "/nix/var/nix/profiles/system/bin/switch-to-configuration";
    if Path::new(switch_to_cfg).exists() {
        let _ = Command::new(switch_to_cfg).arg("boot").output();
    }

    let (_, _, free_after) = get_nix_store_disk_space();
    let freed_bytes = free_after.saturating_sub(free_before);
    let freed_space_human = if freed_bytes > 0 {
        format_bytes(freed_bytes)
    } else {
        "Espace store optimisé".to_string()
    };

    Ok(CleanupResponse {
        success: true,
        deleted_count: to_delete.len(),
        deleted_ids: to_delete,
        freed_bytes,
        freed_space_human,
        error: None,
    })
}
