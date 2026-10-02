//! # Noos NAS Dashboard — Serveur Web Backend Principal
//!
//! Ce module constitue le point d'entrée principal (`main.rs`) du tableau de bord web
//! pour le système d'exploitation **Noos NAS Edition**.
//!
//! ## Architecture générale
//! - **Runtime asynchrone** : Conçu sur Tokio (`#[tokio::main]`), garantissant une
//!   haute concurrence non-bloquante pour les entrées/sorties et les opérations système.
//! - **Framework Web** : Utilise Axum pour le routage haute performance, l'extraction
//!   typée des requêtes HTTP et l'application modulaire de middlewares.
//! - **Sécurité & Authentification** : Contrôle d'accès basé sur PAM/shadow et sessions
//!   persistées, injecté via `auth::auth_middleware`.
//! - **Gestion des médias & anti-cache** : Couche middleware intelligente prévenant
//!   la mise en cache intempestive des données dynamiques tout en préservant le streaming.
//! - **Tâches d'arrière-plan** : Surveillance périodique des mises à jour NixOS et
//!   nettoyage automatique de la corbeille.

use axum::Router;
use std::env;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

// ============================================================================
// DÉCLARATION DES MODULES DU SYSTÈME (STRUCTURATION PAR DOMAINE)
// ============================================================================

// 1. Cœur du système & Informations matérielles
mod system;      // CPU, mémoire vive, statut d'alimentation, extinction & redémarrage
mod hardware;    // Détection d'inventaire matériel détaillé (CPU, PCIe, virtualisation)
mod services;    // Gestion des services systemd et conteneurs Docker
mod smart;       // Surveillance de santé SMART des disques durs & NVMe
mod speedtest;   // Test de débit bande passante réseau local / Internet

// 2. Sécurité, Identités & Pare-feu
mod auth;        // Authentification PAM, gestion des tokens et persistance des sessions
mod users;       // Gestion des comptes utilisateurs et groupes système Linux
mod firewall;    // Gestion des règles de filtrage réseau (iptables / nftables)
mod wireguard;   // Contrôleur de VPN WireGuard (serveur & profils clients)

// 3. Stockage, Systèmes de Fichiers & Partages
mod storage;       // Gestion des disques, pools ZFS/Btrfs, RAID, partitions et montage
mod files;         // Moteur de navigation, permissions, compression et streaming
mod trash;         // Corbeille de rétention temporaire et purge automatique
mod samba;         // Partages réseau Windows / macOS via Samba (SMB/CIFS)
mod sftp;          // Partages sécurisés SFTP avec isolation chroot
mod remote_shares; // Montage de partages distants NFS / SMB

// 4. Conteneurs, Applications & Virtualisation
mod docker_store; // Catalogue d'applications conteneurisées Docker Compose
mod vms;          // Hyperviseur KVM/QEMU, gestion des machines virtuelles et VNC
mod games;        // Gestionnaire de serveurs de jeux (moteur Pterodactyl Eggs)
mod minecraft;    // Gestionnaire dédié pour serveurs Minecraft (Vanilla/Paper/Forge)

// 5. Réseau, DNS & Maintenance du Système
mod network;     // Surveillance des interfaces réseau et statistiques de trafic
mod dns;         // Configuration des résolveurs DNS et serveurs locaux
mod generations; // Gestion des profils de démarrage NixOS et Garbage Collection
mod updates;     // Mises à jour intelligentes du système NixOS et télémétrie Git

// 6. Utilitaires & Passerelles d'Interface
mod terminal;  // Passerelle de terminal interactif sécurisé
mod documents; // Lecteur contextuel de documentation intégrée
mod youtube;   // Outil de téléchargement de médias multimédia (yt-dlp)
mod api;       // Câblage centralisé des routes de l'API REST Axum

// ============================================================================
// CONFIGURATION & CONSTANTES RÉSEAU
// ============================================================================

/// Port d'écoute par défaut du dashboard si aucune variable ni argument n'est fourni.
const DEFAULT_PORT: u16 = 9339;

// ============================================================================
// MIDDLEWARE ANTI-CACHE SÉLECTIF
// ============================================================================

/// Middleware HTTP garantissant que les réponses dynamiques de l'API ne sont pas
/// mises en cache par le navigateur client, évitant les états désynchronisés.
///
/// **Exception** : Les flux multimédia (streaming audio/vidéo, prévisualisation d'images
/// et téléchargements directs) sont exemptés afin de préserver la recherche de position
/// dans les flux (`Range Requests`) et l'optimisation des performances du navigateur.
async fn no_cache_layer(req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let p = req.uri().path();
    let is_media = p.starts_with("/api/files/stream")
        || p.starts_with("/api/files/download")
        || p.starts_with("/api/files/image-view");

    let mut resp = next.run(req).await;
    if !is_media {
        resp.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        );
        resp.headers_mut().insert(
            axum::http::header::PRAGMA,
            axum::http::HeaderValue::from_static("no-cache"),
        );
        resp.headers_mut().insert(
            axum::http::header::EXPIRES,
            axum::http::HeaderValue::from_static("0"),
        );
    }
    resp
}

// ============================================================================
// POINT D'ENTRÉE PRINCIPAL
// ============================================================================

#[tokio::main]
async fn main() {
    // 1. Analyse des arguments de ligne de commande
    let args: Vec<String> = env::args().collect();

    // Mode processus de mise à jour détaché : si invoqué avec `--run-system-update`,
    // le binaire exécute la mise à jour système NixOS de manière isolée et quitte.
    if args.iter().any(|a| a == "--run-system-update") {
        let force_packages = args.iter().any(|a| a == "--force-packages");
        updates::run_detached_update_process(force_packages);
        return;
    }

    // 2. Détermination du port d'écoute et initialisation des gestionnaires d'état
    let port = parse_port();
    updates::init_update_tracker();
    storage::init_storage_tasks_tracker();

    // 3. Résolution du chemin des fichiers statiques du frontend (HTML/CSS/JS)
    let frontend_dir = env::var("NOOS_FRONTEND_DIR")
        .unwrap_or_else(|_| "frontend".to_string());

    // 4. Construction du routeur API protégé
    // - Les routes `/api/auth` restent ouvertes pour la connexion initiale.
    // - L'ensemble des autres routes `/api/*` est soumis au middleware d'authentification.
    let api_router = Router::new()
        .nest("/auth", auth::auth_routes())
        .merge(api::api_routes())
        .layer(axum::middleware::from_fn(auth::auth_middleware));

    // 5. Assemblage de l'application globale avec distribution des fichiers statiques
    let app = Router::new()
        .nest("/api", api_router)
        .fallback_service(ServeDir::new(&frontend_dir))
        .layer(axum::middleware::from_fn(no_cache_layer))
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    // 6. Affichage de la bannière de démarrage stylisée Catppuccin Mocha
    println!("\x1b[38;2;203;166;247m\x1b[1m");
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║       🚀 Noos NAS Edition — Web Dashboard Server           ║");
    println!("║       Propulsé par Rust & Catppuccin Mocha                 ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║  📡 Port d'écoute : {:<39}║", port);
    println!("║  🌐 URL d'accès   : http://0.0.0.0:{:<23}║", port);
    println!("║  📁 Dossier Web   : {:<39}║", frontend_dir);
    println!("╚════════════════════════════════════════════════════════════╝");
    println!("\x1b[0m");

    // 7. Lancement des tâches d'arrière-plan périodiques
    updates::start_background_checker(); // Vérification des mises à jour NixOS
    trash::start_background_pruner();    // Purge automatique des fichiers expirés de la corbeille

    // 8. Liaison du socket TCP et lancement de la boucle d'événements Axum
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Impossible de démarrer le serveur sur le port {} : {}", port, e));

    axum::serve(listener, app)
        .await
        .unwrap();
}

// ============================================================================
// FONCTIONS UTILITAIRES INTERNES
// ============================================================================

/// Analyse le port réseau à utiliser selon l'ordre de priorité suivant :
/// 1. Argument CLI `--port <NUM>` ou `-p <NUM>`
/// 2. Variable d'environnement `NOOS_PORT`
/// 3. Constante de secours [`DEFAULT_PORT`] (9339)
fn parse_port() -> u16 {
    let args: Vec<String> = env::args().collect();
    for i in 0..args.len() {
        if (args[i] == "--port" || args[i] == "-p") && i + 1 < args.len() {
            if let Ok(p) = args[i + 1].parse::<u16>() {
                return p;
            }
        }
    }

    if let Ok(env_p) = env::var("NOOS_PORT") {
        if let Ok(p) = env_p.parse::<u16>() {
            return p;
        }
    }

    DEFAULT_PORT
}
