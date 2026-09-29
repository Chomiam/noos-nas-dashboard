use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckStatus {
    pub config_update_available: bool,
    pub config_local_commit: String,
    pub config_remote_commit: Option<String>,
    pub config_commit_message: Option<String>,
    pub package_updates_available: bool,
    pub package_updates_count: u32,
    pub package_details: Vec<String>,
    pub update_type: UpdateType, // "None", "ConfigOnly", "PackagesOnly", "Both"
    pub status_text: String,
    pub config_dir: String,
    pub last_checked: String,
    pub is_updating: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateType {
    None,
    ConfigOnly,
    PackagesOnly,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyUpdateResult {
    pub success: bool,
    pub steps_executed: Vec<String>,
    pub output_log: String,
    pub error: Option<String>,
}

static UPDATE_CACHE: Mutex<Option<(Instant, UpdateCheckStatus)>> = Mutex::new(None);
static IS_UPDATING: AtomicBool = AtomicBool::new(false);

pub fn is_updating() -> bool {
    IS_UPDATING.load(Ordering::SeqCst)
}

pub fn resolve_config_dir() -> PathBuf {
    if let Ok(dir) = env::var("STEVEOS_CONFIG_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return p;
        }
    }

    let candidate1 = PathBuf::from("/home/chomiam/Projects/steveos-nas");
    if candidate1.exists() {
        return candidate1;
    }

    let candidate2 = PathBuf::from("/etc/nixos");
    if candidate2.exists() {
        return candidate2;
    }

    PathBuf::from(".")
}

pub fn check_updates(force_refresh: bool) -> UpdateCheckStatus {
    if !force_refresh {
        if let Ok(guard) = UPDATE_CACHE.lock() {
            if let Some((instant, ref cached)) = *guard {
                if instant.elapsed() < Duration::from_secs(30) {
                    let mut res = cached.clone();
                    res.is_updating = is_updating();
                    return res;
                }
            }
        }
    }

    let config_dir = resolve_config_dir();
    let config_dir_str = config_dir.display().to_string();

    // 1. Vérification de la configuration Git (Chomiam/steve_os-nix)
    let local_commit = Command::new("git")
        .args(["-C", &config_dir_str, "rev-parse", "--short", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "inconnu".to_string());

    let full_local_commit = Command::new("git")
        .args(["-C", &config_dir_str, "rev-parse", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let mut config_update_available = false;
    let mut config_remote_commit = None;
    let mut config_commit_message = None;

    if let Ok(out) = Command::new("git")
        .args(["-C", &config_dir_str, "ls-remote", "origin", "refs/heads/main"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(token) = text.split_whitespace().next() {
                let short_remote = token[..7.min(token.len())].to_string();
                config_remote_commit = Some(short_remote);

                if !full_local_commit.is_empty() && token != full_local_commit {
                    let is_ancestor = Command::new("git")
                        .args(["-C", &config_dir_str, "merge-base", "--is-ancestor", token, "HEAD"])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);

                    if !is_ancestor {
                        config_update_available = true;
                        config_commit_message = Some("Nouvelle révision disponible sur GitHub (steve_os-nix)".into());
                    }
                }
            }
        }
    }

    // 2. Vérification des paquets Nixpkgs dans flake.lock
    let mut package_updates_available = false;
    let mut package_updates_count = 0;
    let mut package_details = Vec::new();

    let lock_path = config_dir.join("flake.lock");
    if let Ok(lock_str) = fs::read_to_string(&lock_path) {
        if let Ok(lock_json) = serde_json::from_str::<serde_json::Value>(&lock_str) {
            if let Some(nodes) = lock_json.get("nodes").and_then(|n| n.as_object()) {
                if let Some(nixpkgs_node) = nodes.get("nixpkgs") {
                    let locked_rev = nixpkgs_node
                        .get("locked")
                        .and_then(|l| l.get("rev"))
                        .and_then(|r| r.as_str())
                        .unwrap_or("");

                    if !locked_rev.is_empty() {
                        let ref_branch = nixpkgs_node
                            .get("original")
                            .and_then(|o| o.get("ref"))
                            .and_then(|r| r.as_str())
                            .unwrap_or("nixos-26.05");

                        // Vérifier le dernier commit du canal nixpkgs sur GitHub
                        if let Ok(ls_out) = Command::new("git")
                            .args(["ls-remote", "https://github.com/nixos/nixpkgs", &format!("refs/heads/{}", ref_branch)])
                            .output()
                        {
                            if ls_out.status.success() {
                                let ls_text = String::from_utf8_lossy(&ls_out.stdout);
                                if let Some(remote_sha) = ls_text.split_whitespace().next() {
                                    if remote_sha != locked_rev {
                                        package_updates_available = true;
                                        package_updates_count += 1;
                                        package_details.push(format!("Nixpkgs ({}) : mise à jour vers {}", ref_branch, &remote_sha[..7]));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Détermination du type d'action requise
    let update_type = match (config_update_available, package_updates_available) {
        (true, true) => UpdateType::Both,
        (true, false) => UpdateType::ConfigOnly,
        (false, true) => UpdateType::PackagesOnly,
        (false, false) => UpdateType::None,
    };

    let status_text = match update_type {
        UpdateType::Both => "⚡ Nouvelle configuration ET paquets disponibles !".to_string(),
        UpdateType::ConfigOnly => "📥 Nouvelle configuration disponible sur GitHub".to_string(),
        UpdateType::PackagesOnly => "📦 Mises à jour de paquets système disponibles".to_string(),
        UpdateType::None => "✨ Système et configuration à jour".to_string(),
    };

    let status = UpdateCheckStatus {
        config_update_available,
        config_local_commit: local_commit,
        config_remote_commit,
        config_commit_message,
        package_updates_available,
        package_updates_count,
        package_details,
        update_type,
        status_text,
        config_dir: config_dir_str,
        last_checked: current_time_formatted(),
        is_updating: is_updating(),
    };

    if let Ok(mut guard) = UPDATE_CACHE.lock() {
        *guard = Some((Instant::now(), status.clone()));
    }

    status
}

pub fn execute_secure_git_pull(config_dir: &Path, log: &mut String) -> Result<(), String> {
    log.push_str("--- [Étape 1/3] Sécurisation de l'espace de travail local ---\n");

    let dir_str = config_dir.display().to_string();

    // 1. Sauvegarde automatique du commit courant
    let current_sha = Command::new("git")
        .args(["-C", &dir_str, "rev-parse", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    log.push_str(&format!("Point de restauration courant : {}\n", &current_sha[..8.min(current_sha.len())]));

    // 2. Vérifier si des fichiers modifiés localement existent
    let status_out = Command::new("git")
        .args(["-C", &dir_str, "status", "--porcelain"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let has_uncommitted = !status_out.is_empty();
    if has_uncommitted {
        log.push_str("Modifications locales détectées. Création d'un stash de sécurité...\n");
        let stash_res = Command::new("git")
            .args(["-C", &dir_str, "stash", "push", "-u", "-m", "steveos-auto-stash"])
            .output();

        if let Ok(res) = stash_res {
            log.push_str(&String::from_utf8_lossy(&res.stdout));
        }
    }

    // 3. Fetch et merge sécurisé (fast-forward privilégié)
    log.push_str("\n--- [Étape 2/3] Récupération des nouveautés depuis GitHub (steve_os-nix) ---\n");
    let fetch_out = Command::new("git")
        .args(["-C", &dir_str, "fetch", "origin", "main"])
        .output()
        .map_err(|e| format!("Échec git fetch : {}", e))?;

    if !fetch_out.status.success() {
        let err = String::from_utf8_lossy(&fetch_out.stderr);
        return Err(format!("Erreur lors de la récupération distante : {}", err));
    }

    let merge_out = Command::new("git")
        .args(["-C", &dir_str, "merge", "--ff-only", "origin/main"])
        .output()
        .map_err(|e| format!("Échec du merge fast-forward : {}", e))?;

    if !merge_out.status.success() {
        // En cas d'échec fast-forward, tenter un rebase propre
        log.push_str("Merge fast-forward non direct. Tentative de rebase automatique...\n");
        let rebase_out = Command::new("git")
            .args(["-C", &dir_str, "rebase", "origin/main"])
            .output();

        match rebase_out {
            Ok(r) if r.status.success() => {
                log.push_str("Rebase réussi avec succès.\n");
            }
            _ => {
                // Annuler le rebase si conflit
                let _ = Command::new("git").args(["-C", &dir_str, "rebase", "--abort"]).output();
                if has_uncommitted {
                    let _ = Command::new("git").args(["-C", &dir_str, "stash", "pop"]).output();
                }
                return Err("Conflit Git détecté avec la branche distante. Opération annulée pour préserver vos fichiers.".into());
            }
        }
    } else {
        log.push_str(&String::from_utf8_lossy(&merge_out.stdout));
    }

    // 4. Restaurer le stash si existant
    if has_uncommitted {
        log.push_str("Restauration de vos modifications locales...\n");
        let _ = Command::new("git").args(["-C", &dir_str, "stash", "pop"]).output();
    }

    // 5. Validation de la syntaxe Nix (nix eval de sécurité)
    log.push_str("\n--- [Étape 3/3] Validation de la syntaxe de la configuration Nix ---\n");
    let eval_res = Command::new("nix")
        .args(["eval", &format!("{}#nixosConfigurations.nas.config.system.nixos.version", dir_str)])
        .output();

    match eval_res {
        Ok(out) if out.status.success() => {
            log.push_str("✔ Syntaxe Nix validée avec succès.\n");
            Ok(())
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            log.push_str(&format!("⚠ Erreur de syntaxe détectée :\n{}\nAnnulation du pull...\n", err));
            // Rollback de sécurité vers le commit précédent
            let _ = Command::new("git").args(["-C", &dir_str, "reset", "--hard", &current_sha]).output();
            Err(format!("La nouvelle configuration contient une erreur d'évaluation Nix. Rollback de sécurité effectué : {}", err))
        }
        Err(e) => {
            Err(format!("Impossible de valider la syntaxe Nix : {}", e))
        }
    }
}

pub fn apply_intelligent_update(force_packages: bool) -> ApplyUpdateResult {
    // Verrou pour empêcher les mises à jour simultanées
    if IS_UPDATING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return ApplyUpdateResult {
            success: false,
            steps_executed: vec![],
            output_log: "Une mise à jour est déjà en cours d'exécution. Veuillez patienter.".into(),
            error: Some("Mise à jour déjà en cours".into()),
        };
    }

    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            IS_UPDATING.store(false, Ordering::SeqCst);
            if let Ok(mut g) = UPDATE_CACHE.lock() {
                *g = None;
            }
        }
    }
    let _guard = Guard;

    let mut steps_executed = Vec::new();
    let mut output_log = String::new();

    let status = check_updates(true);
    let config_dir = resolve_config_dir();
    let dir_str = config_dir.display().to_string();

    let update_type = if force_packages && status.update_type == UpdateType::None {
        UpdateType::PackagesOnly
    } else {
        status.update_type
    };

    output_log.push_str("==========================================================\n");
    output_log.push_str("🚀 Lancement de la mise à jour intelligente STEvE_OS\n");
    output_log.push_str(&format!("   Mode détecté : {:?}\n", update_type));
    output_log.push_str(&format!("   Répertoire   : {}\n", dir_str));
    output_log.push_str("==========================================================\n\n");

    match update_type {
        UpdateType::None => {
            output_log.push_str("✨ Le système et la configuration sont déjà parfaitement à jour.\n");
            return ApplyUpdateResult {
                success: true,
                steps_executed: vec!["Vérification effectuée : aucun changement".into()],
                output_log,
                error: None,
            };
        }

        UpdateType::ConfigOnly => {
            // 1. D'abord le git pull sécurisé
            steps_executed.push("Git pull sécurisé de la configuration".into());
            if let Err(e) = execute_secure_git_pull(&config_dir, &mut output_log) {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some(e),
                };
            }

            // 2. nh os switch
            steps_executed.push("Déploiement système : nh os switch".into());
            output_log.push_str("\n--- Exécution de nh os switch ---\n");
            let res = run_switch_command(&config_dir, false);
            output_log.push_str(&res.1);
            if !res.0 {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some("Échec de nh os switch".into()),
                };
            }
        }

        UpdateType::PackagesOnly => {
            // 1. nh os switch -u directement
            steps_executed.push("Mise à jour des paquets : nh os switch -u".into());
            output_log.push_str("--- Exécution de nh os switch -u ---\n");
            let res = run_switch_command(&config_dir, true);
            output_log.push_str(&res.1);
            if !res.0 {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some("Échec de nh os switch -u".into()),
                };
            }
        }

        UpdateType::Both => {
            // 1. D'abord le git pull sécurisé
            steps_executed.push("1. Git pull sécurisé de la nouvelle configuration".into());
            if let Err(e) = execute_secure_git_pull(&config_dir, &mut output_log) {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some(e),
                };
            }

            // 2. Ensuite nh os switch -u
            steps_executed.push("2. Déploiement & mise à jour des paquets : nh os switch -u".into());
            output_log.push_str("\n--- Exécution de nh os switch -u (Configuration + Paquets) ---\n");
            let res = run_switch_command(&config_dir, true);
            output_log.push_str(&res.1);
            if !res.0 {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some("Échec de nh os switch -u".into()),
                };
            }
        }
    }

    output_log.push_str("\n✔ Mise à jour terminée avec succès !\n");

    ApplyUpdateResult {
        success: true,
        steps_executed,
        output_log,
        error: None,
    }
}

fn run_switch_command(config_dir: &Path, update_inputs: bool) -> (bool, String) {
    let dir_str = config_dir.display().to_string();

    let (bin, args) = if Path::new("/run/current-system/sw/bin/nh").exists() || Command::new("which").arg("nh").status().map(|s| s.success()).unwrap_or(false) {
        let mut a = vec!["os", "switch"];
        if update_inputs {
            a.push("-u");
        }
        a.push(&dir_str);
        ("nh", a)
    } else {
        // Fallback sans nh si non installé sur la machine
        if update_inputs {
            let _ = Command::new("nix")
                .args(["flake", "update", "--flake", &dir_str])
                .output();
        }
        ("nixos-rebuild", vec!["switch", "--flake", &dir_str])
    };

    let output = Command::new(bin)
        .args(&args)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let mut combined = stdout;
            if !stderr.is_empty() {
                combined.push_str("\n");
                combined.push_str(&stderr);
            }
            (out.status.success(), combined)
        }
        Err(e) => (false, format!("Impossible d'exécuter {} : {}", bin, e)),
    }
}

fn current_time_formatted() -> String {
    let output = Command::new("date")
        .args(["+%H:%M:%S"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "Récemment".into());
    output
}
