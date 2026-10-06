use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// Test 1: Validation de la sérialisation et du masquage de la clé API Immich
#[test]
fn test_immich_api_key_masking() {
    let empty = "";
    assert_eq!(mask_key(empty), "");

    let short = "1234";
    assert_eq!(mask_key(short), "••••••••");

    let normal = "c2e8a1d7f6b490e5";
    let masked = mask_key(normal);
    assert!(masked.starts_with("c2e8"));
    assert!(masked.ends_with("90e5"));
    assert!(masked.contains("••••••••"));
}

fn mask_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.len() <= 8 {
        "••••••••".to_string()
    } else {
        let prefix: String = trimmed.chars().take(4).collect();
        let suffix: String = trimmed.chars().rev().take(4).collect::<String>().chars().rev().collect();
        format!("{}••••••••{}", prefix, suffix)
    }
}

// Test 2: Construction des arguments CLI Immich-Go pour les 3 cas d'usage
#[test]
fn test_immich_go_cli_arguments_generation() {
    let server_url = "http://localhost:2283";
    let api_key = "secret_key_12345";

    // Cas 1 : Google Photos Takeout
    let takeout_path = "/home/chomiam/takeout-2026.zip";
    let album = Some("Vacances 2026");
    let args_takeout = build_cli_args("takeout", takeout_path, server_url, api_key, album);
    assert_eq!(args_takeout[0], "upload");
    assert_eq!(args_takeout[1], "-server");
    assert_eq!(args_takeout[2], server_url);
    assert_eq!(args_takeout[3], "-key");
    assert_eq!(args_takeout[4], api_key);
    assert!(args_takeout.contains(&"-google-photos".to_string()));
    assert!(args_takeout.contains(&"-yes".to_string()));
    assert!(args_takeout.contains(&"-album".to_string()));
    assert!(args_takeout.contains(&"Vacances 2026".to_string()));
    assert_eq!(args_takeout.last().unwrap(), takeout_path);

    // Cas 2 : Dossier local NAS
    let nas_path = "/home/chomiam/Photos";
    let args_nas = build_cli_args("nas", nas_path, server_url, api_key, None);
    assert!(args_nas.contains(&"-recursive".to_string()));
    assert!(!args_nas.contains(&"-google-photos".to_string()));
    assert_eq!(args_nas.last().unwrap(), nas_path);

    // Cas 3 : kDrive (traitement par lot d'éléments en staging)
    let staged_file = "/tmp/noos_immich_kdrive_staged/job_1/photo.jpg";
    let args_kdrive = build_cli_args("kdrive", staged_file, server_url, api_key, None);
    assert!(args_kdrive.contains(&"-yes".to_string()));
    assert_eq!(args_kdrive.last().unwrap(), staged_file);
}

fn build_cli_args(
    source_type: &str,
    target_path: &str,
    server_url: &str,
    api_key: &str,
    album: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "upload".to_string(),
        "-server".to_string(),
        server_url.to_string(),
        "-key".to_string(),
        api_key.to_string(),
    ];

    if source_type == "takeout" {
        args.push("-google-photos".to_string());
    } else if source_type == "nas" {
        args.push("-recursive".to_string());
    }

    args.push("-yes".to_string());

    if let Some(alb) = album {
        if !alb.trim().is_empty() {
            args.push("-album".to_string());
            args.push(alb.trim().to_string());
        }
    }

    args.push(target_path.to_string());
    args
}

// Test 3: Simulation de la détection des extensions multimédia
#[test]
fn test_multimedia_extensions_filtering() {
    let valid_media = ["test.jpg", "photo.HEIC", "RAW_IMAGE.DNG", "CLIP.mp4", "movie.MKV", "takeout.zip"];
    let invalid_media = ["document.pdf", "notes.txt", "archive.iso", "script.sh", "style.css"];

    for f in &valid_media {
        let ext = PathBuf::from(f).extension().unwrap().to_str().unwrap().to_string();
        assert!(is_media_ext(&ext), "L'extension {} devrait être reconnue comme média", ext);
    }

    for f in &invalid_media {
        let ext = PathBuf::from(f).extension().unwrap().to_str().unwrap().to_string();
        assert!(!is_media_ext(&ext), "L'extension {} ne devrait pas être reconnue comme média", ext);
    }
}

fn is_media_ext(ext: &str) -> bool {
    let lower = ext.to_lowercase();
    matches!(
        lower.as_str(),
        "jpg" | "jpeg" | "png" | "heic" | "heif" | "webp" | "gif" | "tiff" | "tif" | "bmp"
            | "svg" | "raw" | "arw" | "cr2" | "cr3" | "nef" | "dng" | "orf" | "rw2" | "pef"
            | "raf" | "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" | "wmv" | "flv" | "3gp"
            | "ts" | "mts" | "m2ts" | "zip" | "tar" | "tgz"
    )
}

// Test 4: Simulation de collecte des échecs et filtre de retry
#[derive(Debug, Clone, PartialEq)]
struct MockFailedItem {
    pub filename: String,
    pub path_or_uri: String,
    pub reason: String,
    pub retryable: bool,
}

#[test]
fn test_failed_items_tracking_and_retry_filtering() {
    let mut failures = Vec::new();

    failures.push(MockFailedItem {
        filename: "corrupt_video.mp4".into(),
        path_or_uri: "/nas/photos/corrupt_video.mp4".into(),
        reason: "Format corrompu".into(),
        retryable: true,
    });

    failures.push(MockFailedItem {
        filename: "unsupported.exe".into(),
        path_or_uri: "/nas/photos/unsupported.exe".into(),
        reason: "Format non pris en charge".into(),
        retryable: false,
    });

    failures.push(MockFailedItem {
        filename: "timeout_photo.jpg".into(),
        path_or_uri: "kdrive://account1/12345".into(),
        reason: "Timeout réseau kDrive HTTP 504".into(),
        retryable: true,
    });

    let retryable_items: Vec<_> = failures.into_iter().filter(|i| i.retryable).collect();
    assert_eq!(retryable_items.len(), 2);
    assert_eq!(retryable_items[0].filename, "corrupt_video.mp4");
    assert_eq!(retryable_items[1].filename, "timeout_photo.jpg");
}

// Test 5: Simulation du verrouillage des mises à jour système pendant l'import
#[test]
fn test_system_update_lockout_during_import() {
    let immich_active = Arc::new(AtomicBool::new(false));

    // Lorsque l'import n'est pas actif, la mise à jour peut démarrer
    let can_update_before = !immich_active.load(Ordering::SeqCst);
    assert!(can_update_before, "La mise à jour devrait être autorisée quand aucun import n'est actif");

    // L'import démarre
    immich_active.store(true, Ordering::SeqCst);
    let can_update_during = !immich_active.load(Ordering::SeqCst);
    assert!(!can_update_during, "La mise à jour doit être strictement verrouillée quand un import Immich est en cours");

    // L'import se termine ou est annulé
    immich_active.store(false, Ordering::SeqCst);
    let can_update_after = !immich_active.load(Ordering::SeqCst);
    assert!(can_update_after, "La mise à jour doit être à nouveau autorisée après la fin de l'import");
}

// Test 6: Simulation du nettoyage de staging kDrive lors d'une annulation
#[test]
fn test_kdrive_staging_cleanup_on_cancellation() {
    let temp_base = PathBuf::from("/tmp/noos_test_immich_staging_clean");
    let job_dir = temp_base.join("job_test_clean_123");
    let _ = fs::create_dir_all(&job_dir);
    let staged_file = job_dir.join("photo_test.jpg");
    let _ = fs::write(&staged_file, b"FAKE_PHOTO_BYTES");

    assert!(staged_file.exists());

    // Simulation de l'annulation
    let cancelled = true;
    if cancelled && job_dir.exists() {
        let _ = fs::remove_dir_all(&job_dir);
    }

    assert!(!job_dir.exists(), "Le dossier temporaire de staging kDrive doit être purgé lors de l'annulation");
    let _ = fs::remove_dir_all(&temp_base);
}

// Test 7: Simulation et logique d'évaluation d'espace disque Takeout (Archive + Décompression 1.4x)
#[test]
fn test_takeout_disk_space_estimation_logic() {
    let archive_bytes = 10 * 1024 * 1024 * 1024; // 10 Go
    let decompressed_estimate_bytes = (archive_bytes as f64 * 1.4).round() as u64; // 14 Go
    let required_total_bytes = archive_bytes + decompressed_estimate_bytes; // 24 Go
    assert_eq!(required_total_bytes, 24 * 1024 * 1024 * 1024);

    // Cas suffisant (30 Go disponibles)
    let available_sufficient = 30 * 1024 * 1024 * 1024;
    let is_sufficient = available_sufficient >= required_total_bytes;
    assert!(is_sufficient, "L'espace doit être déclaré suffisant quand available >= required");

    // Cas insuffisant (15 Go disponibles)
    let available_insufficient = 15 * 1024 * 1024 * 1024;
    let is_insufficient = available_insufficient >= required_total_bytes;
    assert!(!is_insufficient, "L'espace doit être déclaré insuffisant quand available < required");

    let missing = required_total_bytes.saturating_sub(available_insufficient);
    assert_eq!(missing, 9 * 1024 * 1024 * 1024, "Le calcul du manque doit être exact (24 - 15 = 9 Go)");
}

// Test 8: Simulation de la suppression sécurisée d'archives Takeout & filtrage de sécurité
#[test]
fn test_takeout_archive_deletion_and_safety_checks() {
    let temp_base = PathBuf::from("/tmp/noos_test_archive_clean");
    let _ = fs::create_dir_all(&temp_base);

    let zip_file = temp_base.join("takeout-2026-part1.zip");
    fs::write(&zip_file, vec![0u8; 1024 * 50]).unwrap(); // 50 Ko

    let targz_file = temp_base.join("backup.tar.gz");
    fs::write(&targz_file, vec![0u8; 1024 * 30]).unwrap(); // 30 Ko

    let dangerous_file = temp_base.join("important_document.pdf");
    fs::write(&dangerous_file, vec![0u8; 1024 * 10]).unwrap(); // 10 Ko

    // Dossier dédié nommé 'takeout'
    let takeout_subfolder = temp_base.join("takeout");
    fs::create_dir_all(&takeout_subfolder).unwrap();
    let sub_file = takeout_subfolder.join("photo.jpg");
    fs::write(&sub_file, vec![0u8; 1024 * 20]).unwrap(); // 20 Ko

    // Validation des extensions autorisées
    let check_is_safe = |path: &std::path::Path| -> bool {
        let lower = path.to_string_lossy().to_lowercase();
        let is_archive = lower.ends_with(".zip") || lower.ends_with(".tgz") || lower.ends_with(".tar.gz") || lower.ends_with(".tar");
        let is_takeout = lower.ends_with("/takeout") || lower.contains("/takeout/") || lower.ends_with("\\takeout") || lower.contains("\\takeout\\");
        is_archive || is_takeout
    };

    assert!(check_is_safe(&zip_file), "Le fichier .zip doit être autorisé à la suppression");
    assert!(check_is_safe(&targz_file), "Le fichier .tar.gz doit être autorisé à la suppression");
    assert!(check_is_safe(&takeout_subfolder), "Le dossier takeout doit être autorisé à la suppression");
    assert!(!check_is_safe(&dangerous_file), "Un document .pdf en dehors d'un dossier takeout doit être rejeté par sécurité");

    // Suppression effective des archives autorisées
    let mut freed_bytes = 0u64;
    if check_is_safe(&zip_file) && zip_file.is_file() {
        freed_bytes += fs::metadata(&zip_file).unwrap().len();
        fs::remove_file(&zip_file).unwrap();
    }
    if check_is_safe(&targz_file) && targz_file.is_file() {
        freed_bytes += fs::metadata(&targz_file).unwrap().len();
        fs::remove_file(&targz_file).unwrap();
    }
    if check_is_safe(&takeout_subfolder) && takeout_subfolder.is_dir() {
        freed_bytes += 1024 * 20; // taille de sub_file
        fs::remove_dir_all(&takeout_subfolder).unwrap();
    }

    assert_eq!(freed_bytes, 1024 * 100, "Le total des octets libérés doit être de 100 Ko (50 + 30 + 20)");
    assert!(!zip_file.exists());
    assert!(!targz_file.exists());
    assert!(!takeout_subfolder.exists());
    assert!(dangerous_file.exists(), "Le fichier PDF non ciblé ne doit jamais avoir été touché");

    let _ = fs::remove_dir_all(&temp_base);
}

