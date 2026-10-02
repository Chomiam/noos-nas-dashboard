use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::files::{categorize_file, format_size};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashItem {
    pub id: String, // chemin complet vers le .trashinfo
    pub name: String,
    pub original_path: String,
    pub trash_path: String,
    pub deletion_timestamp: u64,
    pub deletion_date: String,
    pub days_remaining: i64,
    pub size_bytes: u64,
    pub size_human: String,
    pub is_dir: bool,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashOverview {
    pub items: Vec<TrashItem>,
    pub total_items: usize,
    pub total_size_bytes: u64,
    pub total_size_human: String,
    pub retention_days: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrashActionRequest {
    pub id: String,
}

pub fn start_background_pruner() {
    tokio::spawn(async {
        loop {
            let _ = prune_expired_trash(30);
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        }
    });
}

fn get_configured_user() -> String {
    crate::updates::target_user()
}

pub fn get_all_trash_dirs() -> Vec<PathBuf> {
    let mut list = Vec::new();
    let user = get_configured_user();

    // 1. Corbeille principale utilisateur ({user_home}/.local/share/Trash)
    let home_trash = crate::updates::get_user_home(&user).join(".local/share/Trash");
    list.push(home_trash);

    // 2. Corbeille du volume de stockage dédié (/mnt/storage/.Trash-1000)
    let storage_dir = PathBuf::from("/mnt/storage");
    if storage_dir.exists() && storage_dir.is_dir() {
        let storage_trash = storage_dir.join(".Trash-1000");
        list.push(storage_trash);
    }

    list
}

fn get_trash_dir_for_path(path: &Path) -> PathBuf {
    let path_str = path.display().to_string();
    let storage_dir = Path::new("/mnt/storage");
    if path_str.starts_with("/mnt/storage") && storage_dir.exists() {
        storage_dir.join(".Trash-1000")
    } else {
        let user = get_configured_user();
        PathBuf::from("/home").join(user).join(".local/share/Trash")
    }
}

fn current_iso_datetime() -> String {
    if let Ok(out) = std::process::Command::new("date").arg("+%Y-%m-%dT%H:%M:%S").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    format!("{}", now)
}

fn parse_deletion_date(content: &str) -> Option<u64> {
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("DeletionDate=") {
            let s = val.trim();
            if let Ok(ts) = s.parse::<u64>() {
                return Some(ts);
            }
            if let Ok(out) = std::process::Command::new("date")
                .arg("-d")
                .arg(s)
                .arg("+%s")
                .output()
            {
                if out.status.success() {
                    let ts_str = String::from_utf8_lossy(&out.stdout);
                    if let Ok(ts) = ts_str.trim().parse::<u64>() {
                        return Some(ts);
                    }
                }
            }
        }
    }
    None
}

fn parse_info_field(content: &str, field: &str) -> Option<String> {
    let prefix = format!("{}=", field);
    for line in content.lines() {
        if let Some(val) = line.strip_prefix(&prefix) {
            return Some(val.trim().to_string());
        }
    }
    None
}

fn split_stem_ext(filename: &str) -> Option<(String, String)> {
    let p = Path::new(filename);
    let ext = p.extension()?.to_str()?;
    let stem = p.file_stem()?.to_str()?;
    Some((stem.to_string(), ext.to_string()))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn get_dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(m) = entry.metadata() {
                if m.is_dir() {
                    total += get_dir_size(&entry.path());
                } else {
                    total += m.len();
                }
            }
        }
    }
    total
}

pub fn prune_expired_trash(days: u32) -> Result<usize, String> {
    let trash_dirs = get_all_trash_dirs();
    let max_age_secs = (days as u64) * 86400;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut purged_count = 0;

    for tdir in trash_dirs {
        let info_dir = tdir.join("info");
        let files_dir = tdir.join("files");
        if !info_dir.exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(&info_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("trashinfo") {
                    if let Ok(content) = fs::read_to_string(&p) {
                        if let Some(del_secs) = parse_deletion_date(&content) {
                            if now.saturating_sub(del_secs) > max_age_secs {
                                let base_name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                                let target_file = files_dir.join(base_name);
                                if target_file.exists() {
                                    if target_file.is_dir() {
                                        let _ = fs::remove_dir_all(&target_file);
                                    } else {
                                        let _ = fs::remove_file(&target_file);
                                    }
                                }
                                let _ = fs::remove_file(&p);
                                purged_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(purged_count)
}

pub fn move_to_trash(item_path: &str) -> Result<String, String> {
    let p = crate::files::normalize_user_path(PathBuf::from(item_path));
    if !p.exists() {
        return Err("Fichier ou dossier introuvable.".into());
    }

    let canonical = p.canonicalize().map_err(|e| e.to_string())?;

    // Protection des répertoires vitaux et des points de montage
    if crate::files::is_mount_point(&canonical) || crate::files::is_mount_point(&p) {
        let name = canonical.file_name().and_then(|f| f.to_str()).unwrap_or(item_path);
        return Err(format!("Suppression interdite : le dossier '{}' est un point de montage de disque protégé.", name));
    }

    let trash_root = get_trash_dir_for_path(&canonical);
    let files_dir = trash_root.join("files");
    let info_dir = trash_root.join("info");

    fs::create_dir_all(&files_dir).map_err(|e| format!("Impossible d'initialiser la corbeille : {}", e))?;
    fs::create_dir_all(&info_dir).map_err(|e| format!("Impossible d'initialiser la corbeille : {}", e))?;

    let file_name = canonical.file_name().and_then(|f| f.to_str()).unwrap_or("element");

    // Trouver un nom unique dans la corbeille
    let mut unique_name = file_name.to_string();
    let mut counter = 1;
    while files_dir.join(&unique_name).exists() || info_dir.join(format!("{}.trashinfo", unique_name)).exists() {
        if let Some((stem, ext)) = split_stem_ext(file_name) {
            unique_name = format!("{}.{}.{}", stem, counter, ext);
        } else {
            unique_name = format!("{}.{}", file_name, counter);
        }
        counter += 1;
    }

    let target_file = files_dir.join(&unique_name);
    let target_info = info_dir.join(format!("{}.trashinfo", unique_name));

    let del_iso = current_iso_datetime();
    let trashinfo_content = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        canonical.display(),
        del_iso
    );

    fs::write(&target_info, trashinfo_content)
        .map_err(|e| format!("Impossible d'enregistrer les métadonnées de corbeille : {}", e))?;

    if let Err(_) = fs::rename(&canonical, &target_file) {
        if canonical.is_dir() {
            copy_dir_recursive(&canonical, &target_file)
                .map_err(|e| format!("Échec du déplacement vers la corbeille : {}", e))?;
            let _ = fs::remove_dir_all(&canonical);
        } else {
            fs::copy(&canonical, &target_file)
                .map_err(|e| format!("Échec du déplacement vers la corbeille : {}", e))?;
            let _ = fs::remove_file(&canonical);
        }
    }

    let _ = std::process::Command::new("chown").arg("-R").arg("1000:100").arg(&trash_root).status();
    let _ = prune_expired_trash(30);

    Ok(format!("{} déplacé dans la corbeille (rétention de 30 jours).", file_name))
}

pub fn restore_trash_item(id: &str) -> Result<String, String> {
    let info_path = PathBuf::from(id);
    if !info_path.exists() {
        return Err("Élément de corbeille introuvable.".into());
    }

    let content = fs::read_to_string(&info_path).map_err(|e| e.to_string())?;
    let original_path_str = parse_info_field(&content, "Path")
        .ok_or_else(|| "Chemin d'origine manquant dans la corbeille.".to_string())?;

    let base_name = info_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let trash_root = info_path.parent().and_then(|p| p.parent()).ok_or("Dossier corbeille invalide")?;
    let trash_file = trash_root.join("files").join(base_name);

    if !trash_file.exists() {
        let _ = fs::remove_file(&info_path);
        return Err("Fichier de corbeille introuvable.".into());
    }

    let mut dest_path = PathBuf::from(&original_path_str);
    if let Some(parent) = dest_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| format!("Impossible de recréer le dossier parent : {}", e))?;
        }
    }

    if dest_path.exists() {
        let orig_name = dest_path.file_name().and_then(|f| f.to_str()).unwrap_or("restaure");
        let parent = dest_path.parent().unwrap_or_else(|| Path::new("/"));
        if let Some((stem, ext)) = split_stem_ext(orig_name) {
            dest_path = parent.join(format!("{} (restauré).{}", stem, ext));
        } else {
            dest_path = parent.join(format!("{} (restauré)", orig_name));
        }
    }

    if let Err(_) = fs::rename(&trash_file, &dest_path) {
        if trash_file.is_dir() {
            copy_dir_recursive(&trash_file, &dest_path)
                .map_err(|e| format!("Échec de restauration du dossier : {}", e))?;
            let _ = fs::remove_dir_all(&trash_file);
        } else {
            fs::copy(&trash_file, &dest_path)
                .map_err(|e| format!("Échec de restauration du fichier : {}", e))?;
            let _ = fs::remove_file(&trash_file);
        }
    }

    let _ = fs::remove_file(&info_path);
    let _ = std::process::Command::new("chown").arg("-R").arg("1000:100").arg(&dest_path).status();

    let display_name = dest_path.file_name().and_then(|f| f.to_str()).unwrap_or("élément");
    Ok(format!("{} restauré avec succès vers {}.", display_name, dest_path.display()))
}

pub fn delete_trash_item(id: &str) -> Result<String, String> {
    let info_path = PathBuf::from(id);
    let base_name = info_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let trash_root = info_path.parent().and_then(|p| p.parent()).ok_or("Dossier corbeille invalide")?;
    let trash_file = trash_root.join("files").join(base_name);

    if trash_file.exists() {
        if trash_file.is_dir() {
            fs::remove_dir_all(&trash_file).map_err(|e| format!("Échec de suppression définitive : {}", e))?;
        } else {
            fs::remove_file(&trash_file).map_err(|e| format!("Échec de suppression définitive : {}", e))?;
        }
    }

    if info_path.exists() {
        let _ = fs::remove_file(&info_path);
    }

    Ok("Élément définitivement supprimé de la corbeille.".into())
}

pub fn empty_trash() -> Result<String, String> {
    let trash_dirs = get_all_trash_dirs();
    let mut total_deleted = 0;

    for tdir in trash_dirs {
        let files_dir = tdir.join("files");
        let info_dir = tdir.join("info");

        if files_dir.exists() {
            if let Ok(entries) = fs::read_dir(&files_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        let _ = fs::remove_dir_all(&p);
                    } else {
                        let _ = fs::remove_file(&p);
                    }
                    total_deleted += 1;
                }
            }
        }

        if info_dir.exists() {
            if let Ok(entries) = fs::read_dir(&info_dir) {
                for entry in entries.flatten() {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }

    Ok(format!("La corbeille a été vidée ({} élément(s) supprimé(s)).", total_deleted))
}

pub fn get_trash_overview() -> Result<TrashOverview, String> {
    let _ = prune_expired_trash(30);

    let trash_dirs = get_all_trash_dirs();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut items = Vec::new();
    let mut total_size = 0u64;

    for tdir in trash_dirs {
        let info_dir = tdir.join("info");
        let files_dir = tdir.join("files");
        if !info_dir.exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(&info_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("trashinfo") {
                    if let Ok(content) = fs::read_to_string(&p) {
                        let original_path = parse_info_field(&content, "Path").unwrap_or_default();
                        let del_date_str = parse_info_field(&content, "DeletionDate").unwrap_or_default();
                        let del_secs = parse_deletion_date(&content).unwrap_or(now);

                        let base_name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        let file_path = files_dir.join(&base_name);

                        if !file_path.exists() {
                            let _ = fs::remove_file(&p);
                            continue;
                        }

                        let is_dir = file_path.is_dir();
                        let size = if is_dir {
                            get_dir_size(&file_path)
                        } else {
                            file_path.metadata().map(|m| m.len()).unwrap_or(0)
                        };
                        total_size += size;

                        let display_name = Path::new(&original_path)
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or(&base_name)
                            .to_string();

                        let elapsed_secs = now.saturating_sub(del_secs);
                        let elapsed_days = elapsed_secs / 86400;
                        let days_remaining = (30i64 - elapsed_days as i64).max(0);

                        let category = if is_dir {
                            "folder".to_string()
                        } else {
                            categorize_file(&display_name)
                        };

                        items.push(TrashItem {
                            id: p.display().to_string(),
                            name: display_name,
                            original_path,
                            trash_path: file_path.display().to_string(),
                            deletion_timestamp: del_secs,
                            deletion_date: del_date_str,
                            days_remaining,
                            size_bytes: size,
                            size_human: format_size(size),
                            is_dir,
                            category,
                        });
                    }
                }
            }
        }
    }

    items.sort_by(|a, b| b.deletion_timestamp.cmp(&a.deletion_timestamp));

    let total_items = items.len();
    Ok(TrashOverview {
        items,
        total_items,
        total_size_bytes: total_size,
        total_size_human: format_size(total_size),
        retention_days: 30,
    })
}
