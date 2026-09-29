use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub size_human: String,
    pub modified: String,
    pub permissions: String,
    pub category: String, // "folder", "image", "video", "audio", "document", "archive", "code", "file"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryListing {
    pub current_path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<FileEntry>,
    pub total_items: usize,
    pub total_size_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MkdirRequest {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub path: String,
    pub new_name: String,
}

#[derive(Debug, Deserialize)]
pub struct ActionRequest {
    pub src_path: String,
    pub dest_dir: String,
}


pub fn normalize_user_path(target: PathBuf) -> PathBuf {
    if target.exists() {
        return target;
    }

    let path_str = target.to_string_lossy().to_string();

    // 1. Remplacement direct des anciens noms avec majuscules et accents
    let lower_variant = path_str
        .replace("/Documents", "/documents")
        .replace("/Images", "/images")
        .replace("/Vidéos", "/videos")
        .replace("/Videos", "/videos")
        .replace("/Musique", "/musique")
        .replace("/Music", "/musique")
        .replace("/Téléchargements", "/telechargements")
        .replace("/Telechargements", "/telechargements")
        .replace("/Downloads", "/telechargements").replace("/downloads", "/telechargements")
        .replace("/Pictures", "/images");

    let p_lower = PathBuf::from(&lower_variant);
    if p_lower.exists() {
        return p_lower;
    }

    // 2. Recherche insensible à la casse et sans accents dans le dossier parent
    if let (Some(parent), Some(file_name)) = (target.parent(), target.file_name()) {
        if parent.is_dir() {
            let target_str = file_name.to_string_lossy().to_lowercase();
            let target_clean: String = target_str
                .replace('é', "e")
                .replace('è', "e")
                .replace('ê', "e")
                .replace('à', "a");

            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let entry_name = entry.file_name().to_string_lossy().to_lowercase();
                    let entry_clean: String = entry_name
                        .replace('é', "e")
                        .replace('è', "e")
                        .replace('ê', "e")
                        .replace('à', "a");
                    if entry_name == target_str || entry_clean == target_clean {
                        return entry.path();
                    }
                }
            }
        }
    }

    target
}

pub fn list_directory(req_path: Option<&str>) -> Result<DirectoryListing, String> {
    let home = env::var("HOME").unwrap_or_else(|_| "/home/chomiam".to_string());
    let raw_target = req_path
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&home));

    let target = normalize_user_path(raw_target);

    let canonical = target.canonicalize()
        .map_err(|e| format!("Impossible d'accéder au dossier {} : {}", target.display(), e))?;

    if !canonical.is_dir() {
        return Err(format!("Le chemin {} n'est pas un dossier valide.", canonical.display()));
    }

    let parent_path = canonical.parent().map(|p| p.display().to_string());

    let mut entries = Vec::new();
    let mut total_size: u64 = 0;

    let dir_entries = fs::read_dir(&canonical)
        .map_err(|e| format!("Erreur lors de la lecture du dossier : {}", e))?;

    for item in dir_entries.flatten() {
        let file_name = item.file_name().to_string_lossy().to_string();

        // Masquer les fichiers système internes trop verbeux ou garder selon préférence
        let path = item.path();
        let path_str = path.display().to_string();

        let metadata = item.metadata().ok();
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = if is_dir { 0 } else { metadata.as_ref().map(|m| m.len()).unwrap_or(0) };
        total_size += size;

        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(format_system_time)
            .unwrap_or_else(|| "--".to_string());

        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            metadata.as_ref().map(|m| format!("{:o}", m.permissions().mode() & 0o777)).unwrap_or_else(|| "755".into())
        };
        #[cfg(not(unix))]
        let permissions = "755".to_string();

        let category = if is_dir {
            "folder".to_string()
        } else {
            categorize_file(&file_name)
        };

        entries.push(FileEntry {
            name: file_name,
            path: path_str,
            is_dir,
            size_bytes: size,
            size_human: if is_dir { "--".into() } else { format_size(size) },
            modified,
            permissions,
            category,
        });
    }

    // Tri : dossiers en premier, puis fichiers par ordre alphabétique
    entries.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    let count = entries.len();

    Ok(DirectoryListing {
        current_path: canonical.display().to_string(),
        parent_path,
        entries,
        total_items: count,
        total_size_bytes: total_size,
    })
}

pub fn create_directory(base_dir: &str, dir_name: &str) -> Result<String, String> {
    let name = dir_name.trim();
    if name.is_empty() || name.contains('/') || name == "." || name == ".." {
        return Err("Nom de dossier invalide.".into());
    }

    let norm_base = normalize_user_path(PathBuf::from(base_dir));
    let p = norm_base.join(name);
    if p.exists() {
        return Err("Un fichier ou dossier porte déjà ce nom.".into());
    }

    fs::create_dir_all(&p)
        .map_err(|e| format!("Impossible de créer le dossier : {}", e))?;

    Ok(format!("Dossier '{}' créé avec succès.", name))
}

pub fn delete_item(item_path: &str) -> Result<String, String> {
    let p = normalize_user_path(PathBuf::from(item_path));
    if !p.exists() {
        return Err("Fichier ou dossier introuvable.".into());
    }

    // Protection contre la suppression des répertoires vitaux
    let canonical = p.canonicalize().map_err(|e| e.to_string())?;
    let path_str = canonical.display().to_string();
    if path_str == "/" || path_str == "/home" || path_str == "/etc" || path_str == "/nix" || path_str == "/boot" {
        return Err("Suppression interdite sur un répertoire système racine.".into());
    }

    if canonical.is_dir() {
        fs::remove_dir_all(&canonical)
            .map_err(|e| format!("Échec de suppression du dossier : {}", e))?;
        Ok(format!("Dossier '{}' supprimé.", canonical.file_name().and_then(|f| f.to_str()).unwrap_or("")))
    } else {
        fs::remove_file(&canonical)
            .map_err(|e| format!("Échec de suppression du fichier : {}", e))?;
        Ok(format!("Fichier '{}' supprimé.", canonical.file_name().and_then(|f| f.to_str()).unwrap_or("")))
    }
}

pub fn rename_item(item_path: &str, new_name: &str) -> Result<String, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() || new_name.contains('/') || new_name == "." || new_name == ".." {
        return Err("Nouveau nom invalide.".into());
    }

    let p = normalize_user_path(PathBuf::from(item_path));
    if !p.exists() {
        return Err("Élément introuvable.".into());
    }

    let parent = p.parent().ok_or("Impossible de trouver le dossier parent.")?;
    let target = parent.join(new_name);

    if target.exists() {
        return Err("Un fichier ou dossier porte déjà ce nom dans ce répertoire.".into());
    }

    fs::rename(p, &target)
        .map_err(|e| format!("Erreur lors du renommage : {}", e))?;

    Ok(format!("Renommé en '{}' avec succès.", new_name))
}

pub fn copy_item(src: &str, dest_dir: &str) -> Result<String, String> {
    let src_path = normalize_user_path(PathBuf::from(src));
    let dest_folder = normalize_user_path(PathBuf::from(dest_dir));

    if !src_path.exists() {
        return Err("Source introuvable.".into());
    }
    if !dest_folder.is_dir() {
        return Err("Dossier de destination invalide.".into());
    }

    let file_name = src_path.file_name().ok_or("Nom de fichier source invalide.")?;
    let target = dest_folder.join(file_name);

    // Éviter de copier un dossier dans lui-même
    if src_path.is_dir() {
        if target.starts_with(&src_path) {
            return Err("Impossible de copier un dossier à l'intérieur de lui-même.".into());
        }
        copy_dir_recursive(&src_path, &target)
            .map_err(|e| format!("Erreur lors de la copie du dossier : {}", e))?;
    } else {
        fs::copy(src_path, &target)
            .map_err(|e| format!("Erreur lors de la copie du fichier : {}", e))?;
    }

    Ok("Élément copié avec succès.".into())
}

pub fn move_item(src: &str, dest_dir: &str) -> Result<String, String> {
    let src_path = normalize_user_path(PathBuf::from(src));
    let dest_folder = normalize_user_path(PathBuf::from(dest_dir));

    if !src_path.exists() {
        return Err("Source introuvable.".into());
    }
    if !dest_folder.is_dir() {
        return Err("Dossier de destination invalide.".into());
    }

    let file_name = src_path.file_name().ok_or("Nom de fichier source invalide.")?;
    let target = dest_folder.join(file_name);

    if target.exists() && target != src_path {
        return Err("Un fichier ou dossier de même nom existe déjà dans la destination.".into());
    }

    // Essai avec rename (rapide sur même partition)
    if fs::rename(src_path, &target).is_err() {
        // Fallback copie + suppression (si partitions différentes)
        copy_item(src, dest_dir)?;
        let _ = delete_item(src);
    }

    Ok("Élément déplacé avec succès.".into())
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

fn categorize_file(name: &str) -> String {
    let lower = name.to_lowercase();

    // Fichiers speciaux / dotfiles connus
    match lower.as_str() {
        "makefile" | "dockerfile" | "containerfile" | "justfile" | "rakefile" | "gemfile"
        | "cmakelists.txt" | "license" | "readme" | ".gitignore" | ".gitattributes"
        | ".bashrc" | ".bash_profile" | ".profile" | ".zshrc" | ".zshenv" | ".fishrc"
        | ".vimrc" | ".nanorc" | ".editorconfig" | ".env" | ".flake-lock" | "flake.lock"
        | "cargo.lock" | "package-lock.json" => return "code".into(),
        _ => {}
    }

    let ext = lower.split('.').last().unwrap_or("");

    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tiff" | "ico" => "image".into(),
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" => "video".into(),
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "opus" | "wma" => "audio".into(),
        "pdf" | "doc" | "docx" | "odt" | "rtf" | "xls" | "xlsx" => "document".into(),
        "txt" | "md" | "markdown" | "rst" | "org" | "tex" | "csv" | "tsv" | "log" => "document".into(),
        "zip" | "tar" | "gz" | "xz" | "bz2" | "7z" | "rar" | "zst" | "iso" => "archive".into(),
        "fish" | "nix" | "rs" | "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "html" | "htm"
        | "css" | "scss" | "sass" | "less" | "json" | "json5" | "jsonc" | "toml" | "yaml" | "yml"
        | "sh" | "bash" | "zsh" | "nu" | "ksh" | "csh" | "py" | "c" | "cpp" | "cc" | "cxx" | "h" | "hpp"
        | "go" | "lua" | "vim" | "sql" | "php" | "rb" | "xml" | "conf" | "config" | "ini" | "cfg"
        | "service" | "timer" | "target" | "socket" | "desktop" | "env" | "diff" | "patch" | "lock" => "code".into(),
        _ => "file".into(),
    }
}

fn format_size(bytes: u64) -> String {
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

fn format_system_time(time: SystemTime) -> String {
    let duration = time.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();

    // Utilisation de strftime standard via libc ou calcul simple
    // Simple format YYYY-MM-DD HH:MM
    let days = secs / 86400;
    let rem_secs = secs % 86400;
    let hours = rem_secs / 3600;
    let mins = (rem_secs % 3600) / 60;

    // Estimation simple de date sans dépendance externe chrono
    let year = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = (day_of_year / 30).min(11) + 1;
    let day = (day_of_year % 30) + 1;

    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hours, mins)
}


#[derive(Debug, Serialize, Deserialize)]
pub struct ReadFileResponse {
    pub content: String,
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub is_truncated: bool,
}

#[derive(Debug, Deserialize)]
pub struct ReadFileQuery {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct WriteFileRequest {
    pub path: String,
    pub content: String,
}

pub fn read_file_content(path_str: &str) -> Result<ReadFileResponse, String> {
    let p = normalize_user_path(PathBuf::from(path_str));
    if !p.exists() || !p.is_file() {
        return Err("Fichier introuvable.".into());
    }
    let metadata = fs::metadata(&p).map_err(|e| e.to_string())?;
    let size = metadata.len();

    const MAX_SIZE: u64 = 5 * 1024 * 1024; // 5 Mo max pour affichage texte
    let (content, is_truncated) = if size > MAX_SIZE {
        use std::io::Read;
        let mut file = fs::File::open(&p).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; MAX_SIZE as usize];
        file.read_exact(&mut buf).map_err(|e| e.to_string())?;
        (String::from_utf8_lossy(&buf).to_string(), true)
    } else {
        let bytes = fs::read(&p).map_err(|e| e.to_string())?;
        (String::from_utf8_lossy(&bytes).to_string(), false)
    };

    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    Ok(ReadFileResponse {
        content,
        path: p.display().to_string(),
        name,
        size_bytes: size,
        is_truncated,
    })
}

pub fn write_file_content(path_str: &str, content: &str) -> Result<String, String> {
    let p = normalize_user_path(PathBuf::from(path_str));
    if !p.exists() {
        return Err("Fichier cible introuvable.".into());
    }
    fs::write(&p, content).map_err(|e| format!("Échec d enregistrement : {}", e))?;
    Ok("Fichier enregistré avec succès.".into())
}
