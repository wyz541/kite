#!/usr/bin/env python3
import argparse
import hashlib
import json
from email import policy
from email.parser import BytesParser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

HTML = r'''<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Kite Drag Upload Test</title>
  <style>
    html, body { width: 100%; height: 100%; margin: 0; background: #f2f5f9; }
    body { font: 18px system-ui, sans-serif; color: #162033; }
    main { height: 100%; box-sizing: border-box; padding: 24px; }
    input { box-sizing: border-box; width: 100%; height: calc(100vh - 130px); padding: 40px; border: 4px dashed #3568d4; border-radius: 16px; background: white; font-size: 20px; }
    #status { margin-top: 14px; font-weight: 600; }
  </style>
</head>
<body>
  <main>
    <input id="drop" type="file" multiple>
    <div id="status">Drop a file here</div>
  </main>
  <script>
    const input = document.querySelector('#drop');
    const status = document.querySelector('#status');
    const browser = new URLSearchParams(location.search).get('browser') || 'unknown';
    let uploading = false;

    async function uploadFiles(eventName) {
      if (uploading || input.files.length === 0) return;
      uploading = true;
      status.textContent = 'Uploading...';
      try {
        const body = new FormData();
        body.append('browser', browser);
        body.append('event', eventName);
        for (const file of input.files) body.append('files', file, file.name);
        const response = await fetch('/upload', { method: 'POST', body });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const result = await response.json();
        status.textContent = `Uploaded ${result.files.length} file(s)`;
      } catch (error) {
        status.textContent = `Upload failed: ${error}`;
        document.title = 'Kite Drag Upload Test - FAILED';
      }
    }

    input.addEventListener('dragover', event => event.preventDefault());
    input.addEventListener('drop', () => setTimeout(() => uploadFiles('drop'), 250));
    input.addEventListener('change', () => uploadFiles('change'));
  </script>
</body>
</html>'''


class UploadHandler(BaseHTTPRequestHandler):
    server_version = "KiteDragUploadTest/1.0"

    def do_GET(self):
        parsed = urlparse(self.path)
        if parsed.path == "/":
            body = HTML.encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if parsed.path == "/health":
            self._json(200, {"ok": True})
            return
        self.send_error(404)

    def do_POST(self):
        if urlparse(self.path).path != "/upload":
            self.send_error(404)
            return
        try:
            content_type = self.headers.get("Content-Type", "")
            length = int(self.headers.get("Content-Length", "0"))
            raw = self.rfile.read(length)
            message = BytesParser(policy=policy.default).parsebytes(
                f"Content-Type: {content_type}\r\nMIME-Version: 1.0\r\n\r\n".encode("ascii") + raw
            )
            fields = {}
            files = []
            for part in message.iter_parts():
                name = part.get_param("name", header="content-disposition")
                filename = part.get_filename()
                payload = part.get_payload(decode=True) or b""
                if filename is not None:
                    files.append(
                        {
                            "name": filename,
                            "size": len(payload),
                            "sha256": hashlib.sha256(payload).hexdigest(),
                            "content_type": part.get_content_type(),
                        }
                    )
                elif name:
                    fields[name] = payload.decode(part.get_content_charset() or "utf-8", "replace")
            if not files:
                raise ValueError("upload contained no files")
            record = {
                "browser": fields.get("browser", "unknown"),
                "event": fields.get("event", "unknown"),
                "files": files,
            }
            with self.server.log_path.open("a", encoding="utf-8") as log:
                log.write(json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n")
            self._json(200, {"ok": True, "files": files})
        except Exception as error:
            self._json(400, {"ok": False, "error": str(error)})

    def _json(self, status, value):
        body = json.dumps(value, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        print(f"{self.address_string()} - {format % args}", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--log", type=Path, required=True)
    args = parser.parse_args()
    args.log.parent.mkdir(parents=True, exist_ok=True)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), UploadHandler)
    server.log_path = args.log
    print(f"listening on http://127.0.0.1:{args.port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
