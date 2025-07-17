# Syslog Parser - 实时日志监控平台

一个基于 Rust 和 Web 技术构建的高性能 Syslog 日志解析和监控系统，提供实时日志收集、解析、存储和可视化功能。

## ✨ 主要特性

### 🚀 多协议支持
- **UDP Syslog 服务器** - 标准 514 端口接收日志
- **TCP Syslog 服务器** - 1514 端口支持可靠传输
- **RFC 3164 & RFC 5424** - 完整支持两种 Syslog 标准格式

### 📊 实时监控面板
- **现代化 Web 界面** - 响应式设计，支持移动端
- **实时数据更新** - WebSocket 连接，零延迟显示新日志
- **统计仪表板** - 消息总数、错误统计、活跃源等关键指标
- **智能搜索过滤** - 支持按设施、严重性、关键词快速筛选

### 💾 数据管理
- **SQLite 数据库** - 轻量级本地存储，自动索引优化
- **自动清理机制** - 可配置最大日志数量，防止磁盘溢出
- **数据导出功能** - 支持 CSV 格式导出，便于分析

### ⚡ 高性能架构
- **异步处理** - 基于 Tokio 异步运行时，高并发处理
- **内存统计** - 实时统计缓存，快速响应查询
- **连接池管理** - 数据库连接复用，提升性能

## 🛠️ 技术栈

**后端核心**
- **Rust** - 系统级性能，内存安全
- **Axum** - 现代异步 Web 框架
- **SQLx** - 类型安全的数据库操作
- **Tokio** - 异步运行时

**前端界面**
- **原生 JavaScript** - 轻量级，无框架依赖
- **WebSocket** - 实时双向通信
- **响应式 CSS** - 现代化 UI 设计

## 🚀 快速开始

### 环境要求
- Rust 1.70+
- 现代浏览器（支持 WebSocket）

### 安装运行

```bash
# 克隆项目
git clone <repository-url>
cd SyslogParser

# 编译运行
cargo run

# 或指定自定义端口
cargo run -- --udp-port 514 --tcp-port 1514 --web-port 8080
```

### 访问界面
打开浏览器访问：`http://localhost:8080`

## 📋 命令行参数

| 参数 | 短参数 | 默认值 | 说明 |
|------|--------|--------|------|
| `--udp-port` | `-u` | 514 | UDP Syslog 接收端口 |
| `--tcp-port` | `-t` | 1514 | TCP Syslog 接收端口 |
| `--web-port` | `-w` | 8080 | Web 管理界面端口 |
| `--max-logs` | `-m` | 10000 | 最大日志保存数量 |

## 🔧 使用说明

### 发送日志测试

**UDP 方式：**
```bash
echo "<34>Oct 11 22:14:15 mymachine su: 'su root' failed for lonvick on /dev/pts/8" | nc -u localhost 514
```

**TCP 方式：**
```bash
echo "<165>1 2003-10-11T22:14:15.003Z mymachine.example.com evntslog - ID47 [exampleSDID@32473 iut=\"3\" eventSource=\"Application\"] BOMAn application event log entry..." | nc localhost 1514
```

### Web 界面功能

1. **实时监控** - 自动显示新接收的日志消息
2. **搜索过滤** - 按关键词、设施类型、严重性筛选
3. **详情查看** - 点击查看完整日志信息
4. **数据导出** - 导出筛选后的日志为 CSV 文件
5. **自动刷新** - 可配置自动刷新间隔
6. **批量清理** - 一键清空所有历史日志

## 📁 项目结构

```
SyslogParser/
├── src/
│   └── main.rs          # 主程序入口
├── static/
│   ├── index.html       # Web 界面
│   ├── app.js          # 前端逻辑
│   └── style.css       # 界面样式
├── syslog_sender/      # 测试工具
├── Cargo.toml          # 项目配置
└── README.md           # 项目说明
```

## 🔍 API 接口

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/logs` | GET | 获取日志列表（支持分页和过滤）|
| `/api/logs/:id` | GET | 获取指定日志详情 |
| `/api/logs` | DELETE | 清空所有日志 |
| `/api/stats` | GET | 获取统计信息 |
| `/api/ws` | WebSocket | 实时日志推送 |

## 🤝 贡献指南

欢迎提交 Issue 和 Pull Request 来改进项目！

## 📄 许可证

本项目采用 MIT 许可证 - 查看 [LICENSE](LICENSE) 文件了解详情。

---

**开发者**: 专为系统管理员和开发者设计的轻量级日志监控解决方案 🚀