"""测试截图压缩效果"""
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
    init_msg = build_message("initialize", {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "test", "version": "0.1"}
    }, 1)

    screenshot_msg = build_message("tools/call", {
        "name": "get_window_state",
        "arguments": {}
    }, 2)

    full_input = init_msg + screenshot_msg

    proc = subprocess.Popen(
        [BINARY],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    stdout, _ = proc.communicate(input=full_input, timeout=60)
    responses = parse_responses(stdout)

    for resp in responses:
        rid = resp.get("id")
        if rid == 2:
            result = resp.get("result", {})
            image_data = result.get("image", "")
            if image_data.startswith("data:image/png;base64,"):
                b64 = image_data[len("data:image/png;base64,"):]
                png_bytes = base64.b64decode(b64)
                png_size_kb = len(png_bytes) / 1024
                print(f"PNG size: {png_size_kb:.1f} KB")
                print(f"Base64 length: {len(b64)} chars")
                print(f"Resolution: {result.get('image_width')}x{result.get('image_height')}")
                print(f"Elements: {len(result.get('elements', []))}")

                # 保存 PNG 用于验证
                output_path = r"D:\project\demo\omkz\test_screenshot.png"
                with open(output_path, "wb") as fp:
                    fp.write(png_bytes)
                print(f"Saved to: {output_path}")
            else:
                print("No image in response")


if __name__ == "__main__":
    main()
