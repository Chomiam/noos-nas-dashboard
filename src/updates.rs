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
pub struct OsTelemetry {
    pub repo_name: String,
    pub local_commit: String,
    pub local_commit_full: String,
    pub remote_commit: Option<String>,
    pub remote_commit_full: Option<String>,
    pub commit_message: Option<String>,
    pub commits_behind: u32,
    pub pending_commits: Vec<GitCommitItem>,
    pub changed_files: Vec<String>,
    pub git_status: String,
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardTelemetry {
    pub repo_name: String,
    pub running_version: String,
    pub running_commit: Option<String>,
    pub target_version: Option<String>,
    pub target_commit: Option<String>,
    pub remote_commit: Option<String>,
    pub update_available: bool,
    pub status_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckStatus {
    // Double Télémétrie
    pub os_telemetry: OsTelemetry,
    pub dashboard_telemetry: DashboardTelemetry,
    pub dashboard_update_available: bool,
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
    pub system_generation: Option<String>,
    #[serde(default)]
    pub system_generations_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgressState {
    pub is_running: bool,
    pub stage: String,
    pub step_index: u32,
    pub total_steps: u32,
    pub progress_percent: u32,
    pub status_title: String,
    pub status_detail: String,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub target_commit: Option<String>,
    pub generation_before: Option<String>,
    pub generation_after: Option<String>,
    pub dashboard_restarting: bool,
    pub log_tail: String,
    #[serde(default)]
    pub total_derivations: Option<u32>,
    #[serde(default)]
    pub current_derivation_index: Option<u32>,
    #[serde(default)]
    pub current_package_name: Option<String>,
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
static IS_CHECKING: AtomicBool = AtomicBool::new(false);
static LIVE_UPDATE_LOG: Mutex<String> = Mutex::new(String::new());

pub fn is_updating() -> bool {
    IS_UPDATING.load(Ordering::SeqCst)
}

fn get_update_log_path() -> std::path::PathBuf {
    let log_dir = std::path::Path::new("/var/log");
    if log_dir.exists() {
        return log_dir.join("steveos-update.log");
    }
    std::path::PathBuf::from("/run/steveos-update.log")
}

pub fn append_live_log(msg: &str) {
    if let Ok(mut log) = LIVE_UPDATE_LOG.lock() {
        log.push_str(msg);
    }
    use std::io::Write;
    let log_path = get_update_log_path();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = f.write_all(msg.as_bytes());
    }
}

pub fn get_live_log() -> (String, bool) {
    let mut log_str = LIVE_UPDATE_LOG.lock().map(|l| l.clone()).unwrap_or_default();
    if log_str.trim().is_empty() {
        let log_path = get_update_log_path();
        if let Ok(content) = std::fs::read_to_string(&log_path) {
            log_str = content;
        }
    }
    (log_str, is_updating())
}

pub fn get_current_system_generation() -> Option<String> {
    if let Ok(target) = fs::read_link("/nix/var/nix/profiles/system") {
        let name = target.file_name()?.to_string_lossy();
        if let Some(stripped) = name.strip_prefix("system-") {
            if let Some(gen) = stripped.strip_suffix("-link") {
                return Some(gen.to_string());
            }
        }
        return Some(name.to_string());
    }
    None
}

pub fn get_system_generations_count() -> u32 {
    if let Ok(entries) = fs::read_dir("/nix/var/nix/profiles") {
        entries.flatten().filter(|e| {
            let fname = e.file_name().to_string_lossy().to_string();
            fname.starts_with("system-") && fname.ends_with("-link")
        }).count() as u32
    } else {
        0
    }
}

pub fn get_update_state_file_path() -> PathBuf {
    PathBuf::from("/run/steveos-update-state.json")
}

pub fn save_update_progress(state: &UpdateProgressState) {
    let path = get_update_state_file_path();
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(&path, json);
    }
}

pub fn get_update_progress() -> UpdateProgressState {
    let path = get_update_state_file_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(state) = serde_json::from_str::<UpdateProgressState>(&content) {
            return state;
        }
    }
    UpdateProgressState {
        is_running: is_updating(),
        stage: if is_updating() { "running".to_string() } else { "idle".to_string() },
        step_index: if is_updating() { 2 } else { 0 },
        total_steps: 4,
        progress_percent: if is_updating() { 50 } else { 0 },
        status_title: if is_updating() { "Mise à jour en cours d'exécution...".to_string() } else { "Système prêt".to_string() },
        status_detail: "Aucune mise à jour en cours d'exécution.".to_string(),
        error: None,
        started_at: None,
        completed_at: None,
        target_commit: None,
        generation_before: get_current_system_generation(),
        generation_after: None,
        dashboard_restarting: false,
        log_tail: String::new(),
        total_derivations: None,
        current_derivation_index: None,
        current_package_name: None,
    }
}

pub fn dismiss_update_progress() {
    let mut state = get_update_progress();
    state.is_running = false;
    state.stage = "idle".to_string();
    state.progress_percent = 0;
    state.error = None;
    state.total_derivations = None;
    state.current_derivation_index = None;
    state.current_package_name = None;
    save_update_progress(&state);
}

pub fn init_update_tracker() {
    let mut state = get_update_progress();
    if state.is_running {
        let cur_gen = get_current_system_generation();
        let changed = match (&state.generation_before, &cur_gen) {
            (Some(before), Some(current)) => before != current,
            _ => true,
        };

        if changed || state.dashboard_restarting || state.stage == "activating" {
            state.is_running = false;
            state.stage = "completed".to_string();
            state.step_index = 4;
            state.total_steps = 4;
            state.progress_percent = 100;
            state.status_title = "Mise à jour terminée avec succès !".to_string();
            state.status_detail = format!("Le système a basculé avec succès sur la génération {}.", cur_gen.clone().unwrap_or_else(|| "suivante".to_string()));
            state.generation_after = cur_gen;
            state.completed_at = Some(current_time_formatted());
            state.dashboard_restarting = false;
            save_update_progress(&state);
        }
    }
}

pub fn get_cached_status() -> Option<UpdateCheckStatus> {
    if let Ok(guard) = UPDATE_CACHE.lock() {
        if let Some((_, ref cached)) = *guard {
            let mut res = cached.clone();
            res.is_updating = is_updating();
            return Some(res);
        }
    }
    None
}

pub fn start_background_checker() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(3));
        let _ = check_updates(true);

        loop {
            std::thread::sleep(Duration::from_secs(15 * 60));
            if !is_updating() {
                let _ = check_updates(true);
            }
        }
    });
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

pub fn nixos_rebuild_binary() -> String {
    find_bin(&[
        "/run/current-system/sw/bin/nixos-rebuild",
        "nixos-rebuild",
        "/nix/var/nix/profiles/default/bin/nixos-rebuild",
        "/usr/bin/nixos-rebuild",
    ])
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

pub fn user_exists(username: &str) -> bool {
    if let Ok(content) = fs::read_to_string("/etc/passwd") {
        for line in content.lines() {
            if let Some(user) = line.split(':').next() {
                if user.trim() == username {
                    return true;
                }
            }
        }
    }
    false
}

pub fn find_first_human_user() -> Option<String> {
    if let Ok(content) = fs::read_to_string("/etc/passwd") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 4 {
                let username = parts[0].trim();
                let uid: u32 = parts[2].trim().parse().unwrap_or(0);
                if uid >= 1000 && uid < 60000 && username != "nobody" && !username.starts_with("nixbld") {
                    return Some(username.to_string());
                }
            }
        }
    }
    None
}

pub fn get_user_home(username: &str) -> PathBuf {
    if let Ok(content) = fs::read_to_string("/etc/passwd") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 6 && parts[0].trim() == username {
                let home = parts[5].trim();
                if !home.is_empty() {
                    return PathBuf::from(home);
                }
            }
        }
    }
    PathBuf::from(format!("/home/{}", username))
}

pub fn target_user() -> String {
    if let Ok(u) = env::var("STEVEOS_USER") {
        let trimmed = u.trim();
        if !trimmed.is_empty() && user_exists(trimmed) {
            return trimmed.to_string();
        }
    }
    if let Some(human) = find_first_human_user() {
        return human;
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
    let home_path = get_user_home(&user);
    let home_str = home_path.to_string_lossy().to_string();
    let is_root = is_root_process();
    let runuser_bin = "/run/current-system/sw/bin/runuser";
    let sudo_b = sudo_binary();
    let cur_path = env::var("PATH").unwrap_or_default();
    let complete_path = format!("/run/wrappers/bin:/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:{}", cur_path);

    if is_root && user != "root" && (Path::new(runuser_bin).exists() || Command::new("runuser").arg("--version").output().is_ok()) {
        let prog = if Path::new(runuser_bin).exists() { runuser_bin } else { "runuser" };
        let mut cmd = Command::new(prog);
        cmd.args(["-u", &user, "--", "env", &format!("PATH={}", complete_path), &format!("HOME={}", home_str), &format!("USER={}", user), &format!("NH_FLAKE=/etc/nixos"), &format!("NH_ELEVATION_STRATEGY={}", sudo_b), bin]);
        cmd.args(args);
        cmd.env("USER", &user);
        cmd.env("HOME", &home_str);
        cmd.env("NH_FLAKE", "/etc/nixos");
        cmd.env("NH_ELEVATION_STRATEGY", &sudo_b);
        cmd.env("PATH", &complete_path);
        cmd
    } else {
        let mut cmd = Command::new(bin);
        cmd.args(args);
        cmd.env("USER", &user);
        cmd.env("HOME", &home_str);
        cmd.env("NH_FLAKE", "/etc/nixos");
        cmd.env("NH_ELEVATION_STRATEGY", &sudo_b);
        cmd.env("PATH", &complete_path);
        cmd
    }
}

fn git_cmd(repo_dir: &str) -> Command {
    let git_b = git_binary();
    create_user_command(&git_b, &["-c", "safe.directory=*", "-c", "user.name=STEvE_OS", "-c", "user.email=steveos@local", "-C", repo_dir])
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

    let user = target_user();
    let candidate2 = get_user_home(&user).join("Projects/steveos-nas");
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
                if instant.elapsed() < Duration::from_secs(900) {
                    let mut res = cached.clone();
                    res.is_updating = is_updating();
                    return res;
                }
            }
        }
    }

    if IS_CHECKING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        if let Some(cached) = get_cached_status() {
            return cached;
        }
    }

    struct CheckGuard;
    impl Drop for CheckGuard {
        fn drop(&mut self) {
            IS_CHECKING.store(false, Ordering::SeqCst);
        }
    }
    let _check_guard = CheckGuard;

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

    // 3. Télémétrie spécifique du Dashboard STEvE_OS (Double Télémétrie)
    let running_dashboard_version = env!("CARGO_PKG_VERSION").to_string();
    let mut dashboard_target_commit = None;
    let mut dashboard_target_commit_full = None;
    let mut dashboard_remote_commit = None;
    let mut dashboard_remote_commit_full = None;
    let mut dashboard_remote_version = None;
    let mut dashboard_update_available = false;

    let dashboard_git_url = "https://github.com/Chomiam/steveos-nas-dashboard.git";
    if let Ok(ls_out) = Command::new(git_binary())
        .args(["-c", "safe.directory=*", "ls-remote", dashboard_git_url, "refs/heads/main"])
        .output()
    {
        if ls_out.status.success() {
            let text = String::from_utf8_lossy(&ls_out.stdout);
            if let Some(sha) = text.split_whitespace().next() {
                dashboard_remote_commit_full = Some(sha.to_string());
                dashboard_remote_commit = Some(sha[..7.min(sha.len())].to_string());
            }
        }
    }

    if let Ok(tags_out) = Command::new(git_binary())
        .args(["-c", "safe.directory=*", "ls-remote", "--tags", dashboard_git_url])
        .output()
    {
        if tags_out.status.success() {
            let text = String::from_utf8_lossy(&tags_out.stdout);
            let mut highest_tag: Option<String> = None;
            for line in text.lines() {
                for part in line.split_whitespace() {
                    if let Some(tag) = part.strip_prefix("refs/tags/v") {
                        let clean_tag = tag.trim_end_matches("^{}");
                        if is_valid_semver(clean_tag) {
                            if let Some(ref current) = highest_tag {
                                if compare_semver(clean_tag, current) > 0 {
                                    highest_tag = Some(clean_tag.to_string());
                                }
                            } else {
                                highest_tag = Some(clean_tag.to_string());
                            }
                        }
                    }
                }
            }
            dashboard_remote_version = highest_tag;
        }
    }

    // Vérification des paquets Nixpkgs & Entrées Flake dans flake.lock
    let mut package_updates_available = false;
    let mut package_updates_count = 0;
    let mut package_details = Vec::new();
    let mut flake_inputs_status = Vec::new();

    let lock_path = config_dir.join("flake.lock");
    if let Ok(lock_str) = fs::read_to_string(&lock_path) {
        if let Ok(lock_json) = serde_json::from_str::<serde_json::Value>(&lock_str) {
            let root_inputs: Vec<String> = lock_json
                .get("nodes")
                .and_then(|n| n.get("root"))
                .and_then(|r| r.get("inputs"))
                .and_then(|i| i.as_object())
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();

            if let Some(nodes) = lock_json.get("nodes").and_then(|n| n.as_object()) {
                for (node_name, node_val) in nodes {
                    if node_name == "root" || !root_inputs.contains(node_name) {
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

                    if node_name == "steveos-nas-dashboard" {
                        dashboard_target_commit_full = Some(locked_rev.to_string());
                        dashboard_target_commit = Some(locked_rev[..7.min(locked_rev.len())].to_string());
                        continue;
                    }

                    if node_name == "nixpkgs" {
                        flake_inputs_status.push(FlakeInputStatus {
                            name: node_name.clone(),
                            locked_rev: locked_rev[..7.min(locked_rev.len())].to_string(),
                            remote_rev: None,
                            has_update: false,
                            channel_or_ref: "nixos-26.05".to_string(),
                        });
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

    let target_dashboard_version = dashboard_remote_version.clone().unwrap_or_else(|| running_dashboard_version.clone());

    // Vérification de la disponibilité d'une mise à jour du Dashboard
    if compare_semver(&target_dashboard_version, &running_dashboard_version) > 0 {
        dashboard_update_available = true;
    }
    if let (Some(ref locked), Some(ref remote)) = (&dashboard_target_commit_full, &dashboard_remote_commit_full) {
        if locked != remote {
            dashboard_update_available = true;
        }
    }

    let os_telemetry = OsTelemetry {
        repo_name: "Chomiam/steve_os-nix".to_string(),
        local_commit: local_commit.clone(),
        local_commit_full: full_local_commit.clone(),
        remote_commit: config_remote_commit.clone(),
        remote_commit_full: config_remote_commit_full.clone(),
        commit_message: config_commit_message.clone(),
        commits_behind: config_commits_behind,
        pending_commits: config_pending_commits.clone(),
        changed_files: config_changed_files.clone(),
        git_status: config_git_status.clone(),
        update_available: config_update_available,
    };

    let dashboard_status_str = if dashboard_update_available {
        format!("Mise à niveau prête : v{} (actuel : v{})", target_dashboard_version, running_dashboard_version)
    } else {
        format!("À jour (version v{} active)", running_dashboard_version)
    };

    let dashboard_telemetry = DashboardTelemetry {
        repo_name: "Chomiam/steveos-nas-dashboard".to_string(),
        running_version: running_dashboard_version.clone(),
        running_commit: None,
        target_version: Some(target_dashboard_version.clone()),
        target_commit: dashboard_target_commit.clone(),
        remote_commit: dashboard_remote_commit.clone(),
        update_available: dashboard_update_available,
        status_text: dashboard_status_str,
    };

    // 4. Liste détaillée des paquets qui seront mis à jour / modifiés
    let package_updates_list = detect_package_updates_list(&config_dir, package_updates_available);

    // 5. Détermination du type d'action requise (Double Télémétrie)
    let has_pkg_or_dash = dashboard_update_available || package_updates_available || !package_updates_list.is_empty();
    let update_type = match (config_update_available, has_pkg_or_dash) {
        (true, true) => UpdateType::Both,
        (true, false) => UpdateType::ConfigOnly,
        (false, true) => UpdateType::PackagesOnly,
        (false, false) => UpdateType::None,
    };

    let status_text = match update_type {
        UpdateType::Both => {
            if dashboard_update_available && config_commits_behind > 0 {
                format!("⚡ Nouvelle configuration OS ({} commit(s)) ET Dashboard v{} disponibles !", config_commits_behind, target_dashboard_version)
            } else if dashboard_update_available {
                format!("⚡ Nouvelle configuration OS et Dashboard v{} disponibles !", target_dashboard_version)
            } else {
                "⚡ Nouvelle configuration OS et paquets disponibles !".to_string()
            }
        }
        UpdateType::ConfigOnly => {
            if config_commits_behind > 0 {
                format!("📥 Nouvelle configuration disponible sur GitHub ({} nouveau(x) commit(s))", config_commits_behind)
            } else {
                "📥 Nouvelle configuration disponible sur GitHub".to_string()
            }
        }
        UpdateType::PackagesOnly => {
            if dashboard_update_available {
                format!("⚡ Mise à jour du Dashboard STEvE_OS disponible (v{} → v{})", running_dashboard_version, target_dashboard_version)
            } else {
                "📦 Mises à jour de paquets système prêtes à être appliquées".to_string()
            }
        }
        UpdateType::None => "✨ Système d'exploitation et Dashboard STEvE_OS à jour".to_string(),
    };

    let status = UpdateCheckStatus {
        os_telemetry,
        dashboard_telemetry,
        dashboard_update_available,
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
        system_generation: get_current_system_generation(),
        system_generations_count: get_system_generations_count(),
    };

    if let Ok(mut guard) = UPDATE_CACHE.lock() {
        *guard = Some((Instant::now(), status.clone()));
    }

    status
}

fn detect_package_updates_list(config_dir: &Path, inputs_have_updates: bool) -> Vec<PackageUpdateItem> {
    let mut list = Vec::new();
    if !inputs_have_updates {
        return list;
    }

    let dir_str = config_dir.display().to_string();
    let target_attr = format!("{}#nixosConfigurations.nas.config.system.build.toplevel", dir_str);
    let args = vec![
        "build",
        &target_attr,
        "--dry-run",
    ];

    if let Ok(out) = Command::new(nix_binary()).args(&args).output() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let combined = format!("{}\n{}", stdout, stderr);
        list.extend(parse_nix_dry_run(&combined));
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

pub fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() >= 3 {
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts[2].parse().ok()?;
        Some((major, minor, patch))
    } else if parts.len() == 2 {
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        Some((major, minor, 0))
    } else {
        None
    }
}

pub fn compare_semver(a: &str, b: &str) -> i32 {
    match (parse_semver(a), parse_semver(b)) {
        (Some(sa), Some(sb)) => {
            if sa.0 != sb.0 {
                sa.0.cmp(&sb.0) as i32
            } else if sa.1 != sb.1 {
                sa.1.cmp(&sb.1) as i32
            } else {
                sa.2.cmp(&sb.2) as i32
            }
        }
        _ => a.cmp(b) as i32,
    }
}

pub fn is_valid_semver(s: &str) -> bool {
    parse_semver(s).is_some()
}


pub fn execute_secure_git_pull(config_dir: &Path, log: &mut String) -> Result<(), String> {
    log.push_str("--- [Étape 1/3] Sécurisation de l'espace de travail local ---\n");

    let dir_str = config_dir.display().to_string();

    // Aligner préventivement flake.lock et les fichiers d'état déclaratifs avec le dépôt Git
    let _ = git_cmd(&dir_str).args(["checkout", "--", "flake.lock", "firewall-state.json", "firewall-rules.json"]).output();

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

    let cached = get_cached_status();
    let config_dir = resolve_config_dir();
    let dir_str = config_dir.display().to_string();

    let update_type = if force_packages {
        UpdateType::PackagesOnly
    } else if let Some(ref c) = cached {
        if c.update_type == UpdateType::None {
            UpdateType::PackagesOnly
        } else {
            c.update_type.clone()
        }
    } else {
        UpdateType::Both
    };

    let header = format!(
        "==========================================================\n🚀 Lancement de la mise à jour intelligente STEvE_OS\n   Mode détecté : {:?}\n   Répertoire   : {}\n==========================================================\n\n",
        update_type, dir_str
    );

    if let Ok(mut log) = LIVE_UPDATE_LOG.lock() {
        log.clear();
        log.push_str(&header);
    }
    let _ = std::fs::write(get_update_log_path(), &header);
    output_log.push_str(&header);

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

            // 2. nixos-rebuild switch
            steps_executed.push("Déploiement système : nixos-rebuild switch".into());
            output_log.push_str("\n--- Exécution de nixos-rebuild switch ---\n");
            let res = run_switch_command(&config_dir, false);
            output_log.push_str(&res.1);
            if !res.0 {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some("Échec de nixos-rebuild switch".into()),
                };
            }
        }

        UpdateType::PackagesOnly => {
            // 1. nixos-rebuild switch -u directement
            steps_executed.push("Mise à jour des paquets : nixos-rebuild switch -u".into());
            output_log.push_str("--- Exécution de nixos-rebuild switch -u ---\n");
            let res = run_switch_command(&config_dir, true);
            output_log.push_str(&res.1);
            if !res.0 {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some("Échec de nixos-rebuild switch -u".into()),
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

            // 2. Ensuite nixos-rebuild switch -u
            steps_executed.push("2. Déploiement système : nixos-rebuild switch".into());
            output_log.push_str("\n--- Exécution de nixos-rebuild switch -u (Configuration + Paquets) ---\n");
            let res = run_switch_command(&config_dir, false);
            output_log.push_str(&res.1);
            if !res.0 {
                return ApplyUpdateResult {
                    success: false,
                    steps_executed,
                    output_log,
                    error: Some("Échec de nixos-rebuild switch -u".into()),
                };
            }
        }
    }

    output_log.push_str("\n✔ Mise à jour terminée avec succès !\n");
    append_live_log("\n✔ Mise à jour terminée avec succès !\n");

    if let Ok(mut guard) = UPDATE_CACHE.lock() {
        let (short_c, full_c) = get_local_commit_from_fs(&config_dir);
        let clean_os_telemetry = OsTelemetry {
            repo_name: "Chomiam/steve_os-nix".to_string(),
            local_commit: short_c.clone(),
            local_commit_full: full_c.clone(),
            remote_commit: Some(short_c.clone()),
            remote_commit_full: Some(full_c.clone()),
            commit_message: Some("Configuration système NixOS synchronisée.".to_string()),
            commits_behind: 0,
            pending_commits: vec![],
            changed_files: vec![],
            git_status: "Arbre de travail propre".to_string(),
            update_available: false,
        };
        let clean_dash_ver = env!("CARGO_PKG_VERSION").to_string();
        let clean_dashboard_telemetry = DashboardTelemetry {
            repo_name: "Chomiam/steveos-nas-dashboard".to_string(),
            running_version: clean_dash_ver.clone(),
            running_commit: None,
            target_version: Some(clean_dash_ver.clone()),
            target_commit: None,
            remote_commit: None,
            update_available: false,
            status_text: format!("À jour (version v{} active)", clean_dash_ver),
        };
        let clean_status = UpdateCheckStatus {
            os_telemetry: clean_os_telemetry,
            dashboard_telemetry: clean_dashboard_telemetry,
            dashboard_update_available: false,
            config_update_available: false,
            config_local_commit: short_c,
            config_local_commit_full: full_c,
            config_remote_commit: None,
            config_remote_commit_full: None,
            config_commit_message: None,
            config_commits_behind: 0,
            config_pending_commits: vec![],
            config_changed_files: vec![],
            config_git_status: "Arbre de travail propre".to_string(),
            package_updates_available: false,
            package_updates_count: 0,
            package_details: vec![],
            flake_inputs_status: vec![],
            package_updates_list: vec![],
            update_type: UpdateType::None,
            status_text: "✨ Système et configuration à jour".to_string(),
            config_dir: dir_str,
            last_checked: current_time_formatted(),
            is_updating: false,
            system_generation: get_current_system_generation(),
            system_generations_count: get_system_generations_count(),
        };
        *guard = Some((Instant::now(), clean_status));
    }

    ApplyUpdateResult {
        success: true,
        steps_executed,
        output_log,
        error: None,
    }
}

pub fn start_detached_update(force_packages: bool) -> Result<(), String> {
    let current_state = get_update_progress();
    if current_state.is_running {
        return Err("Une mise à jour est déjà en cours d'exécution.".to_string());
    }

    let cur_gen = get_current_system_generation();
    let initial_state = UpdateProgressState {
        is_running: true,
        stage: "starting".to_string(),
        step_index: 1,
        total_steps: 4,
        progress_percent: 5,
        status_title: "Initialisation de la mise à jour...".to_string(),
        status_detail: "Préparation de l'environnement d'exécution...".to_string(),
        error: None,
        started_at: Some(current_time_formatted()),
        completed_at: None,
        target_commit: None,
        generation_before: cur_gen,
        generation_after: None,
        dashboard_restarting: false,
        log_tail: "🚀 Démarrage de la mise à jour STEvE_OS...
".to_string(),
        total_derivations: None,
        current_derivation_index: None,
        current_package_name: None,
    };
    save_update_progress(&initial_state);

    let exe = if Path::new("/run/current-system/sw/bin/steveos-nas-dashboard").exists() {
        PathBuf::from("/run/current-system/sw/bin/steveos-nas-dashboard")
    } else {
        env::current_exe().unwrap_or_else(|_| PathBuf::from("/proc/self/exe"))
    };
    let exe_str = exe.display().to_string();

    let mut extra_args = vec![];
    if force_packages {
        extra_args.push("--force-packages");
    }

    // 1. systemd-run si disponible (cgroup indépendant)
    let systemd_run = "/run/current-system/sw/bin/systemd-run";
    if Path::new(systemd_run).exists() {
        let mut cmd = Command::new(systemd_run);
        let complete_path = "/run/wrappers/bin:/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin";
        cmd.args([
            "--unit=steveos-system-update",
            "--property=KillMode=process",
            &format!("--setenv=PATH={}", complete_path),
            "--setenv=NIX_PATH=nixpkgs=flake:nixpkgs",
            "--description=STEvE_OS System Update Runner",
            "--",
            &exe_str,
            "--run-system-update",
        ]);
        for a in &extra_args {
            cmd.arg(a);
        }
        if let Ok(status) = cmd.status() {
            if status.success() {
                return Ok(());
            }
        }
    }

    // 2. Fallback thread indépendant si systemd-run a échoué
    std::thread::spawn(move || {
        run_detached_update_process(force_packages);
    });

    Ok(())
}

pub fn run_detached_update_process(force_packages: bool) {
    let mut state = get_update_progress();
    state.is_running = true;
    let config_dir = resolve_config_dir();
    let dir_str = config_dir.display().to_string();

    let cached = get_cached_status();
    let update_type = if force_packages {
        UpdateType::PackagesOnly
    } else if let Some(ref c) = cached {
        if c.update_type == UpdateType::None {
            UpdateType::PackagesOnly
        } else {
            c.update_type.clone()
        }
    } else {
        UpdateType::Both
    };

    // --- Étape 1 : Git pull si nécessaire ---
    if update_type == UpdateType::ConfigOnly || update_type == UpdateType::Both {
        state.stage = "git_pull".to_string();
        state.step_index = 1;
        state.total_steps = 4;
        state.progress_percent = 8;
        state.status_title = "Synchronisation de la configuration...".to_string();
        state.status_detail = "Récupération des nouveautés depuis GitHub (steve_os-nix)...".to_string();
        save_update_progress(&state);

        let mut pull_log = String::new();
        if let Err(e) = execute_secure_git_pull(&config_dir, &mut pull_log) {
            state.is_running = false;
            state.stage = "failed".to_string();
            state.status_title = "Échec de synchronisation Git".to_string();
            state.status_detail = e.clone();
            state.error = Some(e);
            state.log_tail.push_str(&pull_log);
            save_update_progress(&state);
            return;
        }
        state.log_tail.push_str(&pull_log);
    }

    // --- Étape 2 : Déploiement système (nixos-rebuild switch) ---
    state.stage = "building".to_string();
    state.step_index = 2;
    state.total_steps = 4;
    state.progress_percent = 28;
    state.status_title = "Construction & téléchargement du système...".to_string();
    state.status_detail = "Compilation des dérivations NixOS et téléchargement des paquets binaires...".to_string();
    save_update_progress(&state);

    let nixos_rebuild = nixos_rebuild_binary();
    let mut args = vec!["switch", "--flake", &dir_str];
    // Ne jamais écraser flake.lock avec --recreate-lock-file :
    // 1. flake.lock a été validé ou mis à jour via Git.
    // 2. --recreate-lock-file est déprécié et provoque un contournement vers le cache tarball Nix (~/.cache/nix/tarballs).
    if force_packages {
        args.push("--refresh");
    }
    let mut cmd = create_switch_command(&nixos_rebuild, &args, &config_dir);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut switch_success = false;
    let mut switch_err = String::new();

    if let Ok(mut child) = cmd.spawn() {
        use std::io::BufRead;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel::<String>();

        let tx_out = tx.clone();
        if let Some(out) = stdout {
            std::thread::spawn(move || {
                let reader = std::io::BufReader::new(out);
                for line in reader.lines().flatten() {
                    let _ = tx_out.send(line);
                }
            });
        }

        let tx_err = tx.clone();
        if let Some(err) = stderr {
            std::thread::spawn(move || {
                let reader = std::io::BufReader::new(err);
                for line in reader.lines().flatten() {
                    let _ = tx_err.send(line);
                }
            });
        }
        drop(tx);

        let mut lines = Vec::new();
        let mut total_derivations: Option<u32> = None;
        let mut current_derivation_index: u32 = 0;

        for line in rx {
            lines.push(line.clone());
            append_live_log(&format!("{}
", line));

            // 1. Détection du nombre total de dérivations
            if line.contains("derivations will be built") {
                if let Some(num_str) = line.split_whitespace().nth(1) {
                    if let Ok(n) = num_str.parse::<u32>() {
                        total_derivations = Some(n);
                        state.total_derivations = Some(n);
                        state.progress_percent = 30.max(state.progress_percent);
                        state.status_detail = format!("{} dérivation(s) à compiler et assembler...", n);
                        save_update_progress(&state);
                    }
                }
            }

            // 2. Détection de compilation unitaire
            if line.contains("building '") && line.contains(".drv'") {
                current_derivation_index += 1;
                state.current_derivation_index = Some(current_derivation_index);

                let pkg_name = if let Some(start) = line.find("/nix/store/") {
                    let sub = &line[start + 11..];
                    if let Some(dash) = sub.find('-') {
                        let after_dash = &sub[dash + 1..];
                        if let Some(end) = after_dash.find(".drv") {
                            after_dash[..end].to_string()
                        } else {
                            after_dash.to_string()
                        }
                    } else {
                        sub.to_string()
                    }
                } else {
                    line.clone()
                };

                state.current_package_name = Some(pkg_name.clone());

                if let Some(total) = total_derivations {
                    let ratio = (current_derivation_index as f32 / total.max(1) as f32).min(1.0);
                    // Échelle Étape 2 (Build NixOS) : de 30% à 68% (calibrée visuellement entre bulle 2 et bulle 3)
                    let calculated = 30 + (ratio * 38.0) as u32;
                    state.progress_percent = calculated.max(state.progress_percent);
                    state.status_title = format!("Construction du système [{}/{}]", current_derivation_index, total);
                    state.status_detail = format!("Compilation de {}...", pkg_name);
                } else {
                    state.progress_percent = 50.max(state.progress_percent);
                    state.status_detail = format!("Compilation de {}...", pkg_name);
                }
                save_update_progress(&state);
            }

            // 3. Détection de téléchargement binaire
            if line.contains("copying path '") || line.contains("paths will be fetched") {
                if let Some(start) = line.find("/nix/store/") {
                    let sub = &line[start + 11..];
                    let pkg_name = sub.split('-').nth(1).unwrap_or(sub);
                    let clean_pkg = pkg_name.trim_end_matches('\'').trim_end_matches("...");
                    state.status_detail = format!("Téléchargement du paquet binaire {}...", clean_pkg);
                } else {
                    state.status_detail = "Téléchargement des paquets binaires du système...".to_string();
                }
                state.progress_percent = 32.max(state.progress_percent);
                save_update_progress(&state);
            }

            // 4. Comparatif de fermeture
            if line.starts_with("<<< /nix/store/") || line.starts_with(">>> /nix/store/") {
                state.progress_percent = 70.max(state.progress_percent);
                state.status_title = "Vérification des paquets modifiés...".to_string();
                state.status_detail = "Comparaison de l'ancienne et de la nouvelle génération...".to_string();
                save_update_progress(&state);
            }

            // 5. Activation système (Étape 3 : Activation)
            if line.contains("Activating configuration") || line.contains("switching to system configuration") || line.contains("setting up /etc") {
                state.stage = "activating".to_string();
                state.step_index = 3;
                state.progress_percent = 75.max(state.progress_percent);
                state.status_title = "Activation de la configuration...".to_string();
                state.status_detail = "Mise à jour du bootloader et démarrage des services...".to_string();
                save_update_progress(&state);
            }

            if state.step_index >= 3 && (line.contains("reloading the following units:") || line.contains("restarting the following units:") || line.contains("starting the following units:")) {
                state.progress_percent = 85.max(state.progress_percent);
                save_update_progress(&state);
            }

            // 6. Redémarrage dashboard (strictement pendant l'activation, jamais pendant le build .drv !)
            if (state.step_index >= 3 || state.stage == "activating") &&
               (line.contains("stopping the following units:") && line.contains("steveos-nas-dashboard") ||
                line.contains("stopping steveos-nas-dashboard") ||
                line.contains("restarting steveos-nas-dashboard")) {
                state.progress_percent = 92.max(state.progress_percent);
                state.dashboard_restarting = true;
                state.status_title = "Redémarrage du Dashboard...".to_string();
                state.status_detail = "Le nouveau service Web prend le relais...".to_string();
                save_update_progress(&state);
            }
        }

        match child.wait() {
            Ok(status) => {
                switch_success = status.success();
                if !status.success() {
                    switch_err = format!("Le processus s'est terminé avec le code {}", status.code().unwrap_or(-1));
                }
            }
            Err(e) => {
                switch_err = e.to_string();
            }
        }

        let full_output = if lines.len() > 80 {
            lines[lines.len() - 80..].join("
")
        } else {
            lines.join("
")
        };
        state.log_tail = full_output;
    } else {
        switch_err = "Impossible de lancer la commande de déploiement.".to_string();
    }

    if switch_success {
        let cur_gen = get_current_system_generation();
        // S'assurer que le service steveos-nas-dashboard est bien relancé avec le nouveau binaire
        let _ = Command::new("/run/current-system/sw/bin/systemctl")
            .args(["try-restart", "steveos-nas-dashboard.service"])
            .status();

        state.is_running = false;
        state.stage = "completed".to_string();
        state.step_index = 4;
        state.total_steps = 4;
        state.progress_percent = 100;
        state.status_title = "Mise à jour terminée avec succès !".to_string();
        state.status_detail = format!("Le système est actif sur la génération {}.", cur_gen.clone().unwrap_or_else(|| "suivante".to_string()));
        state.generation_after = cur_gen;
        state.completed_at = Some(current_time_formatted());
        state.dashboard_restarting = false;
        save_update_progress(&state);
    } else {
        state.is_running = false;
        state.stage = "failed".to_string();
        state.status_title = "Échec de la mise à jour".to_string();
        state.status_detail = switch_err.clone();
        state.error = Some(switch_err);
        save_update_progress(&state);
    }
}


pub fn create_switch_command(bin: &str, args: &[&str], config_dir: &Path) -> Command {
    let is_root = is_root_process();
    let complete_path = "/run/wrappers/bin:/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin";
    if is_root {
        let mut cmd = Command::new(bin);
        cmd.args(args);
        cmd.current_dir(config_dir);
        cmd.env("PATH", complete_path);
        cmd.env("HOME", "/root");
        cmd.env("USER", "root");
        cmd.env("NH_FLAKE", config_dir.display().to_string());
        cmd.env("NH_BYPASS_ROOT_CHECK", "1");
        cmd.env("NH_ELEVATION_STRATEGY", "none");
        cmd
    } else {
        let mut cmd = create_user_command(bin, args);
        cmd.current_dir(config_dir);
        cmd
    }
}

fn run_switch_command(config_dir: &Path, update_inputs: bool) -> (bool, String) {
    let dir_str = config_dir.display().to_string();

    let nixos_rebuild = nixos_rebuild_binary();
    let mut args = vec!["switch", "--flake", &dir_str];
    if update_inputs {
        args.push("--refresh");
    }
    let mut cmd = create_switch_command(&nixos_rebuild, &args, config_dir);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    match cmd.spawn() {
        Ok(mut child) => {
            use std::io::BufRead;
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();

            let stdout_handle = std::thread::spawn(move || {
                let mut out_str = String::new();
                if let Some(out) = stdout {
                    let reader = std::io::BufReader::new(out);
                    for line in reader.lines().flatten() {
                        let l = format!("{}\n", line);
                        append_live_log(&l);
                        out_str.push_str(&l);
                    }
                }
                out_str
            });

            let stderr_handle = std::thread::spawn(move || {
                let mut err_str = String::new();
                if let Some(err) = stderr {
                    let reader = std::io::BufReader::new(err);
                    for line in reader.lines().flatten() {
                        let l = format!("{}\n", line);
                        append_live_log(&l);
                        err_str.push_str(&l);
                    }
                }
                err_str
            });

            let out_text = stdout_handle.join().unwrap_or_default();
            let err_text = stderr_handle.join().unwrap_or_default();
            let mut combined = out_text;
            if !err_text.is_empty() {
                combined.push_str(&err_text);
            }

            let status = child.wait().map(|s| s.success()).unwrap_or(false);
            let cleaned = sanitize_terminal_output(&combined);
            (status, cleaned)
        }
        Err(e) => (false, format!("Impossible d'exécuter {} : {}", nixos_rebuild, e)),
    }
}

fn sanitize_terminal_output(raw: &str) -> String {
    let mut out_lines = Vec::new();
    for line in raw.split('\n') {
        if line.contains('\r') {
            let parts: Vec<&str> = line.split('\r').filter(|p| !p.trim().is_empty()).collect();
            if let Some(last) = parts.last() {
                out_lines.push(last.to_string());
            } else {
                out_lines.push(String::new());
            }
        } else {
            out_lines.push(line.to_string());
        }
    }
    out_lines.join("\n")
}

fn current_time_formatted() -> String {
    let output = Command::new("date")
        .args(["+%H:%M:%S"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "Récemment".into());
    output
}
