use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Deserialize)]
pub struct ExecRequest {
    pub command: String,
    pub cwd: Option<String>,
    pub password: Option<String>,
    pub run_as_root: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ExecResponse {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub cwd: String,
    pub duration_ms: u128,
    pub needs_password: bool,
}

#[derive(Debug, Deserialize)]
pub struct CompleteRequest {
    pub prefix: String,
    pub cwd: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CompleteResponse {
    pub suggestions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SudoAuthRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct SudoStatusResponse {
    pub cached: bool,
    pub user: String,
}

/// Vérifie si l'utilisateur courant dispose actuellement de privilèges sudo mis en cache.
pub fn is_sudo_cached() -> bool {
    let mut check_cmd = crate::updates::create_user_command("sudo", &["-n", "-v"]);
    check_cmd.output().map(|o| o.status.success()).unwrap_or(false)
}

/// Valide le mot de passe utilisateur via PAM / sudo -S -v et rafraîchit le ticket d'horodatage.
pub fn validate_sudo_password(password: &str) -> Result<(), String> {
    let pass = password.trim();
    if pass.is_empty() {
        return Err("Le mot de passe ne peut pas être vide.".to_string());
    }

    let mut val_cmd = if crate::updates::is_root_process() {
        crate::updates::create_user_command("sudo", &["-S", "-v", "-p", ""])
    } else {
        let mut c = Command::new("sudo");
        c.args(["-S", "-v", "-p", ""]);
        c
    };

    val_cmd.stdin(std::process::Stdio::piped());
    val_cmd.stdout(std::process::Stdio::piped());
    val_cmd.stderr(std::process::Stdio::piped());

    let mut child = val_cmd.spawn().map_err(|e| format!("Erreur lancement sudo : {}", e))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = writeln!(stdin, "{}", pass);
    }

    let out = child.wait_with_output().map_err(|e| format!("Erreur exécution sudo : {}", e))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        if err.trim().is_empty() {
            Err("Mot de passe administrateur incorrect ou refusé.".to_string())
        } else {
            Err(err.trim().to_string())
        }
    }
}

/// Révoque immédiatement le ticket de cache sudo (sudo -k).
pub fn drop_sudo_cache() -> Result<(), String> {
    let mut cmd = if crate::updates::is_root_process() {
        crate::updates::create_user_command("sudo", &["-k"])
    } else {
        let mut c = Command::new("sudo");
        c.arg("-k");
        c
    };
    cmd.output().map_err(|e| format!("Erreur révocation sudo : {}", e))?;
    Ok(())
}

/// Fournit le statut d'authentification sudo en cours pour l'utilisateur ciblé.
pub fn sudo_status() -> SudoStatusResponse {
    SudoStatusResponse {
        cached: is_sudo_cached(),
        user: crate::updates::target_user(),
    }
}

pub fn execute_command(req: ExecRequest) -> ExecResponse {
    let start = Instant::now();
    let cmd = req.command.trim();

    let mut current_cwd = resolve_cwd(req.cwd.as_deref());

    if cmd.is_empty() {
        return ExecResponse {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            cwd: current_cwd.display().to_string(),
            duration_ms: start.elapsed().as_millis(),
            needs_password: false,
        };
    }

    // Gestion du cd interactif
    let target_u = crate::updates::target_user();
    let default_home = crate::updates::get_user_home(&target_u).to_string_lossy().to_string();
    if cmd == "cd" || cmd == "cd ~" {
        let raw_home = env::var("HOME").unwrap_or_else(|_| default_home.clone());
        let home = if raw_home == "/root" { default_home.clone() } else { raw_home };
        current_cwd = PathBuf::from(home);
        return ExecResponse {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            cwd: current_cwd.display().to_string(),
            duration_ms: start.elapsed().as_millis(),
            needs_password: false,
        };
    }

    if let Some(target) = cmd.strip_prefix("cd ") {
        let target = target.trim();
        let target_path = if target.starts_with('/') {
            PathBuf::from(target)
        } else if let Some(stripped) = target.strip_prefix("~/") {
            let raw_home = env::var("HOME").unwrap_or_else(|_| default_home.clone());
            let home = if raw_home == "/root" { default_home.clone() } else { raw_home };
            PathBuf::from(home).join(stripped)
        } else {
            current_cwd.join(target)
        };

        if target_path.is_dir() {
            if let Ok(canonical) = target_path.canonicalize() {
                current_cwd = canonical;
                return ExecResponse {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: 0,
                    cwd: current_cwd.display().to_string(),
                    duration_ms: start.elapsed().as_millis(),
                    needs_password: false,
                };
            }
        } else {
            return ExecResponse {
                success: false,
                stdout: String::new(),
                stderr: format!("cd: aucun dossier de ce nom : {}\n", target),
                exit_code: 1,
                cwd: current_cwd.display().to_string(),
                duration_ms: start.elapsed().as_millis(),
                needs_password: false,
            };
        }
    }

    // Gestion de l'élévation de privilèges (sudo & password)
    let is_sudo_cmd = cmd.starts_with("sudo") || req.run_as_root == Some(true);
    let pass_opt = req.password.as_deref().map(|p| p.trim()).filter(|p| !p.is_empty());

    if is_sudo_cmd {
        if let Some(pass) = pass_opt {
            if let Err(err_msg) = validate_sudo_password(pass) {
                return ExecResponse {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("[sudo] Authentification refusée : {}\n", err_msg),
                    exit_code: 1,
                    cwd: current_cwd.display().to_string(),
                    duration_ms: start.elapsed().as_millis(),
                    needs_password: true,
                };
            }
        } else if !is_sudo_cached() {
            return ExecResponse {
                success: false,
                stdout: String::new(),
                stderr: "[sudo] Mot de passe administrateur requis pour cette commande.\n".to_string(),
                exit_code: 1,
                cwd: current_cwd.display().to_string(),
                duration_ms: start.elapsed().as_millis(),
                needs_password: true,
            };
        }
    }

    // Commande effective à exécuter
    let effective_cmd = if req.run_as_root == Some(true) && !cmd.starts_with("sudo") {
        format!("sudo {}", cmd)
    } else {
        cmd.to_string()
    };

    // Exécuter dans Bash avec délimiteur de pwd
    let wrapped_cmd = format!("{}; echo \"__PWD_DELIM__\"; pwd", effective_cmd);

    let bash_bin = if Path::new("/run/current-system/sw/bin/bash").exists() {
        "/run/current-system/sw/bin/bash"
    } else {
        "bash"
    };

    let mut cmd_obj = crate::updates::create_user_command(bash_bin, &["-c", &wrapped_cmd]);
    cmd_obj.current_dir(&current_cwd);
    cmd_obj.stdin(std::process::Stdio::piped());
    cmd_obj.stdout(std::process::Stdio::piped());
    cmd_obj.stderr(std::process::Stdio::piped());

    let child_res = cmd_obj.spawn();
    match child_res {
        Ok(mut child) => {
            if let Some(pass) = pass_opt {
                if let Some(mut stdin) = child.stdin.take() {
                    use std::io::Write;
                    let _ = writeln!(stdin, "{}", pass);
                }
            } else {
                // Fermer stdin pour éviter tout blocage d'un processus attendant une entrée
                drop(child.stdin.take());
            }

            match child.wait_with_output() {
                Ok(out) => {
                    let raw_stdout = String::from_utf8_lossy(&out.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                    let exit_code = out.status.code().unwrap_or(-1);

                    let (clean_stdout, new_pwd) = if let Some(idx) = raw_stdout.rfind("__PWD_DELIM__\n") {
                        let stdout_part = &raw_stdout[..idx];
                        let pwd_part = raw_stdout[idx + "__PWD_DELIM__\n".len()..].trim();
                        (stdout_part.to_string(), pwd_part.to_string())
                    } else if let Some(idx) = raw_stdout.rfind("__PWD_DELIM__") {
                        let stdout_part = &raw_stdout[..idx];
                        let pwd_part = raw_stdout[idx + "__PWD_DELIM__".len()..].trim();
                        (stdout_part.to_string(), pwd_part.to_string())
                    } else {
                        (raw_stdout, current_cwd.display().to_string())
                    };

                    let final_cwd = if !new_pwd.is_empty() && Path::new(&new_pwd).is_dir() {
                        new_pwd
                    } else {
                        current_cwd.display().to_string()
                    };

                    let lower_err = stderr.to_lowercase();
                    let needs_password = lower_err.contains("a terminal is required")
                        || lower_err.contains("no tty present")
                        || lower_err.contains("saisir un mot de passe")
                        || lower_err.contains("password is required");

                    ExecResponse {
                        success: out.status.success(),
                        stdout: clean_stdout,
                        stderr,
                        exit_code,
                        cwd: final_cwd,
                        duration_ms: start.elapsed().as_millis(),
                        needs_password,
                    }
                }
                Err(e) => ExecResponse {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Erreur lors de l'exécution de la commande : {}", e),
                    exit_code: 127,
                    cwd: current_cwd.display().to_string(),
                    duration_ms: start.elapsed().as_millis(),
                    needs_password: false,
                },
            }
        }
        Err(e) => ExecResponse {
            success: false,
            stdout: String::new(),
            stderr: format!("Erreur lors du lancement de bash : {}", e),
            exit_code: 127,
            cwd: current_cwd.display().to_string(),
            duration_ms: start.elapsed().as_millis(),
            needs_password: false,
        },
    }
}

pub fn autocomplete(req: CompleteRequest) -> CompleteResponse {
    let prefix = req.prefix.trim_start();
    let current_cwd = resolve_cwd(req.cwd.as_deref());
    let mut suggestions = Vec::new();

    // 1. Suggestions de commandes courantes & CLI
    let common_cmds = [
        "nh os switch",
        "nh os switch -u",
        "nh os test",
        "nh os build",
        "nh clean all",
        "sudo nh os switch",
        "sudo nh os switch -u",
        "sudo nh os test",
        "sudo nh clean all",
        "sudo systemctl restart ",
        "sudo systemctl status ",
        "sudo systemctl stop ",
        "sudo journalctl -xeu ",
        "sudo nixos-rebuild switch",
        "git status",
        "git pull",
        "git push",
        "git diff",
        "git log -n 5 --oneline",
        "systemctl status",
        "systemctl restart",
        "systemctl list-units --type=service",
        "systemctl --failed",
        "journalctl -u",
        "journalctl -f",
        "docker ps",
        "docker logs",
        "docker restart",
        "zpool status",
        "zfs list",
        "btrfs filesystem show",
        "df -hT",
        "free -h",
        "btop",
        "htop",
        "ip -br a",
        "ss -tulpn",
        "ls -la",
        "cat ",
        "nano ",
        "nvim ",
        "cd ",
        "pwd",
        "uptime",
        "uname -a",
    ];

    for c in common_cmds {
        if c.starts_with(prefix) && c != prefix {
            suggestions.push(c.to_string());
        }
    }

    // 2. Autocomplétion de fichiers / répertoires
    let (search_dir, file_prefix) = if let Some(last_token) = req.prefix.split_whitespace().last() {
        if last_token.contains('/') {
            let p = Path::new(last_token);
            if last_token.ends_with('/') {
                (if last_token.starts_with('/') { PathBuf::from(last_token) } else { current_cwd.join(last_token) }, "")
            } else {
                let parent = p.parent().unwrap_or(Path::new(""));
                let file = p.file_name().and_then(|f| f.to_str()).unwrap_or("");
                (if last_token.starts_with('/') { parent.to_path_buf() } else { current_cwd.join(parent) }, file)
            }
        } else {
            (current_cwd.clone(), last_token)
        }
    } else {
        (current_cwd.clone(), "")
    };

    if let Ok(entries) = fs::read_dir(&search_dir) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if file_prefix.is_empty() || name.starts_with(file_prefix) {
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let suffix = if is_dir { "/" } else { "" };
                    suggestions.push(format!("{}{}", name, suffix));
                }
            }
        }
    }

    suggestions.sort();
    suggestions.dedup();
    if suggestions.len() > 15 {
        suggestions.truncate(15);
    }

    CompleteResponse { suggestions }
}

fn resolve_cwd(cwd_opt: Option<&str>) -> PathBuf {
    if let Some(dir) = cwd_opt {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return p;
        }
    }

    if let Ok(dir) = env::var("NOOS_CONFIG_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return p;
        }
    }

    let p1 = PathBuf::from("/etc/nixos");
    if p1.is_dir() {
        return p1;
    }

    let target_u = crate::updates::target_user();
    let user_home = crate::updates::get_user_home(&target_u);
    let p_noos = user_home.join("Projects/noos-nas");
    if p_noos.is_dir() {
        return p_noos;
    }

    user_home
}
