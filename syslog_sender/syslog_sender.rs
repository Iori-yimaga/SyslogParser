use std::io::Write;
use std::net::{TcpStream, UdpSocket};
use chrono::Utc;
use clap::{Arg, Command};

#[derive(Debug, Clone)]
struct SyslogSender {
    host: String,
    udp_port: u16,
    tcp_port: u16,
}

impl SyslogSender {
    fn new(host: String, udp_port: u16, tcp_port: u16) -> Self {
        Self { host, udp_port, tcp_port }
    }

    // 发送 RFC 3164 格式的 syslog 消息 (UDP)
    fn send_rfc3164_udp(&self, facility: u8, severity: u8, hostname: &str, tag: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let priority = facility * 8 + severity;
        let timestamp = chrono::Local::now().format("%b %d %H:%M:%S");
        let syslog_msg = format!("<{}>{} {} {}: {}", priority, timestamp, hostname, tag, message);
        
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.send_to(syslog_msg.as_bytes(), format!("{}:{}", self.host, self.udp_port))?;
        println!("[UDP RFC3164] Sent: {}", syslog_msg);
        Ok(())
    }

    // 发送 RFC 5424 格式的 syslog 消息 (UDP)
    fn send_rfc5424_udp(&self, facility: u8, severity: u8, hostname: &str, app_name: &str, proc_id: &str, msg_id: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let priority = facility * 8 + severity;
        let timestamp = Utc::now().to_rfc3339();
        let syslog_msg = format!(
            "<{}>1 {} {} {} {} {} - {}",
            priority, timestamp, hostname, app_name, proc_id, msg_id, message
        );
        
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.send_to(syslog_msg.as_bytes(), format!("{}:{}", self.host, self.udp_port))?;
        println!("[UDP RFC5424] Sent: {}", syslog_msg);
        Ok(())
    }

    // 发送 RFC 3164 格式的 syslog 消息 (TCP)
    fn send_rfc3164_tcp(&self, facility: u8, severity: u8, hostname: &str, tag: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let priority = facility * 8 + severity;
        let timestamp = chrono::Local::now().format("%b %d %H:%M:%S");
        let syslog_msg = format!("<{}>{} {} {}: {}\n", priority, timestamp, hostname, tag, message);
        
        let mut stream = TcpStream::connect(format!("{}:{}", self.host, self.tcp_port))?;
        stream.write_all(syslog_msg.as_bytes())?;
        stream.flush()?;
        println!("[TCP RFC3164] Sent: {}", syslog_msg.trim());
        Ok(())
    }

    // 发送 RFC 5424 格式的 syslog 消息 (TCP)
    fn send_rfc5424_tcp(&self, facility: u8, severity: u8, hostname: &str, app_name: &str, proc_id: &str, msg_id: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let priority = facility * 8 + severity;
        let timestamp = Utc::now().to_rfc3339();
        let syslog_msg = format!(
            "<{}>1 {} {} {} {} {} - {}\n",
            priority, timestamp, hostname, app_name, proc_id, msg_id, message
        );
        
        let mut stream = TcpStream::connect(format!("{}:{}", self.host, self.tcp_port))?;
        stream.write_all(syslog_msg.as_bytes())?;
        stream.flush()?;
        println!("[TCP RFC5424] Sent: {}", syslog_msg.trim());
        Ok(())
    }

    // 发送无效格式的消息用于测试容错性
    fn send_invalid_message(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut stream = TcpStream::connect(format!("{}:{}", self.host, self.tcp_port))?;
        stream.write_all(format!("{}\n", message).as_bytes())?;
        stream.flush()?;
        println!("[TCP INVALID] Sent: {}", message);
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("Syslog Sender")
        .version("1.0")
        .about("Send test syslog messages to test the parser")
        .arg(Arg::new("host")
            .long("host")
            .value_name("HOST")
            .help("Target host")
            .default_value("localhost"))
        .arg(Arg::new("udp-port")
            .short('u')
            .long("udp-port")
            .value_name("PORT")
            .help("UDP port")
            .default_value("514"))
        .arg(Arg::new("tcp-port")
            .short('t')
            .long("tcp-port")
            .value_name("PORT")
            .help("TCP port")
            .default_value("1514"))
        .arg(Arg::new("count")
            .short('c')
            .long("count")
            .value_name("COUNT")
            .help("Number of messages to send")
            .default_value("5"))
        .get_matches();

    let host = matches.get_one::<String>("host").unwrap().clone();
    let udp_port: u16 = matches.get_one::<String>("udp-port").unwrap().parse()?;
    let tcp_port: u16 = matches.get_one::<String>("tcp-port").unwrap().parse()?;
    let count: usize = matches.get_one::<String>("count").unwrap().parse()?;

    let sender = SyslogSender::new(host, udp_port, tcp_port);

    println!("=== Syslog Message Sender Test ===");
    println!("Target: {}:{} (UDP), {}:{} (TCP)", sender.host, sender.udp_port, sender.host, sender.tcp_port);
    println!("Sending {} messages of each type...\n", count);

    // 测试不同的 facility 和 severity 组合
    let test_cases = vec![
        (16, 6, "testhost1", "myapp", "1234", "MSG001", "This is a test info message"),
        (0, 3, "testhost2", "kernel", "0", "ERR001", "This is a test error message"),
        (23, 4, "testhost3", "webapp", "5678", "WARN001", "This is a test warning message"),
        (1, 2, "testhost4", "daemon", "9999", "CRIT001", "This is a test critical message"),
        (8, 7, "testhost5", "logger", "1111", "DEBUG001", "This is a test debug message"),
    ];

    for i in 0..count {
        let (facility, severity, hostname, app_name, proc_id, msg_id, base_message) = &test_cases[i % test_cases.len()];
        let message = format!("{} #{}", base_message, i + 1);
        
        println!("--- Round {} ---", i + 1);
        
        // 发送 RFC 3164 格式消息 (TCP)
        if let Err(e) = sender.send_rfc3164_tcp(*facility, *severity, hostname, app_name, &message) {
            eprintln!("Error sending RFC3164 TCP: {}", e);
        }
        
        // 发送 RFC 5424 格式消息 (TCP)
        if let Err(e) = sender.send_rfc5424_tcp(*facility, *severity, hostname, app_name, proc_id, msg_id, &message) {
            eprintln!("Error sending RFC5424 TCP: {}", e);
        }
        
        // 尝试发送 UDP 消息 (可能会失败，因为端口被占用)
        if let Err(e) = sender.send_rfc3164_udp(*facility, *severity, hostname, app_name, &message) {
            eprintln!("Error sending RFC3164 UDP: {} (This is expected if port 514 is in use)", e);
        }
        
        if let Err(e) = sender.send_rfc5424_udp(*facility, *severity, hostname, app_name, proc_id, msg_id, &message) {
            eprintln!("Error sending RFC5424 UDP: {} (This is expected if port 514 is in use)", e);
        }
        
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // 测试无效格式消息
    println!("\n--- Testing Invalid Messages ---");
    let invalid_messages = vec![
        "This is a plain text message without syslog format",
        "<>Invalid priority",
        "<999>Priority too high",
        "<16>Incomplete message",
        "GET /?ide_webview_request_time=1752666052349 HTTP/1.1", // 模拟之前出错的消息
    ];

    for (i, msg) in invalid_messages.iter().enumerate() {
        println!("Invalid message {}: {}", i + 1, msg);
        if let Err(e) = sender.send_invalid_message(msg) {
            eprintln!("Error sending invalid message: {}", e);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    println!("\n=== Test completed! Check your syslog parser web interface ===");
    Ok(())
}