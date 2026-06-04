"""快速验证 MCP 协议兼容性"""
import subprocess
import json

BINARY = r"D:\project\demo\omkz\target\release\computer-use-win.exe"

def build_message(method, params=None, req_id=1):
    payload = {"jsonrpc": "2.0", "id": req_id, "method": method}
    if params is not None:
        payload["params"] = params
    body = json.dumps(payload)
    return f"Content-Length: {len(body.encode('utf-8'))}\r\n\r\n{body}".encode("utf-8")

# 发送 initialize 消息
init_msg = build_message("initialize", {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": {"name": "qoderwork-test", "version": "0.5.8"}
})

proc = subprocess.Popen(
    [BINARY],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)

stdout, stderr = proc.communicate(input=init_msg, timeout=5)

print("=== Response ===")
print(stdout.decode("utf-8", errors="replace"))
print("\n=== Stderr ===")
print(stderr.decode("utf-8", errors="replace")[:500])
