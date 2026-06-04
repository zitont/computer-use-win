"""验证换行符分隔协议"""
import subprocess
import json

BINARY = r"D:\project\demo\omkz\target\release\computer-use-win.exe"

def build_message(method, params=None, req_id=1):
    payload = {"jsonrpc": "2.0", "id": req_id, "method": method}
    if params is not None:
        payload["params"] = params
    return json.dumps(payload) + "\n"

# 发送消息
messages = [
    build_message("initialize", {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "qoderwork", "version": "0.5.8"}
    }),
    build_message("notifications/initialized"),
    build_message("tools/list", {}, 2),
    build_message("ping", {}, 3),
]

proc = subprocess.Popen(
    [BINARY],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)

stdout, stderr = proc.communicate(input="".join(messages).encode("utf-8"), timeout=10)

print("=== Stdout ===")
for line in stdout.decode("utf-8", errors="replace").strip().split("\n"):
    if line.strip():
        resp = json.loads(line)
        rid = resp.get("id")
        if rid == 1:
            result = resp.get("result", {})
            print(f"initialize: protocolVersion={result.get('protocolVersion')}")
            print(f"  serverInfo: {result.get('serverInfo')}")
        elif rid == 2:
            tools = resp.get("result", {}).get("tools", [])
            print(f"tools/list: {len(tools)} tools")
            for t in tools:
                print(f"  - {t['name']}")
        elif rid == 3:
            print(f"ping: OK")

print("\n=== Log ===")
with open(r"D:\project\demo\omkz\server.log", "r", encoding="utf-8") as f:
    for line in f.readlines()[-10:]:
        print(line.rstrip())
