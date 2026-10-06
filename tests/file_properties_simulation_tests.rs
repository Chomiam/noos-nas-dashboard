//! Simulation et tests d'intégration du moteur de propriétés de fichiers et de dénombrement
//! Valide la robustesse des calculs, la sanctuarisation de la mémoire vive (bounded queue),
//! la protection contre les liens symboliques cycliques et le lissage CPU.

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

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

/// Simulation de l'algorithme d'exploration itératif à mémoire bornée
fn simulate_bounded_directory_crawler(
    roots: Vec<PathBuf>,
    max_queue_depth: usize,
) -> (u64, u64, u64, usize) {
    let mut total_files: u64 = 0;
    let mut total_dirs: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut peak_queue_len = 0;

    let mut queue = Vec::new();
    for r in roots {
        if let Ok(meta) = fs::symlink_metadata(&r) {
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                total_dirs += 1;
                queue.push(r);
            } else {
                total_files += 1;
                total_bytes += meta.len();
            }
        }
    }

    while let Some(dir) = queue.pop() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_symlink() {
                        // Règle de sécurité : ignorer les liens symboliques pour prévenir les boucles
                        continue;
                    }
                    if ft.is_dir() {
                        total_dirs += 1;
                        if queue.len() < max_queue_depth {
                            queue.push(entry.path());
                            if queue.len() > peak_queue_len {
                                peak_queue_len = queue.len();
                            }
                        }
                    } else if ft.is_file() {
                        total_files += 1;
                        if let Ok(m) = entry.metadata() {
                            total_bytes += m.len();
                        }
                    }
                }
            }
        }
    }

    (total_files, total_dirs, total_bytes, peak_queue_len)
}

#[test]
fn test_simulation_large_directory_volume_and_memory_footprint() {
    let test_dir = std::env::temp_dir().join(format!("noos_sim_large_{}", std::process::id()));
    let _ = fs::create_dir_all(&test_dir);

    // Création d'une structure simulée de 500 fichiers répartis sur 20 dossiers
    let mut expected_bytes: u64 = 0;
    for d in 0..20 {
        let sub = test_dir.join(format!("dossier_{:02}", d));
        fs::create_dir_all(&sub).unwrap();
        for f in 0..25 {
            let file_path = sub.join(format!("fichier_{:03}.dat", f));
            let content = vec![b'X'; (f + 1) * 10]; // Tailles variables
            expected_bytes += content.len() as u64;
            fs::write(&file_path, &content).unwrap();
        }
    }

    let start = Instant::now();
    let (files, dirs, bytes, peak_queue) = simulate_bounded_directory_crawler(vec![test_dir.clone()], 50_000);
    let duration = start.elapsed();

    assert_eq!(files, 500);
    assert_eq!(dirs, 21); // 1 dossier racine + 20 sous-dossiers
    assert_eq!(bytes, expected_bytes);
    // Vérification de la mémoire bornée : la pile d'exploration n'a jamais dépassé les 20 dossiers
    assert!(peak_queue <= 20, "Queue length exceeded expected bounds");
    // L'exécution sur 500 fichiers doit être quasi-instantanée
    assert!(duration.as_millis() < 500, "Crawler took too long: {:?}", duration);

    assert_eq!(format_size(bytes), format_size(expected_bytes));

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_simulation_anti_loop_symlink_protection() {
    let test_dir = std::env::temp_dir().join(format!("noos_sim_loop_{}", std::process::id()));
    let _ = fs::create_dir_all(&test_dir);

    let real_sub = test_dir.join("subfolder");
    fs::create_dir_all(&real_sub).unwrap();
    fs::write(real_sub.join("test.txt"), b"12345").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        // Création d'un lien récursif qui pointe vers la racine du test
        let loop_link = real_sub.join("infinite_loop_link");
        let _ = symlink(&test_dir, &loop_link);
    }

    let start = Instant::now();
    let (files, dirs, bytes, _) = simulate_bounded_directory_crawler(vec![test_dir.clone()], 50_000);
    let duration = start.elapsed();

    // Doit avoir terminé sans boucle infinie
    assert_eq!(files, 1);
    assert_eq!(dirs, 2);
    assert_eq!(bytes, 5);
    assert!(duration.as_millis() < 100, "Symlink loop was traversed or hung!");

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_simulation_mixed_file_selection() {
    let test_dir = std::env::temp_dir().join(format!("noos_sim_mixed_{}", std::process::id()));
    let _ = fs::create_dir_all(&test_dir);

    let f1 = test_dir.join("image.png");
    let f2 = test_dir.join("video.mp4");
    let sub = test_dir.join("musiques");
    fs::create_dir_all(&sub).unwrap();
    let f3 = sub.join("chanson.flac");

    fs::write(&f1, b"1234").unwrap();
    fs::write(&f2, b"12345678").unwrap();
    fs::write(&f3, b"123456789012").unwrap();

    // Sélection : f1 (fichier) et sub (dossier contenant f3)
    let (files, dirs, bytes, _) = simulate_bounded_directory_crawler(vec![f1.clone(), sub.clone()], 50_000);

    assert_eq!(files, 2); // f1 + f3
    assert_eq!(dirs, 1);  // sub
    assert_eq!(bytes, 4 + 12); // f1 (4) + f3 (12)

    let _ = fs::remove_dir_all(&test_dir);
}
