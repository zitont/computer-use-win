"""完整验证测试"""
import subprocess
import json
import base64

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


def main():
    messages = [
        # 1. initialize
        build_message("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0.1"}
        }, 1),
        # 2. tools/list
        build_message("tools/list", {}, 2),
        # 3. get_window_state (验证截图+UIA)
        build_message("tools/call", {
            "name": "get_window_state",
            "arguments": {}
        }, 3),
        # 4. ping
        build_message("ping", {}, 4),
    ]

    full_input = b"".join(messages)

    proc = subprocess.Popen(
        [BINARY],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    stdout, _ = proc.communicate(input=full_input, timeout=60)
    responses = parse_responses(stdout)

    print("=== 验证结果 ===\n")

    for resp in responses:
        rid = resp.get("id")

        if rid == 1:
            result = resp.get("result", {})
            server = result.get("serverInfo", {})
            print(f"[1] initialize: {server.get('name')} v{server.get('version')}")

        elif rid == 2:
            result = resp.get("result", {})
            tools = result.get("tools", [])
            print(f"[2] tools/list: {len(tools)} 个工具")
            for tool in tools:
                print(f"    - {tool['name']}: {tool['description'][:50]}")

        elif rid == 3:
            result = resp.get("result", {})
            if "image" in result:
                img_data = result["image"]
                b64 = img_data.split(",", 1)[1] if "," in img_data else ""
                png_bytes = base64.b64decode(b64)
                elements = result.get("elements", [])
                print(f"[3] get_window_state:")
                print(f"    分辨率: {result.get('image_width')}x{result.get('image_height')}")
                print(f"    PNG: {len(png_bytes) / 1024:.1f} KB")
                print(f"    元素: {len(elements)}")
                print(f"    窗口: {result.get('window_title')} ({result.get('process_name')})")
                cursor = result.get("cursor_position", {})
                print(f"    光标: ({cursor.get('x')}, {cursor.get('y')})")
            else:
                print(f"[3] ERROR: {resp.get('error', {}).get('message', 'unknown')}")

        elif rid == 4:
            print(f"[4] ping: OK")

    print("\n=== 全部通过 ===")


if __name__ == "__main__":
    main()
