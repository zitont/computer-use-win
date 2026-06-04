"""测试 list_installed_apps 功能"""
import subprocess
import json

BINARY = r"D:\project\demo\omkz\target\release\computer-use-win.exe"


def build_message(method, params=None, req_id=1):
    """构建 MCP JSON-RPC 消息"""
    payload = {"jsonrpc": "2.0", "id": req_id, "method": method}
    if params is not None:
        payload["params"] = params
    body = json.dumps(payload)
    return f"Content-Length: {len(body.encode('utf-8'))}\r\n\r\n{body}".encode("utf-8")


def parse_responses(raw_bytes):
    """从原始字节流解析 Content-Length 帧响应"""
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
    init_msg = build_message("initialize", {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "test", "version": "0.1"}
    }, 1)

    list_msg = build_message("tools/call", {
        "name": "list_installed_apps",
        "arguments": {}
    }, 2)

    filter_msg = build_message("tools/call", {
        "name": "list_installed_apps",
        "arguments": {"filter": "notepad"}
    }, 3)

    full_input = init_msg + list_msg + filter_msg

    proc = subprocess.Popen(
        [BINARY],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    stdout, stderr = proc.communicate(input=full_input, timeout=30)

    responses = parse_responses(stdout)

    for i, resp in enumerate(responses):
        print(f"--- Response {i + 1} (id={resp.get('id')}) ---")
        if "error" in resp and resp["error"]:
            print(f"  ERROR: {resp['error']['message']}")
        elif "result" in resp and isinstance(resp["result"], dict):
            result = resp["result"]
            if "apps" in result:
                print(f"  count: {result['count']}")
                for app in result["apps"][:10]:
                    print(f"  - {app['name']}: {app['aumid']}")
                if result["count"] > 10:
                    print(f"  ... and {result['count'] - 10} more")
            elif "serverInfo" in result:
                print(f"  server: {result['serverInfo']['name']} v{result['serverInfo']['version']}")
            else:
                print(json.dumps(result, indent=2, ensure_ascii=False)[:500])
        else:
            print(json.dumps(resp, indent=2, ensure_ascii=False)[:500])

    if stderr:
        err_text = stderr.decode("utf-8", errors="replace")[:200]
        if err_text.strip():
            print(f"\nstderr: {err_text}")


if __name__ == "__main__":
    main()
