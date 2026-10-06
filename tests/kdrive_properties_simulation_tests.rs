//! Batterie de tests de simulation et de robustesse pour l'évaluation des propriétés kDrive (Infomaniak API)
//! Valide la protection anti-rate-limit, le cache en mémoire vive, l'extraction multimédia,
//! la profondeur bornée, le plafond d'appels API et l'immunité contre les blocages HTTP 429.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct SimKDriveFile {
    id: u64,
    name: String,
    is_dir: bool,
    size: u64,
    mimetype: String,
    created_at: i64,
    updated_at: i64,
    parent_id: Option<u64>,
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<f64>,
    video_codec: Option<String>,
    audio_codec: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
struct SimMediaMetadata {
    is_media: bool,
    media_type: String,
    resolution: Option<String>,
    aspect_ratio: Option<String>,
    duration_seconds: Option<f64>,
    video_codec: Option<String>,
    audio_codec: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct SimFilePropertiesResponse {
    name: String,
    path: String,
    parent_path: Option<String>,
    is_dir: bool,
    size_bytes: u64,
    mime_type: String,
    owner: String,
    media_info: Option<SimMediaMetadata>,
}

fn calculate_aspect_ratio(w: u32, h: u32) -> Option<String> {
    if w == 0 || h == 0 {
        return None;
    }
    fn gcd(mut a: u32, mut b: u32) -> u32 {
        while b != 0 {
            let t = b;
            b = a % b;
            a = t;
        }
        a
    }
    let g = gcd(w, h);
    let rw = w / g;
    let rh = h / g;
    if rw == 8 && rh == 5 {
        Some("16:10".to_string())
    } else if (rw == 64 && rh == 27) || (rw == 43 && rh == 18) {
        Some("21:9".to_string())
    } else {
        Some(format!("{}:{}", rw, rh))
    }
}

/// Simulateur d'API Infomaniak avec comptage d'appels et détection de surchauffe
struct MockInfomaniakApi {
    files: HashMap<u64, SimKDriveFile>,
    children: HashMap<u64, Vec<u64>>,
    call_count: Mutex<usize>,
    rate_limit_threshold: usize,
}

impl MockInfomaniakApi {
    fn new(rate_limit_threshold: usize) -> Self {
        Self {
            files: HashMap::new(),
            children: HashMap::new(),
            call_count: Mutex::new(0),
            rate_limit_threshold,
        }
    }

    fn add_item(&mut self, item: SimKDriveFile) {
        let parent = item.parent_id.unwrap_or(1);
        self.children.entry(parent).or_default().push(item.id);
        self.files.insert(item.id, item);
    }

    fn get_file_meta(&self, file_id: u64) -> Result<SimKDriveFile, &'static str> {
        let mut count = self.call_count.lock().unwrap();
        *count += 1;
        if *count > self.rate_limit_threshold {
            return Err("HTTP 429 Too Many Requests");
        }
        self.files.get(&file_id).cloned().ok_or("File not found")
    }

    fn list_folder(&self, folder_id: u64) -> Result<Vec<SimKDriveFile>, &'static str> {
        let mut count = self.call_count.lock().unwrap();
        *count += 1;
        if *count > self.rate_limit_threshold {
            return Err("HTTP 429 Too Many Requests");
        }
        let child_ids = self.children.get(&folder_id).cloned().unwrap_or_default();
        let items: Vec<SimKDriveFile> = child_ids.into_iter().filter_map(|id| self.files.get(&id).cloned()).collect();
        Ok(items)
    }

    fn get_total_api_calls(&self) -> usize {
        *self.call_count.lock().unwrap()
    }
}

// --------------------------------------------------------------------------
// TEST 1 : Propriétés d'un fichier multimédia kDrive avec extraction de ratio & codecs
// --------------------------------------------------------------------------
#[test]
fn test_kdrive_multimedia_properties_and_aspect_ratio() {
    let mut api = MockInfomaniakApi::new(100);
    api.add_item(SimKDriveFile {
        id: 42,
        name: "vacances_corse_4k.mp4".to_string(),
        is_dir: false,
        size: 1_540_000_000,
        mimetype: "video/mp4".to_string(),
        created_at: 1720000000,
        updated_at: 1720001000,
        parent_id: Some(1),
        width: Some(3840),
        height: Some(2160),
        duration: Some(125.5),
        video_codec: Some("H264".to_string()),
        audio_codec: Some("AAC".to_string()),
    });

    let raw = api.get_file_meta(42).expect("Metadata retrieval failed");
    let aspect = calculate_aspect_ratio(raw.width.unwrap(), raw.height.unwrap());

    assert_eq!(aspect, Some("16:9".to_string()));
    assert_eq!(raw.name, "vacances_corse_4k.mp4");
    assert_eq!(raw.size, 1_540_000_000);
    assert_eq!(raw.video_codec.as_deref(), Some("H264"));
    assert_eq!(raw.audio_codec.as_deref(), Some("AAC"));
    assert_eq!(raw.duration, Some(125.5));
}

// --------------------------------------------------------------------------
// TEST 2 : Ratio d'aspect pour résolutions variées (UltraWide, 16:10, 4:3, Vertical)
// --------------------------------------------------------------------------
#[test]
fn test_kdrive_aspect_ratio_permutations() {
    assert_eq!(calculate_aspect_ratio(1920, 1080), Some("16:9".to_string()));
    assert_eq!(calculate_aspect_ratio(3840, 2160), Some("16:9".to_string()));
    assert_eq!(calculate_aspect_ratio(2560, 1440), Some("16:9".to_string()));
    assert_eq!(calculate_aspect_ratio(1920, 1200), Some("16:10".to_string()));
    assert_eq!(calculate_aspect_ratio(2560, 1600), Some("16:10".to_string()));
    assert_eq!(calculate_aspect_ratio(1024, 768), Some("4:3".to_string()));
    assert_eq!(calculate_aspect_ratio(3440, 1440), Some("21:9".to_string()));
    assert_eq!(calculate_aspect_ratio(1080, 1920), Some("9:16".to_string()));
    assert_eq!(calculate_aspect_ratio(0, 1080), None);
}

// --------------------------------------------------------------------------
// TEST 3 : Cache TTL en mémoire - Les requêtes répétées ne consomment AUCUN appel API
// --------------------------------------------------------------------------
#[test]
fn test_kdrive_ttl_cache_prevents_api_burn() {
    let mut api = MockInfomaniakApi::new(5);
    api.add_item(SimKDriveFile {
        id: 101,
        name: "document_confidentiel.pdf".to_string(),
        is_dir: false,
        size: 540_200,
        mimetype: "application/pdf".to_string(),
        created_at: 1720000000,
        updated_at: 1720001000,
        parent_id: Some(1),
        width: None,
        height: None,
        duration: None,
        video_codec: None,
        audio_codec: None,
    });

    let cache: Mutex<HashMap<u64, (Instant, SimFilePropertiesResponse)>> = Mutex::new(HashMap::new());

    let get_with_cache = |file_id: u64| -> SimFilePropertiesResponse {
        {
            let guard = cache.lock().unwrap();
            if let Some((instant, ref props)) = guard.get(&file_id) {
                if instant.elapsed() < Duration::from_secs(60) {
                    return props.clone();
                }
            }
        }

        // Cache miss -> appel API
        let meta = api.get_file_meta(file_id).unwrap();
        let props = SimFilePropertiesResponse {
            name: meta.name,
            path: format!("kdrive://acc_123/{}", file_id),
            parent_path: Some("kdrive://acc_123/1".to_string()),
            is_dir: meta.is_dir,
            size_bytes: meta.size,
            mime_type: meta.mimetype,
            owner: "kDrive (Steve)".to_string(),
            media_info: None,
        };

        let mut guard = cache.lock().unwrap();
        guard.insert(file_id, (Instant::now(), props.clone()));
        props
    };

    // Premier appel : Cache miss (1 appel API)
    let p1 = get_with_cache(101);
    assert_eq!(p1.name, "document_confidentiel.pdf");
    assert_eq!(api.get_total_api_calls(), 1);

    // 100 requêtes consécutives sur le même fichier (ex: rafraîchissements multiples, double clic, modale)
    for _ in 0..100 {
        let p = get_with_cache(101);
        assert_eq!(p.size_bytes, 540_200);
    }

    // Le nombre d'appels API doit être RESTÉ STRICTEMENT À 1 !
    assert_eq!(api.get_total_api_calls(), 1, "Cache hit failed to prevent redundant API calls!");
}

// --------------------------------------------------------------------------
// TEST 4 : Pacing anti-burst respectant un délai minimal entre requêtes
// --------------------------------------------------------------------------
#[test]
fn test_kdrive_rate_pacer_timing_guarantee() {
    let last_req: Mutex<Option<Instant>> = Mutex::new(None);
    let min_interval = Duration::from_millis(20);

    let pace_request = || {
        let mut guard = last_req.lock().unwrap();
        let now = Instant::now();
        if let Some(last) = *guard {
            let elapsed = now.duration_since(last);
            if elapsed < min_interval {
                std::thread::sleep(min_interval - elapsed);
            }
        }
        *guard = Some(Instant::now());
    };

    let start = Instant::now();
    for _ in 0..5 {
        pace_request();
    }
    let total_elapsed = start.elapsed();

    // 4 intervalles d'au moins 20ms = ~80ms minimum
    assert!(total_elapsed >= Duration::from_millis(75), "Pacer did not enforce minimum duration: {:?}", total_elapsed);
}

// --------------------------------------------------------------------------
// TEST 5 : Crawler récursif avec seuil d'appels (MAX_API_CALLS) protégeant le quota
// --------------------------------------------------------------------------
#[test]
fn test_kdrive_crawler_api_call_quota_capping() {
    let mut api = MockInfomaniakApi::new(500);

    // Création d'une arborescence profonde de 50 dossiers
    for i in 1..=50 {
        let child_folder = i + 1;
        api.add_item(SimKDriveFile {
            id: child_folder,
            name: format!("sous_dossier_{}", child_folder),
            is_dir: true,
            size: 0,
            mimetype: "inode/directory".to_string(),
            created_at: 1720000000,
            updated_at: 1720001000,
            parent_id: Some(i),
            width: None,
            height: None,
            duration: None,
            video_codec: None,
            audio_codec: None,
        });
        // Ajouter 2 fichiers par dossier
        api.add_item(SimKDriveFile {
            id: 1000 + i,
            name: format!("file_{}.txt", i),
            is_dir: false,
            size: 100,
            mimetype: "text/plain".to_string(),
            created_at: 1720000000,
            updated_at: 1720001000,
            parent_id: Some(child_folder),
            width: None,
            height: None,
            duration: None,
            video_codec: None,
            audio_codec: None,
        });
    }

    // Crawler borné à 15 appels API maximum
    let max_api_calls = 15;
    let mut api_calls = 0;
    let mut queue = vec![1u64];
    let mut total_files = 0;
    let mut total_bytes = 0;
    let mut total_dirs = 0;
    let mut quota_exceeded_notice = false;

    while let Some(folder_id) = queue.pop() {
        if api_calls >= max_api_calls {
            quota_exceeded_notice = true;
            break;
        }

        api_calls += 1;
        if let Ok(entries) = api.list_folder(folder_id) {
            for entry in entries {
                if entry.is_dir {
                    total_dirs += 1;
                    queue.push(entry.id);
                } else {
                    total_files += 1;
                    total_bytes += entry.size;
                }
            }
        }
    }

    assert!(quota_exceeded_notice, "Quota limit notice should have been triggered");
    assert_eq!(api_calls, max_api_calls);
    assert!(total_files > 0);
    assert!(total_dirs > 0);
    assert!(total_bytes > 0);
}

// --------------------------------------------------------------------------
// TEST 6 : Détection et absorption sans panique d'une erreur HTTP 429
// --------------------------------------------------------------------------
#[test]
fn test_kdrive_resilience_on_http_429_too_many_requests() {
    // API qui déclenche un 429 après seulement 3 requêtes
    let mut api = MockInfomaniakApi::new(3);
    for i in 1..=10 {
        api.add_item(SimKDriveFile {
            id: i,
            name: format!("dossier_{}", i),
            is_dir: true,
            size: 0,
            mimetype: "inode/directory".to_string(),
            created_at: 1720000000,
            updated_at: 1720001000,
            parent_id: Some(1),
            width: None,
            height: None,
            duration: None,
            video_codec: None,
            audio_codec: None,
        });
    }

    let mut queue = vec![1u64];
    let mut error_recorded = None;
    let mut successful_calls = 0;

    while let Some(folder_id) = queue.pop() {
        match api.list_folder(folder_id) {
            Ok(entries) => {
                successful_calls += 1;
                for e in entries {
                    if e.is_dir {
                        queue.push(e.id);
                    }
                }
            }
            Err(e) => {
                if e.contains("429") {
                    error_recorded = Some("Limite de requêtes kDrive atteinte (HTTP 429). Pause de protection activée.".to_string());
                    // Arrêt préventif pour protéger le compte
                    break;
                }
            }
        }
    }

    assert_eq!(successful_calls, 3);
    assert!(error_recorded.is_some(), "429 Error should have been safely caught and reported");
    assert!(error_recorded.unwrap().contains("HTTP 429"));
}

// --------------------------------------------------------------------------
// TEST 7 : Annulation coopérative immédiate via token d'annulation (is_cancelled)
// --------------------------------------------------------------------------
#[test]
fn test_kdrive_crawler_cancellation_token() {
    let is_cancelled = Arc::new(AtomicBool::new(false));
    let total_dirs = Arc::new(AtomicU64::new(0));

    let ic = Arc::clone(&is_cancelled);
    let td = Arc::clone(&total_dirs);

    let handle = std::thread::spawn(move || {
        for _ in 0..1000 {
            if ic.load(Ordering::Relaxed) {
                break;
            }
            td.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(5));
        }
    });

    // Laisser tourner 20ms puis annuler
    std::thread::sleep(Duration::from_millis(20));
    is_cancelled.store(true, Ordering::Relaxed);

    handle.join().unwrap();

    let processed = total_dirs.load(Ordering::Relaxed);
    assert!(processed < 20, "Crawler did not stop immediately upon cancellation: processed {}", processed);
}

// --------------------------------------------------------------------------
// TEST 8 : Sélection mixte (fichiers locaux + fichiers distants kDrive)
// --------------------------------------------------------------------------
#[test]
fn test_mixed_local_and_kdrive_selection_stats() {
    let local_files = vec![("local_1.pdf", 1000u64), ("local_2.png", 2500u64)];
    let kdrive_files = vec![("kdrive_video.mp4", 50_000_000u64), ("kdrive_doc.docx", 30_000u64)];

    let total_files_count = local_files.len() + kdrive_files.len();
    let local_bytes: u64 = local_files.iter().map(|(_, sz)| *sz).sum();
    let kd_bytes: u64 = kdrive_files.iter().map(|(_, sz)| *sz).sum();
    let total_bytes = local_bytes + kd_bytes;

    assert_eq!(total_files_count, 4);
    assert_eq!(total_bytes, 3500 + 50_030_000);
}
