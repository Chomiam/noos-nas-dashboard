use serde::{Deserialize, Serialize};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[allow(dead_code)]
pub fn is_supported_document(path: &Path) -> bool {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(
        ext.as_str(),
        "pdf" | "docx" | "doc" | "odt" | "rtf" | "xlsx" | "xls" | "ods" | "csv" | "pptx" | "ppt" | "odp"
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentInfoResponse {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub extension: String,
    pub is_pdf: bool,
    pub is_cached: bool,
}

fn get_cache_dir() -> PathBuf {
    let primary = PathBuf::from("/var/cache/steveos-nas-dashboard/documents");
    if fs::create_dir_all(&primary).is_ok() {
        primary
    } else {
        let fallback = std::env::temp_dir().join("steveos_doc_cache");
        let _ = fs::create_dir_all(&fallback);
        fallback
    }
}

pub fn get_document_info(raw_path: &str) -> Result<DocumentInfoResponse, String> {
    let file_path = crate::files::normalize_user_path(PathBuf::from(raw_path.trim()));
    if !file_path.exists() || !file_path.is_file() {
        return Err(format!("Le document '{}' n'existe pas.", raw_path));
    }

    let meta = fs::metadata(&file_path)
        .map_err(|e| format!("Impossible de lire les métadonnées : {}", e))?;

    let name = file_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document")
        .to_string();

    let ext = file_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let is_pdf = ext == "pdf";
    let size_bytes = meta.len();
    let size_human = if size_bytes >= 1_073_741_824 {
        format!("{:.2} Go", size_bytes as f64 / 1_073_741_824.0)
    } else if size_bytes >= 1_048_576 {
        format!("{:.1} Mo", size_bytes as f64 / 1_048_576.0)
    } else if size_bytes >= 1024 {
        format!("{:.0} Ko", size_bytes as f64 / 1024.0)
    } else {
        format!("{} o", size_bytes)
    };

    let is_cached = if is_pdf {
        true
    } else {
        let mtime = meta.modified()
            .ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let mut hasher = DefaultHasher::new();
        file_path.display().to_string().hash(&mut hasher);
        mtime.hash(&mut hasher);
        size_bytes.hash(&mut hasher);
        let hash_key = format!("{:016x}", hasher.finish());
        let cached_file = get_cache_dir().join(format!("{}.pdf", hash_key));
        cached_file.exists() && fs::metadata(&cached_file).map(|m| m.len() > 0).unwrap_or(false)
    };

    Ok(DocumentInfoResponse {
        name,
        path: file_path.display().to_string(),
        size_bytes,
        size_human,
        extension: ext,
        is_pdf,
        is_cached,
    })
}

pub async fn get_document_pdf_path(raw_path: &str) -> Result<PathBuf, String> {
    let file_path = crate::files::normalize_user_path(PathBuf::from(raw_path.trim()));
    if !file_path.exists() || !file_path.is_file() {
        return Err(format!("Le document '{}' n'existe pas.", raw_path));
    }

    let ext = file_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if !matches!(
        ext.as_str(),
        "pdf" | "docx" | "doc" | "odt" | "rtf" | "xlsx" | "xls" | "ods" | "csv" | "pptx" | "ppt" | "odp"
    ) {
        return Err(format!("Format de document non pris en charge ('.{}').", ext));
    }

    // 1. Si le document est déjà un PDF natif, renvoi direct !
    if ext == "pdf" {
        return Ok(file_path);
    }

    // 2. Vérification du cache
    let meta = fs::metadata(&file_path)
        .map_err(|e| format!("Impossible d'accéder au fichier : {}", e))?;
    let mtime = meta.modified()
        .ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let size_bytes = meta.len();

    let mut hasher = DefaultHasher::new();
    file_path.display().to_string().hash(&mut hasher);
    mtime.hash(&mut hasher);
    size_bytes.hash(&mut hasher);
    let hash_key = format!("{:016x}", hasher.finish());

    let cache_dir = get_cache_dir();
    let cached_pdf = cache_dir.join(format!("{}.pdf", hash_key));

    // Cache HIT
    if cached_pdf.exists() && fs::metadata(&cached_pdf).map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(cached_pdf);
    }

    // 3. Cache MISS : conversion à la volée via LibreOffice headless
    let temp_job_id = format!("conv_{}_{}", std::process::id(), SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos());
    let temp_job_dir = cache_dir.join(&temp_job_id);
    fs::create_dir_all(&temp_job_dir)
        .map_err(|e| format!("Impossible de créer le dossier de conversion temporaire : {}", e))?;

    let profile_dir = temp_job_dir.join("profile");
    let profile_param = format!("-env:UserInstallation=file://{}", profile_dir.display());

    let mut cmd = tokio::process::Command::new("libreoffice");
    cmd.args([
        "--headless",
        "--invisible",
        "--nologo",
        "--nodefault",
        "--nofirststartwizard",
        &profile_param,
        "--convert-to",
        "pdf",
        file_path.to_str().unwrap_or(""),
        "--outdir",
        temp_job_dir.to_str().unwrap_or(""),
    ]);

    let output_res = cmd.output().await;

    let output = match output_res {
        Ok(out) => out,
        Err(e) => {
            let _ = fs::remove_dir_all(&temp_job_dir);
            return Err(format!("Impossible de démarrer LibreOffice ('libreoffice') : {}. Assurez-vous que libreoffice est installé sur le NAS.", e));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_dir_all(&temp_job_dir);
        let clean_err = stderr.lines().filter(|l| !l.is_empty()).collect::<Vec<&str>>().join(" ");
        return Err(format!("Échec de conversion LibreOffice : {}", if clean_err.is_empty() { format!("code {:?}", output.status.code()) } else { clean_err }));
    }

    // Recherche du fichier .pdf généré dans temp_job_dir
    let mut generated_file = None;
    if let Ok(entries) = fs::read_dir(&temp_job_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("pdf")).unwrap_or(false) {
                generated_file = Some(path);
                break;
            }
        }
    }

    match generated_file {
        Some(gen_path) => {
            // Déplacement vers le cache définitif
            if let Err(e) = fs::rename(&gen_path, &cached_pdf) {
                if let Err(ce) = fs::copy(&gen_path, &cached_pdf) {
                    let _ = fs::remove_dir_all(&temp_job_dir);
                    return Err(format!("Impossible d'enregistrer le PDF dans le cache : {} / {}", e, ce));
                }
            }
            let _ = fs::remove_dir_all(&temp_job_dir);
            Ok(cached_pdf)
        }
        None => {
            let _ = fs::remove_dir_all(&temp_job_dir);
            Err("La conversion LibreOffice a réussi mais aucun fichier PDF n'a été produit.".into())
        }
    }
}
