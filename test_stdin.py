"""通过管道测试 Rust 服务器的 stdin 读取"""
import subprocess
import json
import sys

BINARY = r"D:\project\demo\omkz\target\release\computer-use-win.exe"

# 构建一条 MCP 消息
def build_message(method, params=None, req_id=1):
    payload = {"jsonrpc": "2.0", "id": req_id, "method": method}
    if params is not None:
        payload["params"] = params
    body = json.dumps(payload)
    return f"Content-Length: {len(body.encode('utf-8'))}\r\n\r\n{body}"

# 逐条发送消息,模拟 QoderWork CN 的行为
messages = [
    build_message("initialize", {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "qoderwork", "version": "0.5.8"}
    }),
]

print("启动服务器...")
proc = subprocess.Popen(
    [BINARY],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)

for i, msg in enumerate(messages):
    print(f"\n发送消息 {i+1}:")
    print(f"  长度: {len(msg.encode('utf-8'))} 字节")
    print(f"  内容: {msg[:200]}...")
    
    # 逐字节发送
    data = msg.encode('utf-8')
    for byte in data:
        proc.stdin.write(bytes([byte]))
        proc.stdin.flush()
    
    print(f"  已发送")

# 等待响应
print("\n等待响应...")
import time
time.sleep(2)

# 读取 stderr
proc.stdin.close()
stdout, stderr = proc.communicate(timeout=5)

print(f"\n=== Stderr ===")
print(stderr.decode('utf-8', errors='replace')[:1000])

print(f"\n=== Stdout ===")
raw = stdout.decode('utf-8', errors='replace')
if raw:
    # 解析响应
    offset = 0
    while offset < len(raw):
        header_end = raw.find("\r\n\r\n", offset)
        if header_end < 0:
            break
        header = raw[offset:header_end]
        try:
            length = int(header.replace("Content-Length: ", "").strip())
            body_start = header_end + 4
            body = raw[body_start:body_start + length]
            resp = json.loads(body)
            print(f"  响应 id={resp.get('id')}: {resp.get('result', resp.get('error'))}")
            offset = body_start + length
        except:
            break
