# Syslog 发送测试程序

这是一个用于测试 Syslog 解析器的独立测试程序，可以发送各种格式的 syslog 消息来验证解析器的功能。

## 功能特性

- 支持 RFC 3164 和 RFC 5424 两种 syslog 标准格式
- 支持 UDP 和 TCP 协议发送
- 可配置目标主机和端口
- 发送多种 facility 和 severity 级别的消息
- 包含无效格式消息的容错性测试
- 命令行参数配置

## 编译和运行

```bash
# 编译程序
cargo build --release

# 运行测试（使用默认参数）
cargo run

# 自定义参数运行
cargo run -- --host localhost --tcp-port 1514 --udp-port 514 --count 5
```

## 命令行参数

- `--host <HOST>`: 目标主机地址（默认：localhost）
- `--tcp-port <PORT>`: TCP 端口（默认：1514）
- `--udp-port <PORT>`: UDP 端口（默认：514）
- `--count <COUNT>`: 每种类型消息的发送数量（默认：5）
- `--help`: 显示帮助信息

## 测试消息类型

### RFC 3164 格式
```
<134>Jul 16 19:48:17 testhost1 myapp: This is a test info message #1
```

### RFC 5424 格式
```
<134>1 2025-07-16T11:48:17.029431+00:00 testhost1 myapp 1234 MSG001 - This is a test info message #1
```

### 测试的 Facility 和 Severity 组合

| Facility | Severity | 描述 |
|----------|----------|------|
| 16 (local0) | 6 (info) | 信息消息 |
| 0 (kernel) | 3 (err) | 错误消息 |
| 23 (local7) | 4 (warning) | 警告消息 |
| 1 (user) | 2 (crit) | 严重消息 |
| 8 (cron) | 7 (debug) | 调试消息 |

### 无效格式测试

程序还会发送以下无效格式的消息来测试解析器的容错性：

1. 纯文本消息（无 syslog 格式）
2. 空优先级消息
3. 优先级过高的消息
4. 不完整的消息
5. HTTP 请求消息（模拟意外输入）

## 使用示例

```bash
# 向本地解析器发送 3 条测试消息
cargo run -- --host localhost --tcp-port 1514 --count 3

# 向远程服务器发送测试消息
cargo run -- --host 192.168.1.100 --tcp-port 1514 --udp-port 514 --count 10
```

## 注意事项

1. UDP 端口 514 通常需要管理员权限，如果权限不足会显示错误但不影响 TCP 测试
2. 确保目标 syslog 解析器正在运行并监听指定端口
3. 程序会在每条消息之间添加延迟以避免过快发送
4. 所有发送的消息都会在控制台显示，便于调试和验证

## 输出示例

```
=== Syslog Message Sender Test ===
Target: localhost:514 (UDP), localhost:1514 (TCP)
Sending 3 messages of each type...

--- Round 1 ---
[TCP RFC3164] Sent: <134>Jul 16 19:48:17 testhost1 myapp: This is a test info message #1
[TCP RFC5424] Sent: <134>1 2025-07-16T11:48:17.029431+00:00 testhost1 myapp 1234 MSG001 - This is a test info message #1
[UDP RFC3164] Sent: <134>Jul 16 19:48:17 testhost1 myapp: This is a test info message #1
[UDP RFC5424] Sent: <134>1 2025-07-16T11:48:17.029431+00:00 testhost1 myapp 1234 MSG001 - This is a test info message #1

--- Testing Invalid Messages ---
[TCP INVALID] Sent: This is a plain text message without syslog format
[TCP INVALID] Sent: <>Invalid priority

=== Test completed! Check your syslog parser web interface ===
```