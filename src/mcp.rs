use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Read, Write};

/// JSON-RPC 2.0 请求
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    #[serde(default)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 响应
/// id 字段必须始终存在,即使为 null (JSON-RPC 2.0 规范要求响应必须包含 id)
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
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
/// 始终返回服务器实际支持的版本 "2024-11-05"
/// 即使客户端请求更高版本,也应诚实回传服务器版本让客户端降级,
/// 而不是盲目回传客户端版本 (会导致客户端误以为服务器支持新版特性)
pub fn initialize_result(_protocol_version: Option<&str>) -> Value {
    serde_json::json!({
        "protocolVersion": "2024-11-05",
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
/// 支持两种格式:
/// 1. Content-Length 帧 (MCP 规范标准): Content-Length: N\r\n\r\n{json}
///    客户端可能还附带其他头行 (如 Content-Type), 直到空行为头区结束
/// 2. 换行符分隔 (旧版/调试): 每行一条 JSON, 以 \n 结尾
pub fn read_message() -> io::Result<String> {
    let mut stdin = io::stdin();
    let mut one_byte = [0u8; 1];

    // 跳过前导空白字节 (\r, \n, 空格), 避免残留分隔符干扰首行判断
    let first_byte: u8 = loop {
        let n = stdin.read(&mut one_byte)?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "连接中断"));
        }
        if one_byte[0] != b'\r' && one_byte[0] != b'\n' && one_byte[0] != b' ' {
            break one_byte[0];
        }
    };

    // 读取第一个有效行 (已含首字节)
    let mut line_buf = vec![first_byte];
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
        // 跳过回车符, 不存入缓冲区
        if one_byte[0] != b'\r' {
            line_buf.push(one_byte[0]);
        }
    }

    let line_str = String::from_utf8_lossy(&line_buf).to_string();
    log_debug(&format!("[MCP] 收到行: {}", &line_str[..line_str.len().min(200)]));

    // 判断是否是 Content-Length 帧格式
    if let Some(content_length) = extract_content_length(&line_str) {
        log_debug(&format!("[MCP] Content-Length 帧模式: {}", content_length));

        // 消耗剩余头行直到空行分隔符 (\r\n\r\n 中的第二个空行)
        loop {
            let mut header_line: Vec<u8> = Vec::new();
            loop {
                let n = stdin.read(&mut one_byte)?;
                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "连接中断"));
                }
                if one_byte[0] == b'\n' {
                    break;
                }
                if one_byte[0] != b'\r' {
                    header_line.push(one_byte[0]);
                }
            }
            // 空行表示头区结束, 后面紧跟消息体
            if header_line.is_empty() {
                break;
            }
            // 其他头行 (如 Content-Type) 跳过, 不处理
        }

        // 读取消息体
        let mut buf = vec![0u8; content_length];
        let mut read = 0;
        while read < content_length {
            let n = stdin.read(&mut buf[read..])?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "连接中断"));
            }
            read += n;
        }
        let body = String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        log_debug(&format!("[MCP] Content-Length 体: {} 字节", body.len()));
        return Ok(body);
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
/// 使用换行符分隔格式: 每条 JSON 响应以 \n 结尾
/// QoderWork CN 客户端使用此格式,原版 QoderWork 也兼容此格式
pub fn write_message(msg: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    write!(stdout, "{}\n", msg)?;
    stdout.flush()?;
    Ok(())
}