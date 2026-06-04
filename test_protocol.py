"""验证 MCP 协议兼容性"""
import subprocess
import json

BINARY = r"D:\project\demo\omkz\target\release\computer-use-win.exe"

def build_message(method, params=None, req_id=1):
    payload = {"jsonrpc": "2.0", "id": req_id, "method": method}
    if params is not None:
        payload["params"] = params
    body = json.dumps(payload)
    return f"Content-Length: {len(body.encode('utf-8'))}\r\n\r\n{body}".encode("utf-8")

def parse_responses(raw_bytes):
    offset = 0
    responses = []
    header_marker = b"\r\n\r\n"
    while offset < len(raw_bytes):
        header_end = raw_bytes.find(header_marker, offset)
        if header_end < 0:
            break
        header = raw_bytes[offset:header_end].decode("ascii")
        length = int(header.replace("Content-Length: ", "").strip())
        body_start = header_end + 4
        body_bytes = raw_bytes[body_start:body_start + length]
        responses.append(json.loads(body_bytes.decode("utf-8")))
        offset = body_start + length
    return responses

# 测试 1: 发送 initialize 和 tools/list
messages = [
    build_message("initialize", {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "qoderwork-test", "version": "0.5.8"}
    }, 1),
    build_message("notifications/initialized", {}, 2),
    build_message("tools/list", {}, 3),
    build_message("ping", {}, 4),
]

proc = subprocess.Popen(
    [BINARY],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)

stdout, stderr = proc.communicate(input=b"".join(messages), timeout=10)
responses = parse_responses(stdout)

for resp in responses:
    rid = resp.get("id")
    if rid == 1:
        result = resp.get("result", {})
        print(f"initialize: protocolVersion={result.get('protocolVersion')}")
        print(f"  serverInfo: {result.get('serverInfo')}")
    elif rid == 3:
        result = resp.get("result", {})
        tools = result.get("tools", [])
        print(f"tools/list: {len(tools)} tools")
        for tool in tools:
            print(f"  - {tool['name']}")
    elif rid == 4:
        print(f"ping: OK")
