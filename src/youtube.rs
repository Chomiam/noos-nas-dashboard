use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeVideoInfo {
    pub id: String,
    pub title: String,
    pub thumbnail: String,
    pub duration_seconds: Option<u64>,
    pub duration_formatted: String,
    pub uploader: String,
    pub description: String,
    pub view_count: Option<u64>,
    pub view_count_formatted: Option<String>,
    pub default_filename: String,
    pub web_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YoutubeInfoRequest {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YoutubeDownloadRequest {
    pub url: String,
    #[serde(default = "default_format")]
    pub format: Option<String>, // "mp4" or "mp3"
    #[serde(default = "default_output_dir")]
    pub output_dir: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub custom_filename: Option<String>,
}

fn default_format() -> Option<String> {
    Some("mp4".to_string())
}

fn default_output_dir() -> Option<String> {
    let u = crate::updates::target_user();
    let home = crate::updates::get_user_home(&u);
    Some(home.join("videos").to_string_lossy().to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeJobStatus {
    pub id: String,
    pub url: String,
    pub title: String,
    pub format: String,
    pub output_dir: String,
    pub output_file: String,
    pub file_size: Option<String>,
    pub status: String, // "downloading" | "completed" | "error" | "cancelled"
    pub progress_percent: f32,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub error_message: Option<String>,
}

static YOUTUBE_JOBS: OnceLock<Arc<Mutex<HashMap<String, YoutubeJobStatus>>>> = OnceLock::new();
static YOUTUBE_CANCELS: OnceLock<Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>>> = OnceLock::new();

fn get_jobs_map() -> &'static Arc<Mutex<HashMap<String, YoutubeJobStatus>>> {
    YOUTUBE_JOBS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn get_cancels_map() -> &'static Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>> {
    YOUTUBE_CANCELS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn sanitize_filename(name: &str) -> String {
    let forbidden = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '%', '\'', '`', '$', ';', '\0', '\n', '\r', '\t'];
    let mut clean = String::with_capacity(name.len());
    for ch in name.chars() {
        if forbidden.contains(&ch) {
            clean.push('_');
        } else {
            clean.push(ch);
        }
    }

    let mut result = String::with_capacity(clean.len());
    let mut prev_underscore = false;
    for ch in clean.trim().chars() {
        if ch == '_' {
            if !prev_underscore {
                result.push('_');
                prev_underscore = true;
            }
        } else {
            result.push(ch);
            prev_underscore = false;
        }
    }

    let trimmed = result.trim_matches(|c: char| c == ' ' || c == '.' || c == '_' || c == '-');
    if trimmed.is_empty() {
        "video".to_string()
    } else {
        trimmed.chars().take(180).collect()
    }
}

pub async fn get_youtube_info(url: &str) -> Result<YoutubeVideoInfo, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("Veuillez fournir un lien YouTube valide.".into());
    }

    let mut cmd = Command::new("yt-dlp");
    cmd.args([
        "--dump-single-json",
        "--no-playlist",
        "--skip-download",
        "--no-warnings",
    ])
    .arg(url);

    let output = cmd.output().await.map_err(|e| format!("Impossible d'exécuter yt-dlp : {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let clean_err = stderr
            .lines()
            .filter(|l| l.contains("ERROR:"))
            .collect::<Vec<&str>>()
            .join(" ");

        if clean_err.contains("Video unavailable") || clean_err.contains("Private video") {
            return Err("Cette vidéo est indisponible ou privée.".into());
        } else if clean_err.contains("Incomplete YouTube ID") || clean_err.contains("is not a valid URL") {
            return Err("L'identifiant ou le lien YouTube est incorrect.".into());
        } else if !clean_err.is_empty() {
            return Err(format!("Erreur YouTube : {}", clean_err.replace("ERROR: ", "").trim()));
        } else {
            return Err("Impossible de récupérer les informations de cette vidéo. Vérifiez le lien.".into());
        }
    }

    let json_val: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Erreur d'analyse des métadonnées vidéo : {}", e))?;

    let id = json_val.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let title = json_val.get("title").and_then(|v| v.as_str()).unwrap_or("Sans titre").to_string();
    let thumbnail = json_val.get("thumbnail").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let uploader = json_val.get("uploader").and_then(|v| v.as_str()).unwrap_or("Inconnu").to_string();
    let description = json_val.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let duration_seconds = json_val.get("duration").and_then(|v| v.as_u64());

    let duration_formatted = if let Some(secs) = duration_seconds {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        if h > 0 {
            format!("{:02}:{:02}:{:02}", h, m, s)
        } else {
            format!("{:02}:{:02}", m, s)
        }
    } else {
        "--:--".to_string()
    };

    let view_count = json_val.get("view_count").and_then(|v| v.as_u64());
    let view_count_formatted = view_count.map(|c| {
        if c >= 1_000_000 {
            format!("{:.1}M vues", c as f64 / 1_000_000.0)
        } else if c >= 1_000 {
            format!("{:.1}k vues", c as f64 / 1_000.0)
        } else {
            format!("{} vues", c)
        }
    });

    let default_filename = sanitize_filename(&title);

    Ok(YoutubeVideoInfo {
        id,
        title,
        thumbnail,
        duration_seconds,
        duration_formatted,
        uploader,
        description,
        view_count,
        view_count_formatted,
        default_filename,
        web_url: url.to_string(),
    })
}

pub fn start_youtube_download(req: YoutubeDownloadRequest) -> Result<String, String> {
    let url = req.url.trim().to_string();
    if url.is_empty() {
        return Err("URL manquante.".into());
    }

    let format_str = req.format.unwrap_or_else(|| "mp4".into()).to_lowercase();
    let is_mp3 = format_str == "mp3";
    let ext = if is_mp3 { "mp3" } else { "mp4" };

    let u = crate::updates::target_user();
    let home = crate::updates::get_user_home(&u);
    let default_dir = if is_mp3 { home.join("musique").to_string_lossy().to_string() } else { home.join("videos").to_string_lossy().to_string() };
    let out_dir_raw = req.output_dir.unwrap_or(default_dir);
    let norm_dir = crate::files::normalize_user_path(PathBuf::from(out_dir_raw.trim()));
    if !norm_dir.exists() {
        fs::create_dir_all(&norm_dir).map_err(|e| format!("Impossible de créer le dossier de sortie : {}", e))?;
    }

    let raw_name = req.filename.or(req.custom_filename).unwrap_or_default();
    let sanitized = sanitize_filename(&raw_name);
    let base_name = if sanitized.ends_with(".mp4") || sanitized.ends_with(".mp3") {
        sanitized.rsplitn(2, '.').last().unwrap_or("video").to_string()
    } else {
        sanitized
    };

    let target_file_path = norm_dir.join(format!("{}.{}", base_name, ext));
    let job_id = format!("yt_{}_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis(), std::process::id());

    let job = YoutubeJobStatus {
        id: job_id.clone(),
        url: url.clone(),
        title: base_name.clone(),
        format: ext.to_string(),
        output_dir: norm_dir.display().to_string(),
        output_file: target_file_path.display().to_string(),
        file_size: None,
        status: "downloading".to_string(),
        progress_percent: 0.0,
        speed: None,
        eta: None,
        error_message: None,
    };

    let jobs_map = get_jobs_map();
    {
        let mut map = jobs_map.lock().unwrap();
        map.insert(job_id.clone(), job);
    }

    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut cmap = get_cancels_map().lock().unwrap();
        cmap.insert(job_id.clone(), cancel_tx);
    }

    let out_template = format!("{}/{}.%(ext)s", norm_dir.display(), base_name);
    let job_id_clone = job_id.clone();
    let jobs_map_clone = jobs_map.clone();
    let target_file_path_clone = target_file_path.clone();
    let norm_dir_clone = norm_dir.clone();
    let base_name_clone = base_name.clone();

    tokio::spawn(async move {
        let mut cmd = Command::new("yt-dlp");
        cmd.arg("--newline")
           .arg("--no-playlist")
           .arg("-o")
           .arg(&out_template);

        if is_mp3 {
            cmd.args([
                "-x",
                "--audio-format", "mp3",
                "--audio-quality", "0",
            ]);
        } else {
            // Qualité maximale vidéo + audio
            cmd.args([
                "-f", "bestvideo*+bestaudio/best",
                "--merge-output-format", "mp4",
            ]);
        }

        cmd.arg(&url);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let mut map = jobs_map_clone.lock().unwrap();
                if let Some(j) = map.get_mut(&job_id_clone) {
                    j.status = "error".to_string();
                    j.error_message = Some(format!("Échec de lancement de yt-dlp : {}", e));
                }
                return;
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let stdout_job_id = job_id_clone.clone();
        let stdout_map = jobs_map_clone.clone();

        let stdout_handle = tokio::spawn(async move {
            if let Some(out) = stdout {
                let mut reader = BufReader::new(out).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if line.contains("[download]") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        let mut pct: Option<f32> = None;
                        let mut spd: Option<String> = None;
                        let mut eta_val: Option<String> = None;

                        for (i, &p) in parts.iter().enumerate() {
                            if p.ends_with('%') {
                                if let Ok(val) = p.trim_end_matches('%').parse::<f32>() {
                                    pct = Some(val);
                                }
                            } else if p == "at" && i + 1 < parts.len() {
                                spd = Some(parts[i + 1].to_string());
                            } else if p == "ETA" && i + 1 < parts.len() {
                                eta_val = Some(parts[i + 1].to_string());
                            }
                        }

                        let mut map = stdout_map.lock().unwrap();
                        if let Some(j) = map.get_mut(&stdout_job_id) {
                            if let Some(val) = pct {
                                j.progress_percent = val;
                            }
                            if let Some(s) = spd {
                                j.speed = Some(s);
                            }
                            if let Some(e) = eta_val {
                                j.eta = Some(e);
                            }
                        }
                    }
                }
            }
        });

        let stderr_handle = tokio::spawn(async move {
            let mut err_lines = Vec::new();
            if let Some(err) = stderr {
                let mut reader = BufReader::new(err).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if line.contains("ERROR:") {
                        err_lines.push(line);
                    }
                }
            }
            err_lines.join(" ")
        });

        let (status_res, _, err_output) = tokio::join!(
            async {
                tokio::select! {
                    status = child.wait() => Ok(status),
                    _ = &mut cancel_rx => {
                        let _ = child.kill().await;
                        Err("cancelled")
                    }
                }
            },
            stdout_handle,
            stderr_handle
        );

        // Remove from cancels map
        {
            let mut cmap = get_cancels_map().lock().unwrap();
            cmap.remove(&job_id_clone);
        }

        let clean_err = err_output.unwrap_or_default();

        let mut map = jobs_map_clone.lock().unwrap();
        if let Some(j) = map.get_mut(&job_id_clone) {
            match status_res {
                Ok(Ok(s)) if s.success() => {
                    j.status = "completed".to_string();
                    j.progress_percent = 100.0;
                    j.speed = None;
                    j.eta = None;

                    // Taille du fichier
                    if let Ok(meta) = fs::metadata(&target_file_path_clone) {
                        let len = meta.len();
                        j.file_size = Some(if len >= 1_073_741_824 {
                            format!("{:.2} Go", len as f64 / 1_073_741_824.0)
                        } else if len >= 1_048_576 {
                            format!("{:.1} Mo", len as f64 / 1_048_576.0)
                        } else if len >= 1024 {
                            format!("{:.0} Ko", len as f64 / 1024.0)
                        } else {
                            format!("{} o", len)
                        });
                    }

                    // Fix permissions pour l'utilisateur principal
                    let u = crate::updates::target_user();
                    let _ = std::process::Command::new("chown")
                        .arg(format!("{}:users", u))
                        .arg(&target_file_path_clone)
                        .status();
                }
                Ok(Ok(s)) => {
                    j.status = "error".to_string();
                    let msg = if !clean_err.is_empty() {
                        clean_err.replace("ERROR: ", "").trim().to_string()
                    } else {
                        format!("Erreur lors du téléchargement (code: {:?})", s.code())
                    };
                    j.error_message = Some(msg);
                }
                Ok(Err(e)) => {
                    j.status = "error".to_string();
                    j.error_message = Some(format!("Erreur d'attente du téléchargement : {}", e));
                }
                Err("cancelled") => {
                    j.status = "cancelled".to_string();
                    j.speed = None;
                    j.eta = None;
                    j.error_message = Some("Téléchargement annulé par l'utilisateur.".to_string());

                    // Nettoyage des fichiers partiels ou temporaires (.part, .ytdl, etc.)
                    if let Ok(entries) = fs::read_dir(&norm_dir_clone) {
                        for entry in entries.flatten() {
                            let p = entry.path();
                            if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                                if fname.starts_with(&base_name_clone) && (fname.ends_with(".part") || fname.ends_with(".ytdl") || fname.ends_with(".temp")) {
                                    let _ = fs::remove_file(p);
                                }
                            }
                        }
                    }
                    let _ = fs::remove_file(&target_file_path_clone);
                }
                Err(_) => {}
            }
        }
    });

    Ok(job_id)
}

pub fn cancel_youtube_job(job_id: &str) -> Result<(), String> {
    let cancels = get_cancels_map();
    let sender = {
        let mut map = cancels.lock().unwrap();
        map.remove(job_id)
    };

    if let Some(tx) = sender {
        let _ = tx.send(());
        let jobs = get_jobs_map();
        let mut map = jobs.lock().unwrap();
        if let Some(j) = map.get_mut(job_id) {
            j.status = "cancelled".to_string();
            j.speed = None;
            j.eta = None;
            j.error_message = Some("Téléchargement annulé par l'utilisateur.".to_string());
        }
        Ok(())
    } else {
        let jobs = get_jobs_map();
        let map = jobs.lock().unwrap();
        if let Some(j) = map.get(job_id) {
            if j.status == "cancelled" {
                return Ok(());
            }
            return Err("Ce téléchargement n'est plus actif.".into());
        }
        Err("Tâche introuvable.".into())
    }
}

pub fn clear_youtube_jobs() {
    let cancels = get_cancels_map();
    let mut c_map = cancels.lock().unwrap();
    let jobs = get_jobs_map();
    let mut j_map = jobs.lock().unwrap();
    j_map.retain(|id, job| {
        if job.status == "downloading" {
            true
        } else {
            c_map.remove(id);
            false
        }
    });
}

pub fn get_job_status(job_id: &str) -> Option<YoutubeJobStatus> {
    let binding = get_jobs_map();
    let map = binding.lock().unwrap();
    map.get(job_id).cloned()
}

pub fn list_jobs() -> Vec<YoutubeJobStatus> {
    let binding = get_jobs_map();
    let map = binding.lock().unwrap();
    let mut list: Vec<YoutubeJobStatus> = map.values().cloned().collect();
    list.reverse();
    list
}
