use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Read, Write};

/// JSON-RPC 2.0 请求
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 响应
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

/// MCP 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// 工具列表响应
pub fn tools_list(tools: &[ToolDef]) -> Value {
    let tools_json: Vec<Value> = tools
        .iter()
        .map(|t| serde_json::to_value(t).unwrap())
        .collect();
    serde_json::json!({ "tools": tools_json })
}

/// 初始化响应
pub fn initialize_result(protocol_version: Option<&str>) -> Value {
    let protocol_version = protocol_version.unwrap_or("2024-11-05");
    serde_json::json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "computer-use-win",
            "version": "0.1.0"
        }
    })
}

/// 写入调试日志到文件
fn log_debug(msg: &str) {
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("D:\\project\\demo\\omkz\\server.log")
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", msg)
        });
}

/// 从 stdin 读取一条消息
/// 同时支持两种格式:
/// 1. 换行符分隔 (QoderWork CN / MCP JS SDK): 每行一条 JSON,以 \n 结尾
/// 2. Content-Length 帧 (LSP 风格): Content-Length: N\r\n\r\n{json}
pub fn read_message() -> io::Result<String> {
    let mut stdin = io::stdin();
    let mut line_buf: Vec<u8> = Vec::with_capacity(4096);
    let mut one_byte = [0u8; 1];

    // 按字节读取直到遇到换行符
    loop {
        let n = stdin.read(&mut one_byte)?;
        if n == 0 {
            if line_buf.is_empty() {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "连接中断"));
            }
            break;
        }
        if one_byte[0] == b'\n' {
            break;
        }
        // 跳过回车符,不存入缓冲区
        if one_byte[0] != b'\r' {
            line_buf.push(one_byte[0]);
        }
    }

    let line_str = String::from_utf8_lossy(&line_buf).to_string();
    log_debug(&format!("[MCP] 收到行: {}", &line_str[..line_str.len().min(200)]));

    // 判断是否是 Content-Length 帧格式
    if let Some(content_length) = extract_content_length(&line_str) {
        log_debug(&format!("[MCP] Content-Length 帧模式: {}", content_length));
        let mut buf = vec![0u8; content_length];
        let mut read = 0;
        while read < content_length {
            let n = stdin.read(&mut buf[read..])?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "连接中断"));
            }
            read += n;
        }
        return String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
    }

    // 换行符分隔模式: 直接返回这一行
    log_debug(&format!("[MCP] 换行符分隔模式,长度: {}", line_str.len()));
    Ok(line_str)
}

/// 从 HTTP 头字符串中提取 Content-Length 值
fn extract_content_length(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    if trimmed.to_lowercase().starts_with("content-length:") {
        let value = trimmed[16..].trim();
        return value.parse::<usize>().ok();
    }
    None
}

/// 向 stdout 发送一条消息
/// 使用换行符分隔格式 (兼容 QoderWork CN / MCP JS SDK)
pub fn write_message(msg: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    write!(stdout, "{}\n", msg)?;
    stdout.flush()?;
    Ok(())
}
