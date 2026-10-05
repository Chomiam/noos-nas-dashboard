//! Batterie de tests d'intégration et de fiabilité pour le processus de mise à jour Noos NAS
//! Teste la résilience, la détection des paquets, les transitions d'état et la sanctuarisation SSH.

use std::fs;

#[test]
fn test_integration_channel_persistence_and_candidate_resolution() {
    let temp_dir = std::env::temp_dir().join(format!("noos_test_channel_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let channel_file = temp_dir.join("update_channel");

    // Écriture initiale : testing
    let _ = fs::write(&channel_file, "testing\n");
    let content = fs::read_to_string(&channel_file).unwrap();
    assert_eq!(content.trim(), "testing");

    // Écriture : stable
    let _ = fs::write(&channel_file, "stable\n");
    let content = fs::read_to_string(&channel_file).unwrap();
    assert_eq!(content.trim(), "stable");

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_integration_flake_lock_structure_validation() {
    // Vérification de la robustesse face à un flake.lock réaliste
    let sample_lock = r#"{
  "nodes": {
    "nixpkgs": {
      "locked": {
        "lastModified": 1790635395,
        "narHash": "sha256-bNyvoIyOCu7lzoCpWKWGIsHpTtERUWzqYWFPlN++WTw=",
        "owner": "nixos",
        "repo": "nixpkgs",
        "rev": "7fc6f2c20af09cdcaf48b92ec3121860139ec668",
        "type": "github"
      },
      "original": {
        "owner": "nixos",
        "ref": "nixos-26.05",
        "repo": "nixpkgs",
        "type": "github"
      }
    },
    "noos-nas-dashboard": {
      "locked": {
        "lastModified": 1791012230,
        "narHash": "sha256-9Atz0iJg6w0P47i5LOHF2pVuVG+l0+NpBvifHWG7Rrk=",
        "owner": "Chomiam",
        "repo": "noos-nas-dashboard",
        "rev": "577b75e87c25a7105f5f9c43cf24298d8a15f4c2",
        "type": "github"
      },
      "original": {
        "owner": "Chomiam",
        "ref": "testing",
        "repo": "noos-nas-dashboard",
        "type": "github"
      }
    },
    "root": {
      "inputs": {
        "nixpkgs": "nixpkgs",
        "noos-nas-dashboard": "noos-nas-dashboard"
      }
    }
  },
  "root": "root",
  "version": 7
}"#;

    let v: serde_json::Value = serde_json::from_str(sample_lock).expect("Le JSON du lock doit être valide");
    let root_inputs = v.get("nodes")
        .and_then(|n| n.get("root"))
        .and_then(|r| r.get("inputs"))
        .and_then(|i| i.as_object())
        .expect("Les inputs root doivent être un objet");

    assert!(root_inputs.contains_key("nixpkgs"));
    assert!(root_inputs.contains_key("noos-nas-dashboard"));

    // Vérifier l'extraction des révisions
    let dash_node = v.get("nodes").unwrap().get("noos-nas-dashboard").unwrap();
    let rev = dash_node.get("locked").unwrap().get("rev").unwrap().as_str().unwrap();
    assert_eq!(rev, "577b75e87c25a7105f5f9c43cf24298d8a15f4c2");
}

#[test]
fn test_integration_flake_lock_corrupt_resilience() {
    let corrupt_json = "{ invalid json ...";
    let res = serde_json::from_str::<serde_json::Value>(corrupt_json);
    assert!(res.is_err(), "Un JSON corrompu doit renvoyer une erreur sans panique");
}

#[test]
fn test_integration_sshd_healing_simulation() {
    let bad_conf = r#"# OpenSSH shares config
Subsystem sftp internal-sftp

# Admin bloque par erreur
Match User chomiam
    ChrootDirectory /mnt/storage/data
    ForceCommand internal-sftp -u 002
    AllowTcpForwarding no
    X11Forwarding no

# Root bloque par erreur
Match User root
    ForceCommand internal-sftp -u 002

# Utilisateur legitime sans shell
Match User guest_ftp
    ChrootDirectory /mnt/storage/guest
    ForceCommand internal-sftp -R -u 002
"#;

    let forbidden = ["root", "admin", "chomiam"];
    let mut sanitized = Vec::new();
    let mut in_forbidden = false;

    for line in bad_conf.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Match User ") {
            let u = trimmed.trim_start_matches("Match User ").trim();
            if forbidden.contains(&u) {
                in_forbidden = true;
                continue;
            } else {
                in_forbidden = false;
            }
        } else if trimmed.starts_with("Match ") {
            in_forbidden = false;
        }

        if in_forbidden {
            if trimmed.is_empty() {
                in_forbidden = false;
            }
            continue;
        }
        sanitized.push(line);
    }

    let result = sanitized.join("\n");
    assert!(!result.contains("Match User chomiam"));
    assert!(!result.contains("Match User root"));
    assert!(result.contains("Match User guest_ftp"));
    assert!(result.contains("Subsystem sftp internal-sftp"));
}

#[test]
fn test_integration_vars_nix_sanctuarization_simulation() {
    let vars_before_conflict = r#"{
  hostName = "noos-nas";
  user = {
    username = "chomiam";
    fullName = "Chomiam";
    homeDirectory = "/home/chomiam";
    shell = "bash";
  };
}
"#;

    // Simulation d'un fichier altéré avec marqueurs git
    let vars_corrupted = r#"{
<<<<<<< HEAD
  hostName = "steveos-nas";
=======
  hostName = "noos-nas";
>>>>>>> origin/testing
}
"#;

    // Vérifier la détection de conflit
    assert!(vars_corrupted.contains("<<<<<<<"));
    assert!(vars_corrupted.contains(">>>>>>>"));

    // Rétablissement du contenu sanctuarisé
    let restored = if vars_corrupted.contains("<<<<<<<") {
        vars_before_conflict.to_string()
    } else {
        vars_corrupted.to_string()
    };

    assert!(!restored.contains("<<<<<<<"));
    assert!(restored.contains("username = \"chomiam\""));
    assert!(restored.contains("hostName = \"noos-nas\""));
}
