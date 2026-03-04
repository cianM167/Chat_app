use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::sync::Arc;
use axum::http::header::RETRY_AFTER;
use serde::Serialize;
use tokio::sync::mpsc;
use uuid::Uuid;
use std::net::{Ipv4Addr, SocketAddr};
use axum::response::IntoResponse;
use axum::{Router};
use axum::extract::ws::WebSocket;
use axum::routing::get;
use axum::extract::{Query, State, WebSocketUpgrade};

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct User {
    user_id: Uuid,
    user_name: String
}

struct ChatRequest {
    user_id: Uuid,
    user_name: String
}

struct Chat {
    socket: WebSocket
}

async fn start_chat(
    Query(join): Query<ChatRequest>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    
}

#[tokio::main]
async fn main() {
    let chat_registry: Arc<Vec<String>> = Arc::new(Vec::new());

    let mut file = File::open("/data/user.json").unwrap();
    let mut json = fs::read_to_string("/data/user.json").unwrap(); // spooky evil unwrap
    let user_result:Result<User, serde_json::Error> = serde_json::from_str(&json);

    let user = user_result.unwrap_or_else(|_| {
            let mut user_name = String::new();

            println!("Enter your username:");
            io::stdin().read_line(&mut user_name).unwrap();

            

            User {
                user_id: Uuid::default(),
                user_name
            }
    });

    file.write_all();

    let app = Router::new()
        .route("/start_chat", get(start_chat))
        .with_state(chat_registry)
        .with_state(user.clone());

    let addr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080);
    axum_server::Server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

}