use std::io::{self, Read, Write};

fn main() {
    let mut stdin = io::stdin();
    let mut log_file = std::fs::File::create("D:\\project\\demo\\omkz\\stdin_log.txt").unwrap();
    
    eprintln!("[DEBUG] 服务器启动,等待输入...");
    
    let mut total_bytes = 0;
    let mut buffer = Vec::with_capacity(4096);
    
    loop {
        let mut byte = [0u8; 1];
        match stdin.read(&mut byte) {
            Ok(0) => {
                eprintln!("[DEBUG] EOF,共读取 {} 字节", total_bytes);
                break;
            }
            Ok(_) => {
                total_bytes += 1;
                buffer.push(byte[0]);
                
                // 每 100 字节输出一次状态
                if total_bytes % 100 == 0 {
                    eprintln!("[DEBUG] 已读取 {} 字节", total_bytes);
                }
                
                // 检测分隔符
                let len = buffer.len();
                if len >= 4 && &buffer[len-4..] == b"\r\n\r\n" {
                    eprintln!("[DEBUG] 找到 \\r\\n\\r\\n 分隔符,共 {} 字节", total_bytes);
                    let _ = log_file.write_all(&buffer);
                    let _ = log_file.write_all(b"\n---END---\n");
                    break;
                }
                if len >= 2 && &buffer[len-2..] == b"\n\n" {
                    eprintln!("[DEBUG] 找到 \\n\\n 分隔符,共 {} 字节", total_bytes);
                    let _ = log_file.write_all(&buffer);
                    let _ = log_file.write_all(b"\n---END---\n");
                    break;
                }
            }
            Err(e) => {
                eprintln!("[DEBUG] 读取错误: {}", e);
                break;
            }
        }
    }
    
    // 输出读取到的内容
    let content = String::from_utf8_lossy(&buffer);
    eprintln!("[DEBUG] 收到内容:\n{}", content);
    
    // 尝试提取 Content-Length
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("content-length:") {
            let value = trimmed[16..].trim();
            if let Ok(len) = value.parse::<usize>() {
                eprintln!("[DEBUG] Content-Length: {}", len);
                
                // 读取 JSON payload
                let mut json_buf = vec![0u8; len];
                let mut read = 0;
                while read < len {
                    match stdin.read(&mut json_buf[read..]) {
                        Ok(0) => break,
                        Ok(n) => read += n,
                        Err(e) => {
                            eprintln!("[DEBUG] 读取 JSON 错误: {}", e);
                            break;
                        }
                    }
                }
                
                let json_str = String::from_utf8_lossy(&json_buf);
                eprintln!("[DEBUG] JSON payload:\n{}", json_str);
                let _ = log_file.write_all(&json_buf);
            }
        }
    }
}
