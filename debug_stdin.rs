use std::io::{self, Read, Write};
use std::fs::File;

fn main() {
    let mut stdin = io::stdin();
    let mut log = File::create("D:\\project\\demo\\omkz\\debug.log").unwrap();
    
    writeln!(log, "=== 服务器启动 ===").unwrap();
    writeln!(log, "等待 stdin 输入...").unwrap();
    
    let mut total_bytes = 0;
    let mut header_done = false;
    let mut header_buf = Vec::new();
    let mut payload_buf = Vec::new();
    
    loop {
        let mut byte = [0u8; 1];
        match stdin.read(&mut byte) {
            Ok(0) => {
                writeln!(log, "EOF,共读取 {} 字节", total_bytes).unwrap();
                break;
            }
            Ok(_) => {
                total_bytes += 1;
                
                if !header_done {
                    header_buf.push(byte[0]);
                    
                    // 检测分隔符
                    let len = header_buf.len();
                    if len >= 4 && &header_buf[len-4..] == b"\r\n\r\n" {
                        writeln!(log, "找到 \\r\\n\\r\\n 分隔符").unwrap();
                        header_done = true;
                        let header_str = String::from_utf8_lossy(&header_buf);
                        writeln!(log, "头部内容:\n{}", header_str).unwrap();
                        
                        // 提取 Content-Length
                        for line in header_str.lines() {
                            if line.to_lowercase().starts_with("content-length:") {
                                let value = line[16..].trim();
                                if let Ok(len) = value.parse::<usize>() {
                                    writeln!(log, "Content-Length: {}", len).unwrap();
                                }
                            }
                        }
                    } else if len >= 2 && &header_buf[len-2..] == b"\n\n" {
                        writeln!(log, "找到 \\n\\n 分隔符").unwrap();
                        header_done = true;
                        let header_str = String::from_utf8_lossy(&header_buf);
                        writeln!(log, "头部内容:\n{}", header_str).unwrap();
                    }
                } else {
                    payload_buf.push(byte[0]);
                }
                
                // 每 100 字节输出一次状态
                if total_bytes % 100 == 0 {
                    writeln!(log, "已读取 {} 字节", total_bytes).unwrap();
                }
            }
            Err(e) => {
                writeln!(log, "读取错误: {}", e).unwrap();
                break;
            }
        }
    }
    
    if !payload_buf.is_empty() {
        let payload_str = String::from_utf8_lossy(&payload_buf);
        writeln!(log, "Payload 内容:\n{}", payload_str).unwrap();
    }
    
    writeln!(log, "=== 服务器结束 ===").unwrap();
}
