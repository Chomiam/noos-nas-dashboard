use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitItem {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeInputStatus {
    pub name: String,
    pub locked_rev: String,
    pub remote_rev: Option<String>,
    pub has_update: bool,
    pub channel_or_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageUpdateItem {
    pub name: String,
    pub current_version: String,
    pub new_version: Option<String>,
    pub action: String, // "update", "add", "remove", "rebuild", "flake"
    pub size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckStatus {
    // Configuration Git
    pub config_update_available: bool,
    pub config_local_commit: String,
    pub config_local_commit_full: String,
    pub config_remote_commit: Option<String>,
    pub config_remote_commit_full: Option<String>,
    pub config_commit_message: Option<String>,
    pub config_commits_behind: u32,
    pub config_pending_commits: Vec<GitCommitItem>,
    pub config_changed_files: Vec<String>,
    pub config_git_status: String,

    // Paquets système & Flake
    pub package_updates_available: bool,
    pub package_updates_count: u32,
    pub package_details: Vec<String>,
    pub flake_inputs_status: Vec<FlakeInputStatus>,
    pub package_updates_list: Vec<PackageUpdateItem>,

    // Synthèse globale
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

fn find_bin(candidates: &[&str]) -> String {
    for c in candidates {
        if c.starts_with('/') {
            if Path::new(c).exists() {
                return c.to_string();
            }
        } else if let Ok(out) = Command::new("which").arg(c).output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return p;
                }
            }
        }
    }
    candidates[0].to_string()
}

pub fn git_binary() -> String {
    find_bin(&["/run/current-system/sw/bin/git", "git", "/nix/var/nix/profiles/default/bin/git", "/usr/bin/git"])
}

pub fn nh_binary() -> String {
    find_bin(&["/run/current-system/sw/bin/nh", "nh", "/nix/var/nix/profiles/default/bin/nh", "/usr/bin/nh"])
}

pub fn nix_binary() -> String {
    find_bin(&["/run/current-system/sw/bin/nix", "nix", "/nix/var/nix/profiles/default/bin/nix", "/usr/bin/nix"])
}

pub fn sudo_binary() -> String {
    find_bin(&[
        "/run/wrappers/bin/sudo",
        "sudo",
        "/run/current-system/sw/bin/sudo",
        "/usr/bin/sudo",
    ])
}

pub fn target_user() -> String {
    if let Ok(u) = env::var("STEVEOS_USER") {
        let trimmed = u.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if Path::new("/home/chomiam").exists() {
        return "chomiam".to_string();
    }
    "root".to_string()
}

pub fn is_root_process() -> bool {
    if let Ok(out) = Command::new("id").arg("-u").output() {
        String::from_utf8_lossy(&out.stdout).trim() == "0"
    } else {
        false
    }
}

pub fn create_user_command(bin: &str, args: &[&str]) -> Command {
    let user = target_user();
    let is_root = is_root_process();
    let runuser_bin = "/run/current-system/sw/bin/runuser";

    if is_root && user != "root" && (Path::new(runuser_bin).exists() || Command::new("runuser").arg("--version").output().is_ok()) {
        let prog = if Path::new(runuser_bin).exists() { runuser_bin } else { "runuser" };
        let mut cmd = Command::new(prog);
        cmd.args(["-u", &user, "--", bin]);
        cmd.args(args);
        let sudo_b = sudo_binary();
        let cur_path = env::var("PATH").unwrap_or_default();
        cmd.env("USER", &user);
        cmd.env("HOME", format!("/home/{}", user));
        cmd.env("NH_FLAKE", "/etc/nixos");
        cmd.env("NH_ELEVATION_STRATEGY", &sudo_b);
        cmd.env("PATH", format!("/run/wrappers/bin:/run/current-system/sw/bin:{}", cur_path));
        cmd
    } else {
        let mut cmd = Command::new(bin);
        cmd.args(args);
        cmd
    }
}

fn git_cmd(repo_dir: &str) -> Command {
    let git_b = git_binary();
    create_user_command(&git_b, &["-c", "safe.directory=*", "-C", repo_dir])
}

pub fn resolve_config_dir() -> PathBuf {
    if let Ok(dir) = env::var("STEVEOS_CONFIG_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return p;
        }
    }

    let candidate1 = PathBuf::from("/etc/nixos");
    if candidate1.exists() {
        return candidate1;
    }

    let candidate2 = PathBuf::from("/home/chomiam/Projects/steveos-nas");
    if candidate2.exists() {
        return candidate2;
    }

    PathBuf::from(".")
}

fn get_local_commit_from_fs(config_dir: &Path) -> (String, String) {
    let git_dir = config_dir.join(".git");
    let head_path = git_dir.join("HEAD");
    if let Ok(head_content) = fs::read_to_string(&head_path) {
        let trimmed = head_content.trim();
        if trimmed.starts_with("ref: ") {
            let ref_rel = trimmed.trim_start_matches("ref: ").trim();
            let ref_file = git_dir.join(ref_rel);
            if let Ok(sha) = fs::read_to_string(&ref_file) {
                let full = sha.trim().to_string();
                let short = full[..7.min(full.len())].to_string();
                return (short, full);
            }
            // Check packed-refs
            let packed_path = git_dir.join("packed-refs");
            if let Ok(packed) = fs::read_to_string(&packed_path) {
                for line in packed.lines() {
                    let line = line.trim();
                    if !line.starts_with('#') && !line.starts_with('^') {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 && parts[1] == ref_rel {
                            let full = parts[0].to_string();
                            let short = full[..7.min(full.len())].to_string();
                            return (short, full);
                        }
                    }
                }
            }
        } else if trimmed.len() >= 40 {
            let full = trimmed.to_string();
            let short = full[..7.min(full.len())].to_string();
            return (short, full);
        }
    }
    ("inconnu".to_string(), String::new())
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

    // 1. Détection du commit local (lecture directe FS + fallback git rev-parse)
    let (fs_short, fs_full) = get_local_commit_from_fs(&config_dir);

    let (local_commit, full_local_commit) = if !fs_full.is_empty() {
        (fs_short, fs_full)
    } else {
        let rev = git_cmd(&config_dir_str)
            .args(["rev-parse", "HEAD"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        if !rev.is_empty() {
            (rev[..7.min(rev.len())].to_string(), rev)
        } else {
            ("inconnu".to_string(), String::new())
        }
    };

    let mut config_update_available = false;
    let mut config_remote_commit = None;
    let mut config_remote_commit_full = None;
    let mut config_commit_message = None;
    let mut config_commits_behind = 0;
    let mut config_pending_commits = Vec::new();
    let mut config_changed_files = Vec::new();

    // 2. Détection du commit distant sur GitHub (Chomiam/steve_os-nix)
    let remote_url = "https://github.com/Chomiam/steve_os-nix.git";
    if let Ok(out) = git_cmd(&config_dir_str)
        .args(["ls-remote", remote_url, "refs/heads/main"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(token) = text.split_whitespace().next() {
                let full_remote = token.to_string();
                let short_remote = token[..7.min(token.len())].to_string();
                config_remote_commit = Some(short_remote);
                config_remote_commit_full = Some(full_remote.clone());

                if !full_local_commit.is_empty() && full_remote != full_local_commit {
                    // Récupération sans toucher aux fichiers de travail
                    let _ = git_cmd(&config_dir_str)
                        .args(["fetch", remote_url, "main"])
                        .output();

                    let is_ancestor = git_cmd(&config_dir_str)
                        .args(["merge-base", "--is-ancestor", &full_remote, "HEAD"])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);

                    if !is_ancestor {
                        config_update_available = true;

                        // Liste des commits en retard
                        if let Ok(log_out) = git_cmd(&config_dir_str)
                            .args(["log", "HEAD..FETCH_HEAD", "--pretty=format:%h|%an|%ad|%s", "--date=short"])
                            .output()
                        {
                            let log_str = String::from_utf8_lossy(&log_out.stdout);
                            for line in log_str.lines() {
                                let parts: Vec<&str> = line.splitn(4, '|').collect();
                                if parts.len() == 4 {
                                    config_pending_commits.push(GitCommitItem {
                                        hash: parts[0].to_string(),
                                        author: parts[1].to_string(),
                                        date: parts[2].to_string(),
                                        message: parts[3].to_string(),
                                    });
                                }
                            }
                            config_commits_behind = config_pending_commits.len() as u32;
                        }

                        // Liste des fichiers modifiés
                        if let Ok(diff_out) = git_cmd(&config_dir_str)
                            .args(["diff", "--name-status", "HEAD", "FETCH_HEAD"])
                            .output()
                        {
                            let diff_str = String::from_utf8_lossy(&diff_out.stdout);
                            for line in diff_str.lines() {
                                let l = line.trim();
                                if !l.is_empty() {
                                    config_changed_files.push(l.to_string());
                                }
                            }
                        }

                        config_commit_message = if !config_pending_commits.is_empty() {
                            Some(format!("{} nouvelle(s) révision(s) en attente : {}", config_commits_behind, config_pending_commits[0].message))
                        } else {
                            Some("Nouvelle révision disponible sur GitHub (steve_os-nix)".into())
                        };
                    }
                }
            }
        }
    }

    // Statut local du repo Git (propre ou modifications en cours)
    let git_status_clean = git_cmd(&config_dir_str)
        .args(["status", "--porcelain"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(true);

    let config_git_status = if git_status_clean {
        "Arbre de travail propre".to_string()
    } else {
        "Modifications locales détectées (stash automatique)".to_string()
    };

    // 3. Vérification des paquets Nixpkgs & Entrées Flake dans flake.lock
    let mut package_updates_available = false;
    let mut package_updates_count = 0;
    let mut package_details = Vec::new();
    let mut flake_inputs_status = Vec::new();

    let lock_path = config_dir.join("flake.lock");
    if let Ok(lock_str) = fs::read_to_string(&lock_path) {
        if let Ok(lock_json) = serde_json::from_str::<serde_json::Value>(&lock_str) {
            if let Some(nodes) = lock_json.get("nodes").and_then(|n| n.as_object()) {
                for (node_name, node_val) in nodes {
                    if node_name == "root" {
                        continue;
                    }
                    let locked = node_val.get("locked");
                    let original = node_val.get("original");

                    let locked_rev = locked
                        .and_then(|l| l.get("rev"))
                        .and_then(|r| r.as_str())
                        .unwrap_or("");

                    if locked_rev.is_empty() {
                        continue;
                    }

                    let owner = original.and_then(|o| o.get("owner")).and_then(|o| o.as_str()).unwrap_or("");
                    let repo = original.and_then(|o| o.get("repo")).and_then(|r| r.as_str()).unwrap_or("");
                    let ref_branch = original.and_then(|o| o.get("ref")).and_then(|r| r.as_str()).unwrap_or("main");

                    if !owner.is_empty() && !repo.is_empty() {
                        let remote_git_url = format!("https://github.com/{}/{}", owner, repo);
                        let mut remote_rev = None;
                        let mut has_update = false;

                        if let Ok(ls_out) = Command::new(git_binary())
                            .args(["-c", "safe.directory=*", "ls-remote", &remote_git_url, &format!("refs/heads/{}", ref_branch)])
                            .output()
                        {
                            if ls_out.status.success() {
                                let ls_text = String::from_utf8_lossy(&ls_out.stdout);
                                if let Some(r_sha) = ls_text.split_whitespace().next() {
                                    let short_remote = r_sha[..7.min(r_sha.len())].to_string();
                                    remote_rev = Some(short_remote);
                                    if r_sha != locked_rev {
                                        has_update = true;
                                        package_updates_available = true;
                                        package_updates_count += 1;
                                        package_details.push(format!("{}/{} ({}) : mise à jour disponible", owner, repo, ref_branch));
                                    }
                                }
                            }
                        }

                        flake_inputs_status.push(FlakeInputStatus {
                            name: node_name.clone(),
                            locked_rev: locked_rev[..7.min(locked_rev.len())].to_string(),
                            remote_rev,
                            has_update,
                            channel_or_ref: ref_branch.to_string(),
                        });
                    }
                }
            }
        }
    }

    // 4. Liste détaillée des paquets qui seront mis à jour / modifiés
    let package_updates_list = detect_package_updates_list(&config_dir, package_updates_available);

    // 5. Détermination du type d'action requise
    let update_type = match (config_update_available, package_updates_available || !package_updates_list.is_empty()) {
        (true, true) => UpdateType::Both,
        (true, false) => UpdateType::ConfigOnly,
        (false, true) => UpdateType::PackagesOnly,
        (false, false) => UpdateType::None,
    };

    let status_text = match update_type {
        UpdateType::Both => "⚡ Nouvelle configuration ET paquets disponibles !".to_string(),
        UpdateType::ConfigOnly => "📥 Nouvelle configuration disponible sur GitHub".to_string(),
        UpdateType::PackagesOnly => "📦 Mises à jour de paquets système prêtes à être appliquées".to_string(),
        UpdateType::None => "✨ Système et configuration à jour".to_string(),
    };

    let status = UpdateCheckStatus {
        config_update_available,
        config_local_commit: local_commit,
        config_local_commit_full: full_local_commit,
        config_remote_commit,
        config_remote_commit_full,
        config_commit_message,
        config_commits_behind,
        config_pending_commits,
        config_changed_files,
        config_git_status,
        package_updates_available,
        package_updates_count,
        package_details,
        flake_inputs_status,
        package_updates_list,
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

fn detect_package_updates_list(config_dir: &Path, inputs_have_updates: bool) -> Vec<PackageUpdateItem> {
    let mut list = Vec::new();
    let dir_str = config_dir.display().to_string();

    // Exécution d'un dry-run Nix pour capturer les dérivations et paquets qui seront téléchargés / construits
    let target_attr = format!("{}#nixosConfigurations.nas.config.system.build.toplevel", dir_str);
    let mut args = vec![
        "build",
        &target_attr,
        "--dry-run",
    ];

    if inputs_have_updates {
        args.push("--recreate-lock-file");
        args.push("--no-write-lock-file");
    }

    if let Ok(out) = Command::new(nix_binary()).args(&args).output() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let combined = format!("{}\n{}", stdout, stderr);
        list.extend(parse_nix_dry_run(&combined));
    }

    // Détection complémentaire : paquets ajoutés dans environment.systemPackages absents de /run/current-system/sw/bin
    let sw_bin = Path::new("/run/current-system/sw/bin");
    let check_pkgs = [
        ("gh", "GitHub CLI (Gestionnaire GitHub officiel)", "2.101.0"),
        ("nvd", "Nix Package Version Diff Tool", "0.2.4"),
        ("git", "Git Distributed Version Control", "2.54.0"),
        ("nh", "Nix Helper CLI", "4.4.2"),
        ("steveos-nas-dashboard", "Tableau de bord NAS STEvE_OS", "0.1.0"),
    ];

    for (pkg, desc, ver) in check_pkgs {
        let installed = sw_bin.join(pkg).exists();
        if !installed && !list.iter().any(|i| i.name == pkg) {
            list.push(PackageUpdateItem {
                name: pkg.to_string(),
                current_version: "Non installé sur le système".to_string(),
                new_version: Some(format!("{} ({})", ver, desc)),
                action: "add".to_string(),
                size: Some("Inclus dans la configuration".to_string()),
            });
        }
    }

    list
}

fn parse_nix_dry_run(output: &str) -> Vec<PackageUpdateItem> {
    let mut items = Vec::new();
    let mut current_action = "update".to_string();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("these ") && trimmed.contains("paths will be fetched") {
            current_action = "download".to_string();
            continue;
        } else if trimmed.starts_with("this path will be fetched") {
            current_action = "download".to_string();
            continue;
        } else if trimmed.starts_with("these ") && trimmed.contains("derivations will be built") {
            current_action = "build".to_string();
            continue;
        } else if trimmed.starts_with("this derivation will be built") {
            current_action = "build".to_string();
            continue;
        }

        if trimmed.starts_with("/nix/store/") {
            let path_clean = trimmed.trim_end_matches(".drv");
            let store_name = path_clean.strip_prefix("/nix/store/").unwrap_or(path_clean);
            if let Some(dash_idx) = store_name.find('-') {
                let pkg_and_ver = &store_name[dash_idx + 1..];
                let (name, ver) = split_pkg_name_and_version(pkg_and_ver);
                if !name.is_empty() 
                    && !name.starts_with("system-units") 
                    && !name.starts_with("etc") 
                    && !name.starts_with("unit-") 
                    && !name.starts_with("user-units")
                {
                    if !items.iter().any(|i: &PackageUpdateItem| i.name == name) {
                        items.push(PackageUpdateItem {
                            name: name.to_string(),
                            current_version: "Précédent / Installé".to_string(),
                            new_version: Some(ver.to_string()),
                            action: current_action.clone(),
                            size: None,
                        });
                    }
                }
            }
        }
    }
    items
}

fn split_pkg_name_and_version(s: &str) -> (&str, &str) {
    let bytes = s.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i - 1] == b'-' && bytes[i].is_ascii_digit() {
            return (&s[..i - 1], &s[i..]);
        }
    }
    (s, "dernière version")
}

pub fn execute_secure_git_pull(config_dir: &Path, log: &mut String) -> Result<(), String> {
    log.push_str("--- [Étape 1/3] Sécurisation de l'espace de travail local ---\n");

    let dir_str = config_dir.display().to_string();

    // 1. Sauvegarde automatique du commit courant
    let current_sha = git_cmd(&dir_str)
        .args(["rev-parse", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    log.push_str(&format!("Point de restauration courant : {}\n", &current_sha[..8.min(current_sha.len())]));

    // 2. Vérifier si des fichiers modifiés localement existent
    let status_out = git_cmd(&dir_str)
        .args(["status", "--porcelain"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let has_uncommitted = !status_out.is_empty();
    if has_uncommitted {
        log.push_str("Modifications locales détectées. Création d'un stash de sécurité...\n");
        let stash_res = git_cmd(&dir_str)
            .args(["stash", "push", "-u", "-m", "steveos-auto-stash"])
            .output();

        if let Ok(res) = stash_res {
            log.push_str(&String::from_utf8_lossy(&res.stdout));
        }
    }

    // 3. Fetch et merge sécurisé (fast-forward privilégié)
    log.push_str("\n--- [Étape 2/3] Récupération des nouveautés depuis GitHub (steve_os-nix) ---\n");
    let fetch_out = git_cmd(&dir_str)
        .args(["fetch", "origin", "main"])
        .output()
        .map_err(|e| format!("Échec git fetch : {}", e))?;

    if !fetch_out.status.success() {
        let err = String::from_utf8_lossy(&fetch_out.stderr);
        return Err(format!("Erreur lors de la récupération distante : {}", err));
    }

    let merge_out = git_cmd(&dir_str)
        .args(["merge", "--ff-only", "origin/main"])
        .output()
        .map_err(|e| format!("Échec du merge fast-forward : {}", e))?;

    if !merge_out.status.success() {
        log.push_str("Merge fast-forward non direct. Tentative de rebase automatique...\n");
        let rebase_out = git_cmd(&dir_str)
            .args(["rebase", "origin/main"])
            .output();

        match rebase_out {
            Ok(r) if r.status.success() => {
                log.push_str("Rebase réussi avec succès.\n");
            }
            _ => {
                let _ = git_cmd(&dir_str).args(["rebase", "--abort"]).output();
                if has_uncommitted {
                    let _ = git_cmd(&dir_str).args(["stash", "pop"]).output();
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
        let _ = git_cmd(&dir_str).args(["stash", "pop"]).output();
    }

    // 5. Validation de la syntaxe Nix (nix eval de sécurité)
    log.push_str("\n--- [Étape 3/3] Validation de la syntaxe de la configuration Nix ---\n");
    let nix_b = nix_binary();
    let eval_target = format!("{}#nixosConfigurations.nas.config.system.nixos.version", dir_str);
    let mut eval_cmd = create_user_command(&nix_b, &["eval", &eval_target]);
    let eval_res = eval_cmd.output();

    match eval_res {
        Ok(out) if out.status.success() => {
            log.push_str("✔ Syntaxe Nix validée avec succès.\n");
            Ok(())
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            log.push_str(&format!("⚠ Erreur de syntaxe détectée :\n{}\nAnnulation du pull...\n", err));
            let _ = git_cmd(&dir_str).args(["reset", "--hard", &current_sha]).output();
            Err(format!("La nouvelle configuration contient une erreur d'évaluation Nix. Rollback de sécurité effectué : {}", err))
        }
        Err(e) => {
            Err(format!("Impossible de valider la syntaxe Nix : {}", e))
        }
    }
}

pub fn apply_intelligent_update(force_packages: bool) -> ApplyUpdateResult {
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

    let nh_bin = nh_binary();
    let sudo_b = sudo_binary();
    let (bin, args) = if Path::new(&nh_bin).exists() {
        let mut a = vec!["os", "switch", "-e", &sudo_b];
        if update_inputs {
            a.push("-u");
        }
        a.push(&dir_str);
        (nh_bin, a)
    } else {
        if update_inputs {
            let nix_b = nix_binary();
            let mut update_cmd = create_user_command(&nix_b, &["flake", "update", "--flake", &dir_str]);
            update_cmd.current_dir(config_dir);
            let _ = update_cmd.output();
        }
        ("nixos-rebuild".to_string(), vec!["switch", "--flake", &dir_str])
    };

    let mut cmd = create_user_command(&bin, &args);
    cmd.current_dir(config_dir);

    let output = cmd.output();

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
