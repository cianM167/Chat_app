use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::sync::Arc;
use axum::http::header::RETRY_AFTER;
use serde::Serialize;
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;
use std::net::{Ipv4Addr, SocketAddr};
use axum::response::IntoResponse;
use axum::{Router};
use futures_util::stream::{SplitSink, StreamExt};
use axum::extract::ws::{Message, WebSocket};
use axum::routing::get;
use axum::extract::{Query, State, WebSocketUpgrade};

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct User {
    user_id: Uuid,
    user_name: String
}

#[derive(serde::Deserialize)]
struct ChatRequest {
    user_id: Uuid,
    user_name: String
}

struct Chat {
    user: User,
    contact: User,
    chat_registry: Arc<Mutex<HashMap<String, SplitSink<WebSocket, Message>>>>
}

#[derive(Clone)]
struct Context {
    chat_registry: Arc<Mutex<HashMap<String, SplitSink<WebSocket, Message>>>>,
    user: User
}

#[axum::debug_handler]
async fn start_chat(
    State(context): State<Context>,
    Query(join): Query<ChatRequest>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {

    let contact = User {
        user_id: join.user_id,
        user_name: join.user_name
    };
    
    let chat_ctx = Chat {
        user: context.user,
        contact,
        chat_registry: context.chat_registry 
    };

    ws.on_upgrade(move |socket| handle_socket(socket, chat_ctx))
}

async fn handle_socket(
    mut socket: WebSocket,
    chat_ctx: Chat
) {
    let (mut sender, mut receiver) = socket.split();
}

#[tokio::main]
async fn main() {
    let json = fs::read_to_string("/data/user.json").unwrap(); // spooky evil unwrap
    let user_result:Result<User, serde_json::Error> = serde_json::from_str(&json);

    let user = user_result.unwrap_or_else(|_| {
            let mut user_name = String::new();

            println!("Enter your username:");
            io::stdin().read_line(&mut user_name).unwrap();

            

            let new_user = User {
                user_id: Uuid::default(),
                user_name
            };

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open("/data/user.json")
                .expect("User data file cant be opened");

            let json_string = serde_json::to_string_pretty(&new_user).unwrap();
            file.write_all(json_string.as_bytes()).expect("Write failed");

            new_user
    });

    let chat_registry: Arc<Mutex<HashMap<String, SplitSink<WebSocket, Message>>>> = Arc::new(Mutex::new(HashMap::new()));
    
    let ctx = Context {
        chat_registry,
        user
    };

    let app = Router::new()
        .route("/start_chat", get(start_chat))
        .with_state(ctx.clone());

    let addr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080);
    axum_server::Server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

}