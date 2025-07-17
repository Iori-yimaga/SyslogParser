use anyhow::Result;
use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{Html, Json, Response},
    routing::get,
    Router,
};
use axum::extract::ws::{Message, WebSocket};

use chrono::{DateTime, Utc};
use clap::Parser;
use dashmap::DashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePool, Row};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};
use tokio::{
    net::{TcpListener, UdpSocket},
    sync::broadcast,
    time::timeout,
};
use futures::{sink::SinkExt, stream::StreamExt};
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    services::ServeDir,
};
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// UDP port for syslog reception
    #[arg(short = 'u', long, default_value_t = 514)]
    udp_port: u16,

    /// TCP port for syslog reception
    #[arg(short = 't', long, default_value_t = 1514)]
    tcp_port: u16,

    /// Web server port
    #[arg(short = 'w', long, default_value_t = 8080)]
    web_port: u16,

    /// Maximum number of logs to keep in memory
    #[arg(short = 'm', long, default_value_t = 10000)]
    max_logs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SyslogMessage {
    id: String,
    timestamp: DateTime<Utc>,
    facility: u8,
    severity: u8,
    hostname: Option<String>,
    app_name: Option<String>,
    proc_id: Option<String>,
    msg_id: Option<String>,
    message: String,
    raw_message: String,
    source_ip: String,
}

#[derive(Debug, Clone, Serialize)]
struct Stats {
    total_messages: u64,
    messages_per_facility: HashMap<u8, u64>,
    messages_per_severity: HashMap<u8, u64>,
    recent_sources: Vec<String>,
}

#[derive(Debug, Clone)]
struct AppState {
    db: SqlitePool,
    stats: Arc<DashMap<String, u64>>,
    message_counter: Arc<AtomicU64>,
    tx: broadcast::Sender<SyslogMessage>,
    max_logs: usize,
}

struct SyslogParser {
    rfc3164_regex: Regex,
    rfc5424_regex: Regex,
}

async fn init_database() -> Result<SqlitePool> {
    let database_url = "sqlite:syslog.db";
    info!("Connecting to database: {}", database_url);
    
    // Create connection options to ensure the database file is created
    use sqlx::sqlite::SqliteConnectOptions;
    use std::str::FromStr;
    
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true);
    
    let pool = SqlitePool::connect_with(options).await?;
    
    // Create the syslog_messages table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS syslog_messages (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            facility INTEGER NOT NULL,
            severity INTEGER NOT NULL,
            hostname TEXT,
            app_name TEXT,
            proc_id TEXT,
            msg_id TEXT,
            message TEXT NOT NULL,
            raw_message TEXT NOT NULL,
            source_ip TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#
    )
    .execute(&pool)
    .await?;
    
    // Create indexes for better query performance
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_timestamp ON syslog_messages(timestamp)")
        .execute(&pool)
        .await?;
    
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_facility ON syslog_messages(facility)")
        .execute(&pool)
        .await?;
    
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_severity ON syslog_messages(severity)")
        .execute(&pool)
        .await?;
    
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_source_ip ON syslog_messages(source_ip)")
        .execute(&pool)
        .await?;
    
    info!("Database initialized successfully");
    Ok(pool)
}

impl SyslogParser {
    fn new() -> Result<Self> {
        let rfc3164_regex = Regex::new(
            r"^<(\d{1,3})>(\w{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2})\s+(\S+)\s+(\S+)(?:\[(\d+)\])?:\s*(.*)$"
        )?;
        
        let rfc5424_regex = Regex::new(
            r"^<(\d{1,3})>(\d)\s+(\S+)\s+(\S+)\s+(\S+)\s+(\S+)\s+(\S+)\s+(.*)$"
        )?;
        
        Ok(Self {
            rfc3164_regex,
            rfc5424_regex,
        })
    }

    fn parse(&self, raw_message: &str, source_ip: &str) -> Result<SyslogMessage> {
        let id = Uuid::new_v4().to_string();
        let timestamp = Utc::now();
        
        // Try RFC 5424 format first
        if let Some(captures) = self.rfc5424_regex.captures(raw_message) {
            let priority: u8 = captures[1].parse()?;
            let facility = priority >> 3;
            let severity = priority & 0x07;
            
            return Ok(SyslogMessage {
                id,
                timestamp,
                facility,
                severity,
                hostname: Some(captures[4].to_string()),
                app_name: Some(captures[5].to_string()),
                proc_id: Some(captures[6].to_string()),
                msg_id: Some(captures[7].to_string()),
                message: captures[8].to_string(),
                raw_message: raw_message.to_string(),
                source_ip: source_ip.to_string(),
            });
        }
        
        // Try RFC 3164 format
        if let Some(captures) = self.rfc3164_regex.captures(raw_message) {
            let priority: u8 = captures[1].parse()?;
            let facility = priority >> 3;
            let severity = priority & 0x07;
            
            return Ok(SyslogMessage {
                id,
                timestamp,
                facility,
                severity,
                hostname: Some(captures[3].to_string()),
                app_name: Some(captures[4].to_string()),
                proc_id: captures.get(5).map(|m| m.as_str().to_string()),
                msg_id: None,
                message: captures[6].to_string(),
                raw_message: raw_message.to_string(),
                source_ip: source_ip.to_string(),
            });
        }
        
        // Fallback: basic parsing
        if let Some(priority_end) = raw_message.find('>') {
            if priority_end > 0 {
                let priority_str = &raw_message[1..priority_end];
                let priority: u8 = priority_str.parse().unwrap_or(16);
                let facility = priority >> 3;
                let severity = priority & 0x07;
                
                let message = if priority_end + 1 < raw_message.len() {
                    raw_message[priority_end + 1..].to_string()
                } else {
                    String::new()
                };
                
                return Ok(SyslogMessage {
                    id,
                    timestamp,
                    facility,
                    severity,
                    hostname: None,
                    app_name: None,
                    proc_id: None,
                    msg_id: None,
                    message,
                    raw_message: raw_message.to_string(),
                    source_ip: source_ip.to_string(),
                });
            }
        }
        
        // If no valid syslog format found, treat as plain message
        Ok(SyslogMessage {
            id,
            timestamp,
            facility: 16, // local0
            severity: 6,  // info
            hostname: None,
            app_name: None,
            proc_id: None,
            msg_id: None,
            message: raw_message.to_string(),
            raw_message: raw_message.to_string(),
            source_ip: source_ip.to_string(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    info!("Starting Syslog Parser with args: {:?}", args);
    
    let (tx, _rx) = broadcast::channel(1000);
    
    let db = init_database().await?;
    
    let state = AppState {
        db,
        stats: Arc::new(DashMap::new()),
        message_counter: Arc::new(AtomicU64::new(0)),
        tx: tx.clone(),
        max_logs: args.max_logs,
    };
    
    let parser = Arc::new(SyslogParser::new()?);
    
    // Start UDP server
    let udp_state = state.clone();
    let udp_parser = parser.clone();
    tokio::spawn(async move {
        if let Err(e) = start_udp_server(args.udp_port, udp_state, udp_parser).await {
            error!("UDP server error: {}", e);
        }
    });
    
    // Start TCP server
    let tcp_state = state.clone();
    let tcp_parser = parser.clone();
    tokio::spawn(async move {
        if let Err(e) = start_tcp_server(args.tcp_port, tcp_state, tcp_parser).await {
            error!("TCP server error: {}", e);
        }
    });
    
    // Start web server
    start_web_server(args.web_port, state).await?;
    
    Ok(())
}

async fn start_udp_server(
    port: u16,
    state: AppState,
    parser: Arc<SyslogParser>,
) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let socket = UdpSocket::bind(&addr).await?;
    info!("UDP syslog server listening on {}", addr);
    
    let mut buf = [0; 65536];
    
    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, addr)) => {
                let data = &buf[..len];
                if let Ok(message) = String::from_utf8(data.to_vec()) {
                    let message = message.trim();
                    if !message.is_empty() {
                        match parser.parse(message, &addr.ip().to_string()) {
                            Ok(syslog_msg) => {
                                process_message(syslog_msg, &state).await;
                            }
                            Err(e) => {
                                warn!("Failed to parse UDP message from {}: {}", addr, e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("UDP receive error: {}", e);
            }
        }
    }
}

async fn start_tcp_server(
    port: u16,
    state: AppState,
    parser: Arc<SyslogParser>,
) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    info!("TCP syslog server listening on {}", addr);
    
    loop {
        match listener.accept().await {
            Ok((mut stream, addr)) => {
                let state = state.clone();
                let parser = parser.clone();
                
                tokio::spawn(async move {
                    use tokio::io::{AsyncBufReadExt, BufReader};
                    
                    let reader = BufReader::new(&mut stream);
                    let mut lines = reader.lines();
                    
                    while let Ok(line_result) = timeout(Duration::from_secs(30), lines.next_line()).await {
                        if let Ok(Some(line)) = line_result {
                            let line = line.trim();
                            if !line.is_empty() {
                                match parser.parse(line, &addr.ip().to_string()) {
                                    Ok(syslog_msg) => {
                                        process_message(syslog_msg, &state).await;
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse TCP message from {}: {}", addr, e);
                                    }
                                }
                            }
                        }
                    }
                });
            }
            Err(e) => {
                error!("TCP accept error: {}", e);
            }
        }
    }
}

async fn process_message(message: SyslogMessage, state: &AppState) {
    use std::sync::atomic::Ordering;
    
    // Update statistics
    state.message_counter.fetch_add(1, Ordering::Relaxed);
    
    let facility_key = format!("facility_{}", message.facility);
    state.stats.entry(facility_key).and_modify(|e| *e += 1).or_insert(1);
    
    let severity_key = format!("severity_{}", message.severity);
    state.stats.entry(severity_key).and_modify(|e| *e += 1).or_insert(1);
    
    let source_key = format!("source_{}", message.source_ip);
    state.stats.entry(source_key).and_modify(|e| *e += 1).or_insert(1);
    
    // Store message in database
    let result = sqlx::query(
        r#"
        INSERT INTO syslog_messages 
        (id, timestamp, facility, severity, hostname, app_name, proc_id, msg_id, message, raw_message, source_ip)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#
    )
    .bind(&message.id)
    .bind(&message.timestamp.to_rfc3339())
    .bind(message.facility as i32)
    .bind(message.severity as i32)
    .bind(&message.hostname)
    .bind(&message.app_name)
    .bind(&message.proc_id)
    .bind(&message.msg_id)
    .bind(&message.message)
    .bind(&message.raw_message)
    .bind(&message.source_ip)
    .execute(&state.db)
    .await;
    
    if let Err(e) = result {
        error!("Failed to insert message into database: {}", e);
    }
    
    // Cleanup old messages if we exceed max_logs
    let count_result = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM syslog_messages")
        .fetch_one(&state.db)
        .await;
    
    if let Ok(count) = count_result {
        if count > state.max_logs as i64 {
            let delete_count = count - (state.max_logs as i64 / 2);
            let _ = sqlx::query(
                "DELETE FROM syslog_messages WHERE id IN (SELECT id FROM syslog_messages ORDER BY created_at ASC LIMIT ?)"
            )
            .bind(delete_count)
            .execute(&state.db)
            .await;
        }
    }
    
    // Broadcast to websocket clients
    let _ = state.tx.send(message);
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

async fn handle_websocket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();
    
    // Send initial connection confirmation
    if sender.send(Message::Text("{\"type\":\"connected\"}".to_string())).await.is_err() {
        return;
    }
    
    // Handle incoming messages from client (if any)
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            if let Ok(msg) = msg {
                match msg {
                    Message::Close(_) => break,
                    _ => {} // Ignore other message types for now
                }
            } else {
                break;
            }
        }
    });
    
    // Handle outgoing messages to client
    let send_task = tokio::spawn(async move {
        while let Ok(message) = rx.recv().await {
            let json_msg = match serde_json::to_string(&message) {
                Ok(json) => json,
                Err(_) => continue,
            };
            
            if sender.send(Message::Text(json_msg)).await.is_err() {
                break;
            }
        }
    });
    
    // Wait for either task to complete
    tokio::select! {
        _ = recv_task => {},
        _ = send_task => {},
    }
}

async fn start_web_server(port: u16, state: AppState) -> Result<()> {
    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/logs", get(get_logs).delete(clear_logs))
        .route("/api/stats", get(get_stats))
        .route("/api/logs/:id", get(get_log_by_id))
        .route("/api/ws", get(websocket_handler))
        .nest_service("/static", ServeDir::new("static"))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
        )
        .with_state(state);
    
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Web server listening on http://{}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}

#[derive(Deserialize)]
struct LogQuery {
    limit: Option<usize>,
    offset: Option<usize>,
    facility: Option<u8>,
    severity: Option<u8>,
    search: Option<String>,
}

async fn get_logs(
    Query(params): Query<LogQuery>,
    State(state): State<AppState>,
) -> Json<Vec<SyslogMessage>> {
    let mut query_str = "SELECT id, timestamp, facility, severity, hostname, app_name, proc_id, msg_id, message, raw_message, source_ip FROM syslog_messages WHERE 1=1".to_string();
    let mut conditions = Vec::new();
    
    // Build WHERE conditions
    if let Some(facility) = params.facility {
        query_str.push_str(" AND facility = ?");
        conditions.push(facility.to_string());
    }
    
    if let Some(severity) = params.severity {
        query_str.push_str(" AND severity = ?");
        conditions.push(severity.to_string());
    }
    
    if let Some(search) = &params.search {
        query_str.push_str(" AND (message LIKE ? OR hostname LIKE ? OR app_name LIKE ?)");
        let search_pattern = format!("%{}%", search);
        conditions.push(search_pattern.clone());
        conditions.push(search_pattern.clone());
        conditions.push(search_pattern);
    }
    
    // Add ordering and pagination
    query_str.push_str(" ORDER BY timestamp DESC");
    
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(100).min(1000);
    
    query_str.push_str(" LIMIT ? OFFSET ?");
    conditions.push(limit.to_string());
    conditions.push(offset.to_string());
    
    let mut query = sqlx::query(&query_str);
    for condition in conditions {
        query = query.bind(condition);
    }
    
    let rows = query.fetch_all(&state.db).await.unwrap_or_default();
    
    let logs: Vec<SyslogMessage> = rows.into_iter().map(|row| {
        let timestamp_str: String = row.get("timestamp");
        let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
            .unwrap_or_else(|_| Utc::now().into())
            .with_timezone(&Utc);
        
        SyslogMessage {
            id: row.get("id"),
            timestamp,
            facility: row.get::<i32, _>("facility") as u8,
            severity: row.get::<i32, _>("severity") as u8,
            hostname: row.get("hostname"),
            app_name: row.get("app_name"),
            proc_id: row.get("proc_id"),
            msg_id: row.get("msg_id"),
            message: row.get("message"),
            raw_message: row.get("raw_message"),
            source_ip: row.get("source_ip"),
        }
    }).collect();
    
    Json(logs)
}

async fn get_log_by_id(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<SyslogMessage>, StatusCode> {
    let row = sqlx::query(
        "SELECT id, timestamp, facility, severity, hostname, app_name, proc_id, msg_id, message, raw_message, source_ip FROM syslog_messages WHERE id = ?"
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    match row {
        Some(row) => {
            let timestamp_str: String = row.get("timestamp");
            let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                .unwrap_or_else(|_| Utc::now().into())
                .with_timezone(&Utc);
            
            let log = SyslogMessage {
                id: row.get("id"),
                timestamp,
                facility: row.get::<i32, _>("facility") as u8,
                severity: row.get::<i32, _>("severity") as u8,
                hostname: row.get("hostname"),
                app_name: row.get("app_name"),
                proc_id: row.get("proc_id"),
                msg_id: row.get("msg_id"),
                message: row.get("message"),
                raw_message: row.get("raw_message"),
                source_ip: row.get("source_ip"),
            };
            Ok(Json(log))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn clear_logs(State(state): State<AppState>) -> StatusCode {
    use std::sync::atomic::Ordering;
    
    // Clear all logs from database
    let result = sqlx::query("DELETE FROM syslog_messages")
        .execute(&state.db)
        .await;
    
    if result.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    
    // Reset message counter
    state.message_counter.store(0, Ordering::Relaxed);
    
    // Clear statistics
    state.stats.clear();
    
    info!("All logs cleared");
    StatusCode::NO_CONTENT
}

async fn get_stats(State(state): State<AppState>) -> Json<Stats> {
    use std::sync::atomic::Ordering;
    
    let total_messages = state.message_counter.load(Ordering::Relaxed);
    
    let mut messages_per_facility = HashMap::new();
    let mut messages_per_severity = HashMap::new();
    let mut recent_sources = Vec::new();
    
    for entry in state.stats.iter() {
        let key = entry.key();
        let value = *entry.value();
        
        if let Some(facility_str) = key.strip_prefix("facility_") {
            if let Ok(facility) = facility_str.parse::<u8>() {
                messages_per_facility.insert(facility, value);
            }
        } else if let Some(severity_str) = key.strip_prefix("severity_") {
            if let Ok(severity) = severity_str.parse::<u8>() {
                messages_per_severity.insert(severity, value);
            }
        } else if let Some(source) = key.strip_prefix("source_") {
            recent_sources.push(source.to_string());
        }
    }
    
    // Limit recent sources to top 10
    recent_sources.sort();
    recent_sources.truncate(10);
    
    Json(Stats {
        total_messages,
        messages_per_facility,
        messages_per_severity,
        recent_sources,
    })
}

async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}
