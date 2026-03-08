use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use axum::http::header::RETRY_AFTER;
use futures_util::SinkExt;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader, stdin};
use tokio::sync::{mpsc};
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
    chat_registry: Arc<Mutex<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
}

#[derive(Clone)]
struct Context {
    chat_registry: Arc<Mutex<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    user: User
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct Handshake {
    user_id: Uuid,
    username: String,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type", content = "data")]
enum ClientMessage {
    Handshake(Handshake),
    ChatMessage {
        to: Uuid,
        text: String
    }
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
    

    let remote_user =  if let Some(Ok(Message::Text(text))) = receiver.next().await {
        if let Ok(ClientMessage::Handshake(hs)) = serde_json::from_str(&text) {
            let hs_clone = hs.clone();
            tokio::task::spawn_blocking(move || {
                let conn = Connection::open("chat_history.db").unwrap();
                conn.execute(
                    "INSERT INTO contacts (user_id, username) 
                     VALUES (?1, ?2) 
                     ON CONFLICT(user_id) DO UPDATE SET username=excluded.username",
                    params![hs_clone.user_id.to_string(), hs_clone.username],
                ).expect("Failed to sync contact");
            }).await.unwrap();
            
            hs
        } else {
             return;// Closing connection handshake not sent
        }
    } else {
        return;
    };

    //sending second part of handshake

    let my_info = ClientMessage::Handshake(Handshake { 
        user_id: chat_ctx.user.user_id, 
        username: chat_ctx.user.user_name, 
    });

    let handshake_json = serde_json::to_string(&my_info).unwrap();

    if sender.send(Message::Text(handshake_json.into())).await.is_err() {
        return; // Dropping connections
    }

    // Registring correspondent in registry

    let (tx, mut rx) = mpsc::unbounded_channel();
    {
        let mut registry = chat_ctx.chat_registry.lock().unwrap();
        registry.insert(remote_user.user_id, tx);
    }

    let mut send_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if sender.send(message).await.is_err() { break; }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(message) => {
                    println!("{}: {}", remote_user.username, message);
                }

                _ => {}
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => (),
        _ = (&mut recv_task) => (),
    };

    // 4. Cleanup: Remove from HashMap on disconnect
    let mut registry = chat_ctx.chat_registry.lock().unwrap();
    registry.remove(&chat_ctx.user.user_id);
    
}

#[tokio::main]
async fn main() {
    let json = fs::read_to_string("data/user.json").unwrap(); // spooky evil unwrap
    let user_result:Result<User, serde_json::Error> = serde_json::from_str(&json);

    init_db();

    let user = user_result.unwrap_or_else(|_| {
            let mut user_name = String::new();

            println!("Enter your username:");
            io::stdin().read_line(&mut user_name).unwrap();

            

            let new_user = User {
                user_id: Uuid::new_v4(),
                user_name
            };

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open("data/user.json")
                .expect("User data file cant be opened");

            let json_string = serde_json::to_string_pretty(&new_user).unwrap();
            file.write_all(json_string.as_bytes()).expect("Write failed");

            new_user
    });

    let chat_registry: Arc<Mutex<HashMap<Uuid, mpsc::UnboundedSender<Message>>>> = Arc::new(Mutex::new(HashMap::new()));

    tokio::spawn(terminal(chat_registry.clone()));
    
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

fn init_db() -> Connection {
    let conn = Connection::open("chat_history.db").expect("Failed to open connection");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS contacts (
            user_id TEXT PRIMARY KEY,
            username TEXT NOT NULL
        )",
        [],
    ).expect("Failed to create contact table");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            sender_id TEXT NOT NULL,
            receiver_id TEXT NOT NULL,
            content TEXT NOT NULL,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
            -- Link these to the contacts table
            FOREIGN KEY (sender_id) REFERENCES contacts (user_id),
            FOREIGN KEY (receiver_id) REFERENCES contacts (user_id)
        )",
        [],
    ).expect("Failed to create message table");

    conn
}

async fn terminal(
    chat_registry: Arc<Mutex<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>
) {
    let stidn = tokio::io::stdin();

    let mut reader = BufReader::new(stidn);
    let mut input = String::new();

    println!("Welcome to my chat app please enter a command (type help for help):");

    loop {
        input.clear();

        let bytes_read = reader.read_line(&mut input).await.unwrap();

        if bytes_read == 0 {
            break;
        }

        let trimmed = input.trim();
        println!("you typed: {}", trimmed);
    }
}

fn chat_request() {

}