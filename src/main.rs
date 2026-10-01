async fn no_cache_layer(req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let p = req.uri().path();
    let is_media = p.starts_with("/api/files/stream") || p.starts_with("/api/files/download") || p.starts_with("/api/files/image-view");
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

mod api;
mod auth;
mod dns;
mod files;
mod firewall;
mod hardware;
mod network;
mod services;
mod smart;
mod speedtest;
mod storage;
mod system;
mod terminal;
mod trash;
mod updates;
mod wireguard;
mod documents;
mod youtube;
mod docker_store;
mod games;
mod minecraft;
mod vms;
mod generations;
mod users;
mod samba;
mod sftp;

use axum::Router;
use std::env;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

const DEFAULT_PORT: u16 = 9339;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--run-system-update") {
        let force_packages = args.iter().any(|a| a == "--force-packages");
        updates::run_detached_update_process(force_packages);
        return;
    }

    let port = parse_port();
    updates::init_update_tracker();
    storage::init_storage_tasks_tracker();

    let frontend_dir = env::var("STEVEOS_FRONTEND_DIR").unwrap_or_else(|_| "frontend".to_string());

    let api_router = Router::new()
        .nest("/auth", auth::auth_routes())
        .merge(api::api_routes())
        .layer(axum::middleware::from_fn(auth::auth_middleware));

    let app = Router::new()
        .nest("/api", api_router)
        .fallback_service(ServeDir::new(&frontend_dir))
        .layer(axum::middleware::from_fn(no_cache_layer))
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    println!("\x1b[38;2;203;166;247m\x1b[1m");
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║       🚀 STEvE_OS NAS Edition — Web Dashboard Server       ║");
    println!("║       Propulsé par Rust & Catppuccin Mocha                 ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║  📡 Port d'écoute : {:<39}║", port);
    println!("║  🌐 URL d'accès   : http://0.0.0.0:{:<23}║", port);
    println!("║  📁 Dossier Web   : {:<39}║", frontend_dir);
    println!("╚════════════════════════════════════════════════════════════╝");
    println!("\x1b[0m");

    updates::start_background_checker();
    trash::start_background_pruner();

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Impossible de démarrer le serveur sur le port {} : {}", port, e));

    axum::serve(listener, app)
        .await
        .unwrap();
}

fn parse_port() -> u16 {
    let args: Vec<String> = env::args().collect();
    for i in 0..args.len() {
        if (args[i] == "--port" || args[i] == "-p") && i + 1 < args.len() {
            if let Ok(p) = args[i + 1].parse::<u16>() {
                return p;
            }
        }
    }

    if let Ok(env_p) = env::var("STEVEOS_PORT") {
        if let Ok(p) = env_p.parse::<u16>() {
            return p;
        }
    }

    DEFAULT_PORT
}
