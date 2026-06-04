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

/// 初始化 MCP 服务器的 tools/list 响应
pub fn tools_list(tools: &[ToolDef]) -> Value {
    let tools_json: Vec<Value> = tools
        .iter()
        .map(|t| serde_json::to_value(t).unwrap())
        .collect();
    serde_json::json!({ "tools": tools_json })
}

/// 初始化响应
pub fn initialize_result() -> Value {
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

/// 从 stdin 读取一条 Content-Length 帧消息
pub fn read_message() -> io::Result<String> {
    let mut stdin = io::stdin();
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        stdin.read_line(&mut line)?;
        let line = line.trim_end_matches("\r\n").trim_end_matches("\n");

        if line.is_empty() {
            break;
        }
        if let Some(len_str) = line.strip_prefix("Content-Length: ") {
            content_length = len_str.parse().ok();
        }
    }

    let len = content_length.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "缺少 Content-Length 头")
    })?;

    let mut buf = vec![0u8; len];
    let mut read = 0;
    while read < len {
        let n = stdin.read(&mut buf[read..])?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "连接中断"));
        }
        read += n;
    }

    String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// 向 stdout 发送一条 Content-Length 帧消息
pub fn write_message(msg: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    write!(stdout, "Content-Length: {}\r\n\r\n{}", msg.len(), msg)?;
    stdout.flush()?;
    Ok(())
}
