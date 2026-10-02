//! # Moteur de Gestion de Fichiers & Système de Fichiers (Noos Files)
//!
//! Ce module centralise l'ensemble des opérations d'E/S sur le système de fichiers :
//! - **Navigation & Arborescence** : Exploration de dossiers avec calcul de taille,
//!   permissions Unix formatées et classification visuelle (Catppuccin Mocha).
//! - **Sécurité & Intégrité** :
//!   - Détection et protection absolue des points de montage Linux (`/proc/mounts`,
//!     points de montage déclarés dans `mounts.json` et racines système).
//!   - Prévention des suppressions ou renommages accidentels de disques montés.
//!   - Normalisation et canonisation des chemins utilisateur contre les traversées.
//! - **Opérations CRUD** : Création de dossiers, renommage, déplacement intelligent
//!   (rename atomique sur même système de fichiers ou copie récursive + purge),
//!   copie et suppression (redirection corbeille ou destruction définitive).
//! - **Visualisation & Édition** : Lecture et enregistrement de fichiers texte/code
//!   avec gestion des limites de taille (5 Mo max pour éviter la saturation mémoire).
//! - **Moteur Multimédia** : Génération de vignettes et extraction de métadonnées EXIF
//!   pour photos RAW, HEIC (iPhone) et formats Web natifs.
//! - **Moteur d'Archives** : Inspection, compression multi-formats (.zip, .7z, .tar.gz,
//!   .tar.xz, .tar.zst) avec chiffrement AES256 optionnel et extraction automatique.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ============================================================================
// 1. STRUCTURES DE DONNÉES & CONTRATS D'ÉCHANGE (REQUESTS / RESPONSES)
// ============================================================================

/// Représentation détaillée d'une entrée de système de fichiers (fichier ou dossier).
#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntry {
    /// Nom du fichier ou dossier (ex: `rapport.pdf` ou `photos`).
    pub name: String,
    /// Chemin absolu complet vers l'élément dans le système de fichiers.
    pub path: String,
    /// Indique s'il s'agit d'un répertoire (`true`) ou d'un fichier régulier (`false`).
    pub is_dir: bool,
    /// Taille en octets (pour les dossiers, fixée à 0).
    pub size_bytes: u64,
    /// Taille formatée pour lecture humaine (ex: `12.4 Mo`, `2.1 Go`).
    pub size_human: String,
    /// Date et heure de dernière modification formatée (ex: `02/10/2026 04:36`).
    pub modified: String,
    /// Droits d'accès Unix en notation octale (ex: `755`, `644`).
    pub permissions: String,
    /// Catégorie visuelle pour l'interface :
    /// `"folder"`, `"image"`, `"video"`, `"audio"`, `"document"`, `"archive"`, `"code"`, `"file"`
    pub category: String,
    /// Flag de sécurité indiquant si ce dossier correspond à un point de montage de disque actif.
    /// Si `true`, la suppression et le renommage sont formellement interdits dans l'API.
    #[serde(default)]
    pub is_mount_point: bool,
}

/// Résultat complet de l'exploration d'un répertoire.
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryListing {
    /// Chemin canonique absolu du répertoire actuellement listé.
    pub current_path: String,
    /// Chemin du dossier parent pour la navigation arrière (None si racine).
    pub parent_path: Option<String>,
    /// Liste des éléments enfants (dossiers en premier, puis fichiers triés alphabétiquement).
    pub entries: Vec<FileEntry>,
    /// Nombre total d'éléments dans le dossier.
    pub total_items: usize,
    /// Taille cumulée totale des fichiers du dossier en octets.
    pub total_size_bytes: u64,
}

/// Paramètres de requête GET pour lister un répertoire.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Chemin optionnel demandé. Si omis, le dossier personnel de l'utilisateur est utilisé.
    pub path: Option<String>,
}

/// Requête de création d'un nouveau dossier.
#[derive(Debug, Deserialize)]
pub struct MkdirRequest {
    /// Dossier parent dans lequel créer le sous-dossier.
    pub path: String,
    /// Nom du dossier à créer.
    pub name: String,
}

/// Requête de suppression d'un élément (fichier ou dossier).
#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    /// Chemin absolu de l'élément à supprimer.
    pub path: String,
    /// Si `true`, supprime définitivement l'élément (`rm -rf`).
    /// Si `false` ou omis, déplace l'élément vers la corbeille sécurisée Noos.
    pub permanent: Option<bool>,
}

/// Requête de renommage d'un fichier ou d'un dossier.
#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    /// Chemin actuel de l'élément.
    pub path: String,
    /// Nouveau nom désiré (doit être un nom simple sans slash).
    pub new_name: String,
}

/// Requête générique de transfert (copie ou déplacement).
#[derive(Debug, Deserialize)]
pub struct ActionRequest {
    /// Chemin absolu de la source.
    pub src_path: String,
    /// Chemin absolu du répertoire de destination.
    pub dest_dir: String,
}

// ============================================================================
// 2. DÉTECTION & PROTECTION DES POINTS DE MONTAGE DU NOYAU LINUX
// ============================================================================

/// Décode les séquences d'échappement octales utilisées par le noyau Linux dans `/proc/mounts`.
///
/// Par exemple, un point de montage contenant un espace `/mnt/Disque 1` est encodé
/// sous la forme `/mnt/Disque\0401`. Cette fonction restaure les caractères originaux.
fn unescape_proc_mount_path(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let mut octal = String::new();
            for _ in 0..3 {
                if let Some(&next_c) = chars.peek() {
                    if ('0'..='7').contains(&next_c) {
                        octal.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
            }
            if octal.len() == 3 {
                if let Ok(byte) = u8::from_str_radix(&octal, 8) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('\\');
            result.push_str(&octal);
        } else {
            result.push(c);
        }
    }
    result
}

/// Agrège l'ensemble des points de montage actifs et sensibles du système à travers 3 niveaux de détection :
/// 1. **Racines système critiques** (`/`, `/bin`, `/boot`, `/nix`, `/mnt`, `/etc`, etc.)
/// 2. **Points de montage déclarés** dans la configuration persistée (`mounts.json`)
/// 3. **Points de montage actifs du noyau** lus directement depuis `/proc/mounts` ou `/proc/self/mounts`
///
/// Renvoie un ensemble `HashSet<PathBuf>` de chemins canoniques et résolus.
pub fn get_all_mount_points() -> HashSet<PathBuf> {
    let mut set = HashSet::new();

    // 1. Racines système critiques protégées
    for sys in &[
        "/", "/bin", "/boot", "/dev", "/etc", "/home", "/lib", "/lib64",
        "/lost+found", "/media", "/mnt", "/nix", "/opt", "/proc", "/root",
        "/run", "/sbin", "/srv", "/sys", "/tmp", "/usr", "/var"
    ] {
        let pb = PathBuf::from(sys);
        if let Ok(c) = pb.canonicalize() {
            set.insert(c);
        }
        set.insert(pb);
    }

    // 2. Points de montage déclarés et persistés (mounts.json)
    for pm in crate::storage::load_persisted_mounts() {
        let trimmed = pm.mount_point.trim();
        if !trimmed.is_empty() {
            let normalized = if trimmed.len() > 1 {
                trimmed.trim_end_matches('/')
            } else {
                trimmed
            };
            let pb = PathBuf::from(normalized);
            if let Ok(c) = pb.canonicalize() {
                set.insert(c);
            }
            set.insert(pb);
        }
    }

    // 3. Points de montage actifs dans le noyau (/proc/mounts ou /proc/self/mounts)
    for mounts_file in &["/proc/mounts", "/proc/self/mounts"] {
        if let Ok(content) = fs::read_to_string(mounts_file) {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let mp_unescaped = unescape_proc_mount_path(parts[1]);
                    let trimmed = if mp_unescaped.len() > 1 {
                        mp_unescaped.trim_end_matches('/')
                    } else {
                        &mp_unescaped
                    };
                    let pb = PathBuf::from(trimmed);
                    if let Ok(c) = pb.canonicalize() {
                        set.insert(c);
                    }
                    set.insert(pb);
                }
            }
            break;
        }
    }

    set
}

/// Vérifie de façon infaillible si un dossier correspond à un point de montage de système de fichiers.
///
/// Combine deux techniques complémentaires :
/// - Vérification d'appartenance dans le cache des points de montage (`get_all_mount_points`).
/// - Inspection bas-niveau des métadonnées d'inode Unix (`MetadataExt::dev() != parent_meta.dev()`),
///   permettant de détecter tout franchissement de partition physique même pour un montage éphémère.
pub fn is_mount_point(path: &Path) -> bool {
    let canonical = match path.canonicalize() {
        Ok(c) => c,
        Err(_) => path.to_path_buf(),
    };

    let mount_points = get_all_mount_points();
    if mount_points.contains(&canonical) || mount_points.contains(path) {
        return true;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if canonical.is_dir() {
            if let Ok(meta) = fs::metadata(&canonical) {
                if let Some(parent) = canonical.parent() {
                    if let Ok(parent_meta) = fs::metadata(parent) {
                        if meta.dev() != parent_meta.dev() {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

// ============================================================================
// 3. SÉCURITÉ DES CHEMINS, CANONISATION & NORMALISATION UTILISATEUR
// ============================================================================

/// Normalise un chemin fourni par le client web pour garantir la robustesse de l'accès :
/// 1. **Redirection utilisateur** : Redirige automatiquement tout chemin pointant
///    vers `/home/<ancien_user>` vers le dossier personnel de l'utilisateur actif.
/// 2. **Compatibilité XDG** : Tente la résolution des variantes avec majuscules et accents
///    (ex: `Téléchargements` vs `telechargements`, `Vidéos` vs `videos`).
/// 3. **Résolution tolérante** : Effectue une recherche insensible à la casse et sans
///    accents dans le répertoire parent si le chemin exact n'existe pas.
pub fn normalize_user_path(target: PathBuf) -> PathBuf {
    if target.exists() {
        return target;
    }

    let target_u = crate::updates::target_user();
    let user_home = crate::updates::get_user_home(&target_u);

    // 0. Redirection automatique des dossiers /home/<utilisateur> inexistants vers le dossier de l'utilisateur actif
    if target.starts_with("/home") {
        if let Ok(rel) = target.strip_prefix("/home") {
            let mut components = rel.components();
            if let Some(first) = components.next() {
                let first_name = first.as_os_str().to_string_lossy();
                if first_name != target_u {
                    let subpath: PathBuf = components.collect();
                    let redirected = if subpath.as_os_str().is_empty() {
                        user_home.clone()
                    } else {
                        user_home.join(&subpath)
                    };
                    if redirected.exists() {
                        return redirected;
                    }
                }
            }
        }
    }

    let path_str = target.to_string_lossy().to_string();

    // 1. Remplacement direct des anciens noms avec majuscules et accents
    let lower_variant = path_str
        .replace("/Documents", "/documents")
        .replace("/Images", "/images")
        .replace("/Vidéos", "/videos")
        .replace("/Videos", "/videos")
        .replace("/Musique", "/musique")
        .replace("/Music", "/musique")
        .replace("/Téléchargements", "/telechargements")
        .replace("/Telechargements", "/telechargements")
        .replace("/Downloads", "/telechargements").replace("/downloads", "/telechargements")
        .replace("/Pictures", "/images");

    let p_lower = PathBuf::from(&lower_variant);
    if p_lower.exists() {
        return p_lower;
    }

    // 2. Recherche insensible à la casse et sans accents dans le dossier parent
    if let (Some(parent), Some(file_name)) = (target.parent(), target.file_name()) {
        if parent.is_dir() {
            let target_str = file_name.to_string_lossy().to_lowercase();
            let target_clean: String = target_str
                .replace('é', "e")
                .replace('è', "e")
                .replace('ê', "e")
                .replace('à', "a");

            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let entry_name = entry.file_name().to_string_lossy().to_lowercase();
                    let entry_clean: String = entry_name
                        .replace('é', "e")
                        .replace('è', "e")
                        .replace('ê', "e")
                        .replace('à', "a");
                    if entry_name == target_str || entry_clean == target_clean {
                        return entry.path();
                    }
                }
            }
        }
    }

    target
}

// ============================================================================
// 4. EXPLORATION DE DOSSIER, MÉTADONNÉES & CATÉGORISATION
// ============================================================================

/// Liste le contenu d'un répertoire avec calcul des métadonnées étendues :
/// - Résout le chemin cible avec repli vers le dossier personnel de l'utilisateur actif.
/// - Calcule la taille, les dates de modification et les permissions Unix octales.
/// - Détecte si un sous-dossier correspond à un point de montage de disque (`is_mount_point`).
/// - Trie les entrées de façon ergonomique : répertoires en tête, puis fichiers
///   par ordre alphabétique insensible à la casse.
pub fn list_directory(req_path: Option<&str>) -> Result<DirectoryListing, String> {
    let target_u = crate::updates::target_user();
    let user_home = crate::updates::get_user_home(&target_u).to_string_lossy().to_string();
    let raw_home = env::var("HOME").unwrap_or_else(|_| user_home.clone());
    let home = if raw_home == "/root" { user_home.clone() } else { raw_home };
    let raw_target = req_path
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&home));

    let target = normalize_user_path(raw_target);

    let canonical = match target.canonicalize() {
        Ok(c) => c,
        Err(e) => {
            // Repli gracieux vers user_home si le dossier demandé est introuvable
            let fallback_home = crate::updates::get_user_home(&target_u);
            if target != fallback_home && fallback_home.exists() {
                fallback_home.canonicalize().unwrap_or(fallback_home)
            } else {
                return Err(format!("Impossible d'accéder au dossier {} : {}", target.display(), e));
            }
        }
    };

    if !canonical.is_dir() {
        return Err(format!("Le chemin {} n'est pas un dossier valide.", canonical.display()));
    }

    let parent_path = canonical.parent().map(|p| p.display().to_string());

    let mut entries = Vec::new();
    let mut total_size: u64 = 0;

    let mount_points = get_all_mount_points();
    #[cfg(unix)]
    let parent_dev = fs::metadata(&canonical).ok().map(|m| {
        use std::os::unix::fs::MetadataExt;
        m.dev()
    });

    let dir_entries = fs::read_dir(&canonical)
        .map_err(|e| format!("Erreur lors de la lecture du dossier : {}", e))?;

    for item in dir_entries.flatten() {
        let file_name = item.file_name().to_string_lossy().to_string();

        let path = item.path();
        let path_str = path.display().to_string();

        let metadata = item.metadata().ok();
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = if is_dir { 0 } else { metadata.as_ref().map(|m| m.len()).unwrap_or(0) };
        total_size += size;

        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(format_system_time)
            .unwrap_or_else(|| "--".to_string());

        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            metadata.as_ref().map(|m| format!("{:o}", m.permissions().mode() & 0o777)).unwrap_or_else(|| "755".into())
        };
        #[cfg(not(unix))]
        let permissions = "755".to_string();

        let category = if is_dir {
            "folder".to_string()
        } else {
            categorize_file(&file_name)
        };

        let is_mount_point = if is_dir {
            let item_canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if mount_points.contains(&item_canonical) || mount_points.contains(&path) {
                true
            } else {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if let (Some(p_dev), Some(meta)) = (parent_dev, metadata.as_ref()) {
                        meta.dev() != p_dev
                    } else {
                        false
                    }
                }
                #[cfg(not(unix))]
                false
            }
        } else {
            false
        };

        entries.push(FileEntry {
            name: file_name,
            path: path_str,
            is_dir,
            size_bytes: size,
            size_human: if is_dir { "--".into() } else { format_size(size) },
            modified,
            permissions,
            category,
            is_mount_point,
        });
    }

    // Tri : dossiers en premier, puis fichiers par ordre alphabétique
    entries.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    let count = entries.len();

    Ok(DirectoryListing {
        current_path: canonical.display().to_string(),
        parent_path,
        entries,
        total_items: count,
        total_size_bytes: total_size,
    })
}

// ============================================================================
// 5. OPÉRATIONS CRUD SUR LES FICHIERS (CRÉATION, SUPPRESSION, RENOMMAGE)
// ============================================================================

/// Crée un nouveau dossier de manière sécurisée en validant le nom
/// (rejet des caractères interdits `/`, `.`, `..`).
pub fn create_directory(base_dir: &str, dir_name: &str) -> Result<String, String> {
    let name = dir_name.trim();
    if name.is_empty() || name.contains('/') || name == "." || name == ".." {
        return Err("Nom de dossier invalide.".into());
    }

    let norm_base = normalize_user_path(PathBuf::from(base_dir));
    let p = norm_base.join(name);
    if p.exists() {
        return Err("Un fichier ou dossier porte déjà ce nom.".into());
    }

    fs::create_dir_all(&p)
        .map_err(|e| format!("Impossible de créer le dossier : {}", e))?;

    Ok(format!("Dossier '{}' créé avec succès.", name))
}

/// Supprime un fichier ou un dossier :
/// - **Protection critique** : Refuse formellement la suppression si le dossier
///   correspond à un point de montage de disque protégé (`is_mount_point`).
/// - Si `permanent == false`, délègue le déplacement sécurisé vers la corbeille Noos.
/// - Si `permanent == true`, procède à une suppression définitive directe (`rm -rf`).
pub fn delete_item(item_path: &str, permanent: bool) -> Result<String, String> {
    let p = normalize_user_path(PathBuf::from(item_path));
    if !p.exists() {
        return Err("Fichier ou dossier introuvable.".into());
    }

    let canonical = match p.canonicalize() {
        Ok(c) => c,
        Err(_) => p.clone(),
    };

    if is_mount_point(&canonical) || is_mount_point(&p) {
        let name = canonical.file_name().and_then(|f| f.to_str()).unwrap_or(item_path);
        return Err(format!("Suppression interdite : le dossier '{}' est un point de montage de disque protégé.", name));
    }

    if permanent {
        if canonical.is_dir() {
            fs::remove_dir_all(&canonical)
                .map_err(|e| format!("Échec de suppression du dossier : {}", e))?;
            Ok(format!("Dossier {} supprimé définitivement.", canonical.file_name().and_then(|f| f.to_str()).unwrap_or("")))
        } else {
            fs::remove_file(&canonical)
                .map_err(|e| format!("Échec de suppression du fichier : {}", e))?;
            Ok(format!("Fichier {} supprimé définitivement.", canonical.file_name().and_then(|f| f.to_str()).unwrap_or("")))
        }
    } else {
        crate::trash::move_to_trash(item_path)
    }
}

/// Renomme un fichier ou un dossier :
/// - **Protection critique** : Refuse formellement le renommage si l'élément correspond
///   à un point de montage de disque protégé.
/// - Empêche l'écrasement involontaire si le nom de destination existe déjà.
pub fn rename_item(item_path: &str, new_name: &str) -> Result<String, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() || new_name.contains('/') || new_name == "." || new_name == ".." {
        return Err("Nouveau nom invalide.".into());
    }

    let p = normalize_user_path(PathBuf::from(item_path));
    if !p.exists() {
        return Err("Élément introuvable.".into());
    }

    let canonical = match p.canonicalize() {
        Ok(c) => c,
        Err(_) => p.clone(),
    };

    if is_mount_point(&canonical) || is_mount_point(&p) {
        let name = canonical.file_name().and_then(|f| f.to_str()).unwrap_or(item_path);
        return Err(format!("Renommage interdit : le dossier '{}' est un point de montage de disque protégé.", name));
    }

    let parent = p.parent().ok_or("Impossible de trouver le dossier parent.")?;
    let target = parent.join(new_name);

    if target.exists() {
        return Err("Un fichier ou dossier porte déjà ce nom dans ce répertoire.".into());
    }

    fs::rename(p, &target)
        .map_err(|e| format!("Erreur lors du renommage : {}", e))?;

    Ok(format!("Renommé en '{}' avec succès.", new_name))
}

// ============================================================================
// 6. TRANSFERTS DE FICHIERS (COPIE & DÉPLACEMENT)
// ============================================================================

/// Génère un chemin cible unique dans le dossier de destination en évitant
/// tout écrasement accidentel. Si le nom existe déjà, ajoute un suffixe
/// incrémental : `fichier (copie).ext`, `fichier (copie 2).ext`, etc.
fn get_unique_target_path(dest_folder: &Path, file_name: &str) -> PathBuf {
    let original = dest_folder.join(file_name);
    if !original.exists() {
        return original;
    }

    let p = Path::new(file_name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(file_name);
    let ext = p.extension().and_then(|s| s.to_str());

    let mut counter = 1;
    loop {
        let candidate_name = if counter == 1 {
            match ext {
                Some(e) => format!("{} (copie).{}", stem, e),
                None => format!("{} (copie)", stem),
            }
        } else {
            match ext {
                Some(e) => format!("{} (copie {}).{}", stem, counter, e),
                None => format!("{} (copie {})", stem, counter),
            }
        };

        let candidate_path = dest_folder.join(&candidate_name);
        if !candidate_path.exists() {
            return candidate_path;
        }
        counter += 1;
    }
}

/// Copie un fichier ou un dossier de façon récursive vers un dossier cible :
/// - Vérifie l'existence de la source et la validité du répertoire de destination.
/// - Calcule un nom cible unique pour ne jamais écraser de données existantes.
/// - Prévient toute copie récursive d'un dossier dans lui-même.
pub fn copy_item(src: &str, dest_dir: &str) -> Result<String, String> {
    let src_path = normalize_user_path(PathBuf::from(src));
    let dest_folder = normalize_user_path(PathBuf::from(dest_dir));

    if !src_path.exists() {
        return Err("Source introuvable.".into());
    }
    if !dest_folder.is_dir() {
        return Err("Dossier de destination invalide.".into());
    }

    let file_name = src_path.file_name().and_then(|f| f.to_str()).ok_or("Nom de fichier source invalide.")?;
    let target = get_unique_target_path(&dest_folder, file_name);

    // Éviter de copier un dossier dans lui-même
    if src_path.is_dir() {
        if target.starts_with(&src_path) {
            return Err("Impossible de copier un dossier à l'intérieur de lui-même.".into());
        }
        copy_dir_recursive(&src_path, &target)
            .map_err(|e| format!("Erreur lors de la copie du dossier : {}", e))?;
    } else {
        fs::copy(&src_path, &target)
            .map_err(|e| format!("Erreur lors de la copie du fichier : {}", e))?;
    }

    Ok(format!("Élément copié avec succès sous '{}'.", target.file_name().and_then(|f| f.to_str()).unwrap_or(file_name)))
}

/// Déplace un fichier ou un répertoire vers une destination :
/// - **Protection critique** : Refuse le déplacement de points de montage système.
/// - Exécute un renommage atomique instantané (`fs::rename`) si la destination se situe sur la même partition.
/// - Bascule automatiquement en mode copie récursive suivie d'une suppression si le déplacement
///   traverse des systèmes de fichiers physiques distincts (ex: de `/home` vers `/mnt/nvme1`).
pub fn move_item(src: &str, dest_dir: &str) -> Result<String, String> {
    let src_path = normalize_user_path(PathBuf::from(src));
    let dest_folder = normalize_user_path(PathBuf::from(dest_dir));

    if !src_path.exists() {
        return Err("Source introuvable.".into());
    }
    if !dest_folder.is_dir() {
        return Err("Dossier de destination invalide.".into());
    }

    let canonical_src = match src_path.canonicalize() {
        Ok(c) => c,
        Err(_) => src_path.clone(),
    };

    if is_mount_point(&canonical_src) || is_mount_point(&src_path) {
        let name = canonical_src.file_name().and_then(|f| f.to_str()).unwrap_or(src);
        return Err(format!("Déplacement interdit : le dossier '{}' est un point de montage de disque protégé.", name));
    }

    let file_name = src_path.file_name().and_then(|f| f.to_str()).ok_or("Nom de fichier source invalide.")?;
    let default_target = dest_folder.join(file_name);

    if default_target == src_path {
        return Ok("L'élément se trouve déjà dans le dossier de destination.".into());
    }

    let target = get_unique_target_path(&dest_folder, file_name);

    // Essai avec rename (rapide sur même partition)
    if fs::rename(&src_path, &target).is_err() {
        // Fallback copie + suppression (si partitions différentes)
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &target)
                .map_err(|e| format!("Erreur lors de la copie du dossier : {}", e))?;
            let _ = fs::remove_dir_all(&src_path);
        } else {
            fs::copy(&src_path, &target)
                .map_err(|e| format!("Erreur lors de la copie du fichier : {}", e))?;
            let _ = fs::remove_file(&src_path);
        }
    }

    Ok(format!("Élément déplacé avec succès sous '{}'.", target.file_name().and_then(|f| f.to_str()).unwrap_or(file_name)))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Détermine la catégorie d'un fichier en fonction de son nom ou de son extension pour
/// l'attribution des icônes, des actions contextuelles et des couleurs de l'interface Catppuccin Mocha.
///
/// Catégories possibles : `"folder"`, `"image"`, `"video"`, `"audio"`, `"document"`, `"archive"`, `"code"`, `"file"`
pub fn categorize_file(name: &str) -> String {
    let lower = name.to_lowercase();

    // Fichiers speciaux / dotfiles connus
    match lower.as_str() {
        "makefile" | "dockerfile" | "containerfile" | "justfile" | "rakefile" | "gemfile"
        | "cmakelists.txt" | "license" | "readme" | ".gitignore" | ".gitattributes"
        | ".bashrc" | ".bash_profile" | ".profile" | ".zshrc" | ".zshenv" | ".fishrc"
        | ".vimrc" | ".nanorc" | ".editorconfig" | ".env" | ".flake-lock" | "flake.lock"
        | "cargo.lock" | "package-lock.json" => return "code".into(),
        _ => {}
    }

    let ext = lower.split('.').last().unwrap_or("");

    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tiff" | "tif" | "ico"
        | "heic" | "heif" | "hif" | "avif" | "jxl"
        | "nef" | "nrw" | "cr2" | "cr3" | "crw" | "arw" | "srf" | "sr2"
        | "dng" | "raf" | "rw2" | "orf" | "pef" | "3fr" | "psd" | "raw"
        | "pcx" | "tga" | "targa" | "dds" => "image".into(),
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" => "video".into(),
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "opus" | "wma" => "audio".into(),
        "pdf" | "doc" | "docx" | "odt" | "rtf" | "xls" | "xlsx" | "ods" | "csv" | "pptx" | "ppt" | "odp" => "document".into(),
        "txt" | "md" | "markdown" | "rst" | "org" | "tex" | "tsv" | "log" => "document".into(),
        "zip" | "tar" | "gz" | "xz" | "bz2" | "7z" | "rar" | "zst" | "iso" => "archive".into(),
        "fish" | "nix" | "rs" | "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "html" | "htm"
        | "css" | "scss" | "sass" | "less" | "json" | "json5" | "jsonc" | "toml" | "yaml" | "yml"
        | "sh" | "bash" | "zsh" | "nu" | "ksh" | "csh" | "py" | "c" | "cpp" | "cc" | "cxx" | "h" | "hpp"
        | "go" | "lua" | "vim" | "sql" | "php" | "rb" | "xml" | "conf" | "config" | "ini" | "cfg"
        | "service" | "timer" | "target" | "socket" | "desktop" | "env" | "diff" | "patch" | "lock" => "code".into(),
        _ => "file".into(),
    }
}

/// Convertit une quantité brute d'octets en chaîne lisible avec unité adaptée :
/// `To`, `Go`, `Mo`, `Ko` ou `o`.
pub fn format_size(bytes: u64) -> String {
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

/// Formate un instant `SystemTime` en chaîne de date/heure standard `YYYY-MM-DD HH:MM`.
fn format_system_time(time: SystemTime) -> String {
    let duration = time.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();

    let days = secs / 86400;
    let rem_secs = secs % 86400;
    let hours = rem_secs / 3600;
    let mins = (rem_secs % 3600) / 60;

    let year = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = (day_of_year / 30).min(11) + 1;
    let day = (day_of_year % 30) + 1;

    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hours, mins)
}

// ============================================================================
// 7. ÉDITION & LECTURE DE FICHIERS TEXTE / CODE
// ============================================================================

/// Réponse retournée lors de la lecture d'un fichier texte pour l'éditeur du dashboard.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadFileResponse {
    /// Contenu textuel décodé en UTF-8 (permissif via lossy).
    pub content: String,
    /// Chemin absolu du fichier.
    pub path: String,
    /// Nom du fichier.
    pub name: String,
    /// Taille totale du fichier sur disque en octets.
    pub size_bytes: u64,
    /// Indique si le contenu a été tronqué pour des raisons de performance et de sécurité mémoire.
    pub is_truncated: bool,
}

/// Paramètres de requête pour la lecture d'un fichier texte.
#[derive(Debug, Deserialize)]
pub struct ReadFileQuery {
    pub path: String,
}

/// Requête d'enregistrement d'un fichier texte modifié depuis l'éditeur.
#[derive(Debug, Deserialize)]
pub struct WriteFileRequest {
    pub path: String,
    pub content: String,
}

/// Lit le contenu texte d'un fichier pour l'éditeur contextuel web :
/// - **Protection mémoire** : Si le fichier dépasse 5 Mo, les premiers 5 Mo sont
///   lus et le flag `is_truncated = true` est positionné pour avertir l'utilisateur.
pub fn read_file_content(path_str: &str) -> Result<ReadFileResponse, String> {
    let p = normalize_user_path(PathBuf::from(path_str));
    if !p.exists() || !p.is_file() {
        return Err("Fichier introuvable.".into());
    }
    let metadata = fs::metadata(&p).map_err(|e| e.to_string())?;
    let size = metadata.len();

    const MAX_SIZE: u64 = 5 * 1024 * 1024; // 5 Mo max pour affichage texte
    let (content, is_truncated) = if size > MAX_SIZE {
        use std::io::Read;
        let mut file = fs::File::open(&p).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; MAX_SIZE as usize];
        file.read_exact(&mut buf).map_err(|e| e.to_string())?;
        (String::from_utf8_lossy(&buf).to_string(), true)
    } else {
        let bytes = fs::read(&p).map_err(|e| e.to_string())?;
        (String::from_utf8_lossy(&bytes).to_string(), false)
    };

    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    Ok(ReadFileResponse {
        content,
        path: p.display().to_string(),
        name,
        size_bytes: size,
        is_truncated,
    })
}

/// Enregistre les modifications apportées à un fichier texte existant.
pub fn write_file_content(path_str: &str, content: &str) -> Result<String, String> {
    let p = normalize_user_path(PathBuf::from(path_str));
    if !p.exists() {
        return Err("Fichier cible introuvable.".into());
    }
    fs::write(&p, content).map_err(|e| format!("Échec d'enregistrement : {}", e))?;
    Ok("Fichier enregistré avec succès.".into())
}

// ============================================================================
// 8. GESTION DES IMAGES, VIGNETTES & MÉTADONNÉES EXIF
// ============================================================================

/// Métadonnées détaillées d'une image pour l'inspecteur contextuel du dashboard.
///
/// Regroupe les propriétés géométriques (résolution) ainsi que les balises
/// EXIF, IPTC et XMP extraites d'appareils photographiques professionnels ou smartphones.
#[derive(Debug, Serialize, Deserialize)]
pub struct ImageInfoResponse {
    /// Nom du fichier image.
    pub name: String,
    /// Chemin absolu du fichier sur le disque.
    pub path: String,
    /// Taille brute en octets.
    pub size_bytes: u64,
    /// Taille formatée lisible pour l'humain (ex: "4.2 Mo").
    pub size_human: String,
    /// Désignation explicite du format (ex: "Sony Alpha RAW (ARW)", "JPEG Image").
    pub format: String,
    /// Largeur de l'image en pixels.
    pub width: Option<u32>,
    /// Hauteur de l'image en pixels.
    pub height: Option<u32>,
    /// Marque du constructeur de l'appareil (ex: "Sony", "Nikon", "Apple").
    pub camera_make: Option<String>,
    /// Modèle exact de l'appareil photo (ex: "ILCE-7M4", "iPhone 15 Pro").
    pub camera_model: Option<String>,
    /// Objectif utilisé (ex: "FE 24-70mm F2.8 GM II").
    pub lens: Option<String>,
    /// Longueur focale équivalente (ex: "35.0 mm").
    pub focal_length: Option<String>,
    /// Ouverture du diaphragme (ex: "f/2.8").
    pub aperture: Option<String>,
    /// Vitesse d'obturation / temps de pose (ex: "1/500").
    pub shutter_speed: Option<String>,
    /// Sensibilité ISO (ex: "400").
    pub iso: Option<String>,
    /// Date et heure de prise de vue au format ISO ou EXIF standard.
    pub date_taken: Option<String>,
    /// Programme d'exposition (ex: "Manuel", "Priorité ouverture").
    pub exposure_mode: Option<String>,
    /// Balance des blancs (ex: "Auto", "Ensoleillé").
    pub white_balance: Option<String>,
    /// Espace colorimétrique (ex: "sRGB", "Display P3", "Adobe RGB").
    pub color_space: Option<String>,
    /// Logiciel ou firmware de traitement (ex: "Adobe Lightroom", "iOS 17.4").
    pub software: Option<String>,
}

/// Calcule une empreinte de hachage 64-bit déterministe pour tout objet implémentant `Hash`.
///
/// Utilisé principalement pour générer des clés de cache disque uniques
/// combinant le chemin absolu, le timestamp de modification `mtime`, la taille du fichier
/// et le format demandé (vignette ou plein écran).
fn calculate_hash<T: std::hash::Hash>(t: &T) -> u64 {
    use std::hash::Hasher;
    let mut s = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

/// Résout ou génère à chaud le chemin d'accès au fichier d'aperçu d'une image :
///
/// - **Formats Web natifs directs** : Les formats courants (JPEG, PNG, WebP, SVG, GIF)
///   sont retournés directement sans conversion si la pleine résolution est demandée.
/// - **Cache de conversion (`/tmp/noos_image_cache`)** : Les formats lourds ou non lisibles
///   par le navigateur (RAW, HEIC, TIFF, PSD, DDS, TGA) ou les miniatures sont convertis et
///   mis en cache en utilisant une clé de hachage invalidée dès que le fichier source change.
/// - **Stratégie en cascade à 3 niveaux** :
///   1. Pour les fichiers RAW reflex (NEF, CR2, CR3, ARW, DNG, RAF, RW2, etc.), extraction
///      quasi-instantanée de l'aperçu JPEG pleine définition pré-rendu dans le RAW via `exiftool`.
///   2. Si non RAW ou échec, conversion vectorielle / bitmap via `magick` ou `convert` (ImageMagick)
///      avec orientation automatique EXIF (`-auto-orient`) et réduction proportionnelle.
///   3. En cas d'échec sur les fichiers HEIC / AVIF Apple, délégation de secours à `ffmpeg`.
pub fn get_image_preview_path(path_str: &str, is_thumb: bool) -> Result<(PathBuf, String), String> {
    let p = normalize_user_path(PathBuf::from(path_str));
    if !p.exists() || !p.is_file() {
        return Err("Fichier image introuvable.".into());
    }

    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    // Formats web natifs directs si pleine résolution demandée
    let is_native = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif" | "svg");
    if is_native && !is_thumb {
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            _ => "application/octet-stream",
        };
        return Ok((p, mime.to_string()));
    }

    // Gestion du cache pour conversions lourdes (RAW, HEIC, TIFF, thumbnails)
    let meta = fs::metadata(&p).map_err(|e| e.to_string())?;
    let mtime = meta.modified().ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let size = meta.len();

    let cache_dir = PathBuf::from("/tmp/noos_image_cache");
    let _ = fs::create_dir_all(&cache_dir);

    let raw_key = format!("{}_{}_{}_{}", p.display(), mtime, size, is_thumb);
    let hash = format!("{:x}", calculate_hash(&raw_key));
    let cached_path = cache_dir.join(format!("{}.jpg", hash));

    if cached_path.exists() && cached_path.metadata().map(|m| m.len() > 100).unwrap_or(false) {
        return Ok((cached_path, "image/jpeg".to_string()));
    }

    let is_raw = matches!(
        ext.as_str(),
        "nef" | "nrw" | "cr2" | "cr3" | "crw" | "arw" | "srf" | "sr2" | "dng" | "raf" | "rw2" | "orf" | "pef" | "3fr" | "raw"
    );

    let mut generated = false;

    // Étape 1 : pour les fichiers RAW, extraction instantanée de l'aperçu JPEG intégré
    if is_raw {
        if let Ok(out) = std::process::Command::new("exiftool")
            .args(["-b", "-PreviewImage"])
            .arg(&p)
            .output()
        {
            if out.status.success() && out.stdout.len() > 2048 && out.stdout.starts_with(&[0xFF, 0xD8, 0xFF]) {
                if fs::write(&cached_path, &out.stdout).is_ok() {
                    generated = true;
                }
            }
        }

        if !generated {
            if let Ok(out) = std::process::Command::new("exiftool")
                .args(["-b", "-JpgFromRaw"])
                .arg(&p)
                .output()
            {
                if out.status.success() && out.stdout.len() > 2048 && out.stdout.starts_with(&[0xFF, 0xD8, 0xFF]) {
                    if fs::write(&cached_path, &out.stdout).is_ok() {
                        generated = true;
                    }
                }
            }
        }
    }

    // Étape 2 : si pas encore généré (iPhone HEIC, TIFF, PSD, ou fallback RAW complet)
    if !generated {
        let max_dim = if is_thumb { "240x240>" } else { "2560x1440>" };
        let input_arg = if is_raw || matches!(ext.as_str(), "heic" | "heif" | "hif" | "psd" | "tiff" | "tif" | "pcx" | "tga" | "targa" | "dds") {
            format!("{}[0]", p.display())
        } else {
            p.display().to_string()
        };

        let mut cmd = if std::process::Command::new("magick").arg("-version").output().is_ok() {
            std::process::Command::new("magick")
        } else {
            std::process::Command::new("convert")
        };

        let quality_str = if is_thumb { "80" } else { "88" };
        let status = cmd
            .arg(&input_arg)
            .args([
                "-auto-orient",
                "-resize",
                max_dim,
                "-quality",
                quality_str,
            ])
            .arg(&cached_path)
            .status();

        if status.map(|s| s.success()).unwrap_or(false) && cached_path.exists() {
            generated = true;
        }
    }

    // Étape 3 : Fallback ffmpeg pour HEIC / AVIF si ImageMagick délégué n'est pas encore prêt
    if !generated && matches!(ext.as_str(), "heic" | "heif" | "hif") {
        let status = std::process::Command::new("ffmpeg")
            .args(["-y", "-i"])
            .arg(&p)
            .args(["-frames:v", "1", "-q:v", "2"])
            .arg(&cached_path)
            .status();

        if status.map(|s| s.success()).unwrap_or(false) && cached_path.exists() {
            generated = true;
        }
    }

    if generated && cached_path.exists() {
        Ok((cached_path, "image/jpeg".to_string()))
    } else if is_native {
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            _ => "application/octet-stream",
        };
        Ok((p, mime.to_string()))
    } else {
        Err(format!("Impossible de convertir l'image au format '.{}' pour l'affichage.", ext))
    }
}

/// Extrait les métadonnées techniques et photographiques complètes d'un fichier image.
///
/// Utilise en premier recours l'outil système `exiftool` avec sortie JSON structurée,
/// puis s'appuie sur `identify` (ImageMagick) pour combler les dimensions géométriques de base
/// si les balises EXIF sont absentes.
pub fn get_image_info(path_str: &str) -> Result<ImageInfoResponse, String> {
    let p = normalize_user_path(PathBuf::from(path_str));
    if !p.exists() || !p.is_file() {
        return Err("Fichier image introuvable.".into());
    }

    let meta = fs::metadata(&p).map_err(|e| e.to_string())?;
    let size_bytes = meta.len();
    let size_human = format_size(size_bytes);
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    let pretty_format = match ext.as_str() {
        "nef" | "nrw" => "Nikon RAW (NEF)",
        "cr2" => "Canon RAW (CR2)",
        "cr3" => "Canon RAW (CR3)",
        "crw" => "Canon RAW (CRW)",
        "arw" | "srf" | "sr2" => "Sony Alpha RAW (ARW)",
        "dng" => "Adobe / Apple ProRAW (DNG)",
        "raf" => "Fujifilm RAW (RAF)",
        "rw2" => "Panasonic Lumix RAW (RW2)",
        "orf" => "Olympus RAW (ORF)",
        "pef" => "Pentax RAW (PEF)",
        "heic" => "Apple High Efficiency (HEIC)",
        "heif" | "hif" => "High Efficiency Image (HEIF)",
        "jpg" | "jpeg" => "JPEG Image",
        "png" => "Portable Network Graphics (PNG)",
        "webp" => "Google WebP Image",
        "gif" => "GIF Image animée/statique",
        "svg" => "Scalable Vector Graphics (SVG)",
        "bmp" => "Bitmap Image (BMP)",
        "tiff" | "tif" => "TIFF Image haute fidélité",
        "psd" => "Adobe Photoshop Document (PSD)",
        "avif" => "AV1 Image File (AVIF)",
        "pcx" => "ZSoft PCX Image (PCX)",
        "tga" | "targa" => "Truevision Targa (TGA)",
        "dds" => "DirectDraw Surface (DDS)",
        _ => "Image",
    }.to_string();

    let mut info = ImageInfoResponse {
        name,
        path: p.display().to_string(),
        size_bytes,
        size_human,
        format: pretty_format,
        width: None,
        height: None,
        camera_make: None,
        camera_model: None,
        lens: None,
        focal_length: None,
        aperture: None,
        shutter_speed: None,
        iso: None,
        date_taken: None,
        exposure_mode: None,
        white_balance: None,
        color_space: None,
        software: None,
    };

    if let Ok(out) = std::process::Command::new("exiftool")
        .args(["-json", "-q", "-q"])
        .arg(&p)
        .output()
    {
        if out.status.success() {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                if let Some(first) = val.as_array().and_then(|a| a.first()).and_then(|o| o.as_object()) {
                    let get_str = |key: &str| -> Option<String> {
                        first.get(key).and_then(|v| {
                            if let Some(s) = v.as_str() {
                                Some(s.to_string())
                            } else if let Some(n) = v.as_i64() {
                                Some(n.to_string())
                            } else if let Some(f) = v.as_f64() {
                                Some(f.to_string())
                            } else {
                                None
                            }
                        })
                    };

                    let get_u32 = |key: &str| -> Option<u32> {
                        first.get(key).and_then(|v| {
                            if let Some(n) = v.as_u64() {
                                Some(n as u32)
                            } else if let Some(s) = v.as_str() {
                                s.parse::<u32>().ok()
                            } else {
                                None
                            }
                        })
                    };

                    info.width = get_u32("ImageWidth").or_else(|| get_u32("ExifImageWidth"));
                    info.height = get_u32("ImageHeight").or_else(|| get_u32("ExifImageHeight"));
                    info.camera_make = get_str("Make");
                    info.camera_model = get_str("Model");
                    info.lens = get_str("LensModel").or_else(|| get_str("Lens")).or_else(|| get_str("LensID"));
                    info.focal_length = get_str("FocalLength");
                    info.aperture = get_str("Aperture").or_else(|| get_str("FNumber")).map(|a| if a.starts_with('f') { a } else { format!("f/{}", a) });
                    info.shutter_speed = get_str("ShutterSpeed").or_else(|| get_str("ExposureTime"));
                    info.iso = get_str("ISO");
                    info.date_taken = get_str("DateTimeOriginal").or_else(|| get_str("CreateDate"));
                    info.exposure_mode = get_str("ExposureProgram").or_else(|| get_str("ExposureMode"));
                    info.white_balance = get_str("WhiteBalance");
                    info.color_space = get_str("ColorSpace");
                    info.software = get_str("Software");
                }
            }
        }
    }

    if info.width.is_none() || info.height.is_none() {
        if let Ok(out) = std::process::Command::new("identify")
            .arg("-format")
            .arg("%w %h")
            .arg(format!("{}[0]", p.display()))
            .output()
        {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                let parts: Vec<&str> = s.split_whitespace().collect();
                if parts.len() >= 2 {
                    if info.width.is_none() {
                        info.width = parts[0].parse::<u32>().ok();
                    }
                    if info.height.is_none() {
                        info.height = parts[1].parse::<u32>().ok();
                    }
                }
            }
        }
    }

    Ok(info)
}

// ============================================================================
// 9. GESTION DES ARCHIVES & COMPRESSION (ZIP, 7Z, TAR.*)
// ============================================================================

/// Détecte le chemin absolu vers un exécutable système en testant une liste de chemins candidats.
///
/// Pratique particulièrement adaptée à NixOS où les binaires résident souvent
/// dans `/run/current-system/sw/bin/` ou `/nix/var/nix/profiles/default/bin/`.
fn find_bin(candidates: &[&str]) -> String {
    for b in candidates {
        if std::path::Path::new(b).exists() {
            return b.to_string();
        }
        if std::process::Command::new(b).arg("--version").output().is_ok() {
            return b.to_string();
        }
    }
    candidates[0].to_string()
}

/// Valeur par défaut pour l'option de décompression dans un sous-dossier dédié.
fn default_subfolder_true() -> bool {
    true
}

/// Paramètres de requête pour créer une nouvelle archive compressée.
#[derive(Debug, Deserialize)]
pub struct CompressRequest {
    /// Chemins des fichiers et répertoires sources à inclure dans l'archive.
    #[serde(alias = "items")]
    pub sources: Vec<String>,
    /// Répertoire de destination où enregistrer le fichier archive.
    #[serde(alias = "destination_dir")]
    pub dest_dir: String,
    /// Nom de base du fichier archive (ex: "sauvegarde" ou "photos.zip").
    pub archive_name: String,
    /// Format cible ("zip", "7z", "tar.gz", "tar.xz", "tar.zst", "tar").
    pub format: String,
    /// Niveau de compression ("fast", "normal", "max" / "maximum").
    #[serde(alias = "level")]
    pub compression_level: String,
    /// Mot de passe optionnel pour le chiffrement symétrique de l'archive (AES-256).
    pub password: Option<String>,
}

/// Paramètres de requête pour décompresser une archive existante.
#[derive(Debug, Deserialize)]
pub struct ExtractRequest {
    /// Chemin absolu du fichier archive à extraire.
    pub archive_path: String,
    /// Dossier cible où extraire les données.
    #[serde(alias = "destination_dir")]
    pub dest_dir: String,
    /// Si true, crée un sous-dossier portant le nom de l'archive pour éviter de polluer le dossier parent.
    #[serde(default = "default_subfolder_true")]
    pub create_subfolder: bool,
    /// Mot de passe optionnel pour déverrouiller une archive chiffrée.
    pub password: Option<String>,
}

/// Paramètres de requête pour inspecter les métadonnées d'une archive sans l'extraire.
#[derive(Debug, Deserialize)]
pub struct ArchiveInfoRequest {
    pub archive_path: String,
}

/// Métadonnées d'une archive retournées pour guider l'interface utilisateur.
#[derive(Debug, Serialize)]
pub struct ArchiveInfoResponse {
    /// Indique si le fichier est une archive valide reconnue.
    pub is_archive: bool,
    /// Format d'archivage détecté (ex: "zip", "7z", "tar.gz", "tar.zst").
    pub format: String,
    /// Indique si l'archive requiert un mot de passe pour être extraite.
    pub is_encrypted: bool,
    /// Nombre d'éléments contenus dans l'archive (si dénombrables sans extraction).
    pub file_count: Option<usize>,
}

/// Inspecte une archive de manière non-destructive :
///
/// Détecte l'extension de l'archive, interroge `7z` en mode liste détaillée (`-slt`)
/// pour repérer la présence d'un chiffrement (`Encrypted = +`) et dénombrer les entrées.
pub fn get_archive_info(archive_path_str: &str) -> Result<ArchiveInfoResponse, String> {
    let p = normalize_user_path(PathBuf::from(archive_path_str));
    if !p.is_file() {
        return Err("Fichier introuvable".into());
    }

    let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    let format = if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") {
        "tar.gz".to_string()
    } else if file_name.ends_with(".tar.xz") || file_name.ends_with(".txz") {
        "tar.xz".to_string()
    } else if file_name.ends_with(".tar.zst") || file_name.ends_with(".tzst") {
        "tar.zst".to_string()
    } else if file_name.ends_with(".tar.bz2") || file_name.ends_with(".tbz2") {
        "tar.bz2".to_string()
    } else if file_name.ends_with(".zip") {
        "zip".to_string()
    } else if file_name.ends_with(".7z") {
        "7z".to_string()
    } else if file_name.ends_with(".rar") {
        "rar".to_string()
    } else if file_name.ends_with(".tar") {
        "tar".to_string()
    } else {
        "archive".to_string()
    };

    let mut is_encrypted = false;
    let mut file_count = 0;

    let p7z_bin = find_bin(&["/run/current-system/sw/bin/7z", "7z", "7za", "/nix/var/nix/profiles/default/bin/7z"]);
    if let Ok(out) = std::process::Command::new(&p7z_bin)
        .args(["l", "-slt", "-p", p.to_str().unwrap_or_default()])
        .output()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        if combined.contains("Encrypted = +") || combined.contains("Enter password") || combined.contains("Wrong password") {
            is_encrypted = true;
        }

        for line in stdout.lines() {
            if line.starts_with("Path = ") && !line.ends_with(p.to_str().unwrap_or_default()) {
                file_count += 1;
            }
        }
    }

    Ok(ArchiveInfoResponse {
        is_archive: true,
        format,
        is_encrypted,
        file_count: if file_count > 0 { Some(file_count) } else { None },
    })
}

/// Compresse une liste de fichiers et répertoires dans le format d'archive spécifié.
///
/// - **Normalisation des chemins relatifs** : Établit un répertoire de travail commun
///   afin de préserver une structure d'arborescence propre et sans chemins absolus pollueurs.
/// - **Formats pris en charge** :
///   - `zip` : standard ou sécurisé en AES-256 via `7z` si un mot de passe est fourni.
///   - `7z` : haute compression LZMA2 avec option de masquage des noms d'en-tête (`-mhe=on`).
///   - `tar.gz`, `tar.xz`, `tar.zst`, `tar` : via l'exécutable système `tar`.
/// - **Gestion des droits** : Assigne automatiquement la propriété (`chown user:users`) à
///   l'utilisateur système cible.
pub fn compress_items(req: CompressRequest) -> Result<String, String> {
    if req.sources.is_empty() {
        return Err("Aucun fichier ou dossier sélectionné pour la compression".into());
    }

    let dest_dir = normalize_user_path(PathBuf::from(&req.dest_dir));
    if !dest_dir.is_dir() {
        return Err(format!("Le dossier de destination n'existe pas : {}", req.dest_dir));
    }

    let ext = match req.format.as_str() {
        "zip" => ".zip",
        "7z" => ".7z",
        "tar.gz" | "tgz" => ".tar.gz",
        "tar.xz" | "txz" => ".tar.xz",
        "tar.zst" | "tzst" => ".tar.zst",
        "tar" => ".tar",
        _ => ".zip",
    };

    let mut clean_name = req.archive_name.trim().to_string();
    if clean_name.is_empty() {
        clean_name = "archive".to_string();
    }
    if !clean_name.to_lowercase().ends_with(ext) {
        clean_name.push_str(ext);
    }

    let archive_path = dest_dir.join(&clean_name);

    let mut resolved_sources = Vec::new();
    let mut common_parent: Option<PathBuf> = None;

    for s in &req.sources {
        let p = normalize_user_path(PathBuf::from(s));
        if !p.exists() {
            return Err(format!("L'élément source n'existe pas : {}", s));
        }
        if common_parent.is_none() {
            common_parent = p.parent().map(|d| d.to_path_buf());
        }
        resolved_sources.push(p);
    }

    let work_dir = common_parent.unwrap_or_else(|| dest_dir.clone());

    let rel_items: Vec<String> = resolved_sources
        .iter()
        .map(|p| {
            p.strip_prefix(&work_dir)
                .map(|r| r.to_string_lossy().to_string())
                .unwrap_or_else(|_| p.to_string_lossy().to_string())
        })
        .collect();

    let p7z_bin = find_bin(&["/run/current-system/sw/bin/7z", "7z", "7za", "/nix/var/nix/profiles/default/bin/7z"]);
    let zip_bin = find_bin(&["/run/current-system/sw/bin/zip", "zip", "/nix/var/nix/profiles/default/bin/zip"]);
    let tar_bin = find_bin(&["/run/current-system/sw/bin/tar", "tar", "/nix/var/nix/profiles/default/bin/tar"]);

    let level_num = match req.compression_level.as_str() {
        "fast" => "1",
        "max" | "maximum" => "9",
        _ => "6",
    };

    let status = match req.format.as_str() {
        "zip" => {
            if let Some(ref pass) = req.password {
                if !pass.trim().is_empty() {
                    let mut cmd = std::process::Command::new(&p7z_bin);
                    cmd.current_dir(&work_dir);
                    cmd.args(["a", "-tzip", &format!("-p{}", pass.trim()), "-mem=AES256", &format!("-mx={}", level_num), archive_path.to_str().unwrap_or_default()]);
                    for item in &rel_items {
                        cmd.arg(item);
                    }
                    cmd.status()
                } else {
                    let mut cmd = std::process::Command::new(&zip_bin);
                    cmd.current_dir(&work_dir);
                    cmd.args(["-r", &format!("-{}", level_num), archive_path.to_str().unwrap_or_default()]);
                    for item in &rel_items {
                        cmd.arg(item);
                    }
                    cmd.status()
                }
            } else {
                let mut cmd = std::process::Command::new(&zip_bin);
                cmd.current_dir(&work_dir);
                cmd.args(["-r", &format!("-{}", level_num), archive_path.to_str().unwrap_or_default()]);
                for item in &rel_items {
                    cmd.arg(item);
                }
                cmd.status()
            }
        }
        "7z" => {
            let mut cmd = std::process::Command::new(&p7z_bin);
            cmd.current_dir(&work_dir);
            cmd.args(["a", "-t7z", &format!("-mx={}", level_num)]);
            if let Some(ref pass) = req.password {
                if !pass.trim().is_empty() {
                    cmd.arg(format!("-p{}", pass.trim()));
                    cmd.arg("-mhe=on");
                }
            }
            cmd.arg(archive_path.to_str().unwrap_or_default());
            for item in &rel_items {
                cmd.arg(item);
            }
            cmd.status()
        }
        "tar.gz" | "tgz" => {
            let mut cmd = std::process::Command::new(&tar_bin);
            cmd.current_dir(&work_dir);
            cmd.args(["-czf", archive_path.to_str().unwrap_or_default()]);
            for item in &rel_items {
                cmd.arg(item);
            }
            cmd.status()
        }
        "tar.xz" | "txz" => {
            let mut cmd = std::process::Command::new(&tar_bin);
            cmd.current_dir(&work_dir);
            cmd.args(["-cJf", archive_path.to_str().unwrap_or_default()]);
            for item in &rel_items {
                cmd.arg(item);
            }
            cmd.status()
        }
        "tar.zst" | "tzst" => {
            let mut cmd = std::process::Command::new(&tar_bin);
            cmd.current_dir(&work_dir);
            cmd.args(["--zstd", "-cf", archive_path.to_str().unwrap_or_default()]);
            for item in &rel_items {
                cmd.arg(item);
            }
            cmd.status()
        }
        "tar" => {
            let mut cmd = std::process::Command::new(&tar_bin);
            cmd.current_dir(&work_dir);
            cmd.args(["-cf", archive_path.to_str().unwrap_or_default()]);
            for item in &rel_items {
                cmd.arg(item);
            }
            cmd.status()
        }
        _ => return Err(format!("Format non supporté : {}", req.format)),
    };

    match status {
        Ok(s) if s.success() => {
            let user = crate::updates::target_user();
            let _ = std::process::Command::new("chown").args([&format!("{}:users", user), archive_path.to_str().unwrap_or_default()]).output();
            Ok(format!("Archive créée avec succès : {}", clean_name))
        }
        Ok(s) => Err(format!("Échec de la compression (code {})", s.code().unwrap_or(-1))),
        Err(e) => Err(format!("Erreur lors de l'exécution de la commande de compression : {}", e)),
    }
}

/// Décompresse une archive existante vers le répertoire de destination spécifié.
///
/// - **Moteur 7z universel** : Décompresse les formats `zip`, `7z`, `tar.*`, `rar` via `7z x -y`.
/// - **Protection par mot de passe** : Transmet l'option `-p<password>` et détecte précisément
///   les messages d'erreur "Wrong password" pour notifier l'interface web de façon claire.
/// - **Isolation facultative** : Crée automatiquement un sous-dossier au nom de l'archive si
///   demandé par l'utilisateur pour éviter l'éparpillement des fichiers dans le dossier parent.
/// - **Attribution des droits** : Réassigne récursivement les fichiers extraits (`chown -R user:users`)
///   afin de garantir l'accès en lecture/écriture à l'utilisateur du NAS.
pub fn extract_archive(req: ExtractRequest) -> Result<String, String> {
    let archive_path = normalize_user_path(PathBuf::from(&req.archive_path));
    if !archive_path.is_file() {
        return Err("Le fichier d'archive n'existe pas".into());
    }

    let mut dest_dir = normalize_user_path(PathBuf::from(&req.dest_dir));
    if !dest_dir.is_dir() {
        return Err(format!("Le dossier de destination n'existe pas : {}", req.dest_dir));
    }

    if req.create_subfolder {
        let file_name = archive_path.file_name().and_then(|n| n.to_str()).unwrap_or("archive");
        let subfolder_name = file_name
            .trim_end_matches(".tar.gz")
            .trim_end_matches(".tar.xz")
            .trim_end_matches(".tar.zst")
            .trim_end_matches(".tar.bz2")
            .trim_end_matches(".tgz")
            .trim_end_matches(".txz")
            .trim_end_matches(".tzst")
            .trim_end_matches(".zip")
            .trim_end_matches(".7z")
            .trim_end_matches(".rar")
            .trim_end_matches(".tar");
        
        let target_subfolder = dest_dir.join(subfolder_name);
        if !target_subfolder.exists() {
            let _ = fs::create_dir_all(&target_subfolder);
        }
        dest_dir = target_subfolder;
    }

    let p7z_bin = find_bin(&["/run/current-system/sw/bin/7z", "7z", "7za", "/nix/var/nix/profiles/default/bin/7z"]);
    let mut cmd = std::process::Command::new(&p7z_bin);
    cmd.args(["x", "-y"]);
    if let Some(ref pass) = req.password {
        if !pass.trim().is_empty() {
            cmd.arg(format!("-p{}", pass.trim()));
        }
    }
    cmd.arg(format!("-o{}", dest_dir.to_str().unwrap_or_default()));
    cmd.arg(archive_path.to_str().unwrap_or_default());

    let out = cmd.output().map_err(|e| format!("Impossible d'exécuter 7z : {}", e))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let combined = format!("{}\n{}", stdout, stderr);
        if combined.contains("Wrong password") || combined.contains("Enter password") {
            return Err("Mot de passe incorrect ou manquant pour extraire cette archive.".into());
        }
        return Err(format!("Échec de l'extraction : {}", stderr.trim()));
    }

    let user = crate::updates::target_user();
    let _ = std::process::Command::new("chown").args(["-R", &format!("{}:users", user), dest_dir.to_str().unwrap_or_default()]).output();

    Ok(format!("Archive extraite avec succès dans {}", dest_dir.display()))
}
