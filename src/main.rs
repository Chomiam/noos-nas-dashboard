mod api;
mod files;
mod firewall;
mod hardware;
mod services;
mod smart;
mod speedtest;
mod storage;
mod system;
mod terminal;
mod updates;

use axum::Router;
use std::env;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

const DEFAULT_PORT: u16 = 9339;

#[tokio::main]
async fn main() {
    let port = parse_port();

    let frontend_dir = env::var("STEVEOS_FRONTEND_DIR").unwrap_or_else(|_| "frontend".to_string());

    let app = Router::new()
        .nest("/api", api::api_routes())
        .fallback_service(ServeDir::new(&frontend_dir))
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
