#!/usr/bin/env python3
"""Lightweight HTTP wrapper for Piper TTS.

Exposes a simple REST API for text-to-speech synthesis
using Piper as the backend engine.

Endpoints:
  GET  /health          - Health check
  POST /api/tts         - Synthesize speech (JSON body: {"text": "..."})
  POST /api/tts/stream  - Synthesize speech (streaming WAV response)
"""

import http.server
import json
import subprocess
import tempfile
import os
import sys

PIPER_BIN = os.environ.get("PIPER_BIN", "/usr/local/bin/piper")
MODEL_PATH = os.environ.get("PIPER_MODEL", "/models/de_DE-thorsten-medium.onnx")
PORT = int(os.environ.get("PIPER_PORT", "8082"))


class PiperHandler(http.server.BaseHTTPRequestHandler):
    """HTTP request handler for Piper TTS."""

    def do_GET(self):
        """Handle GET requests (health check)."""
        if self.path == "/health":
            self._send_json(200, {"status": "ok", "model": os.path.basename(MODEL_PATH)})
        else:
            self._send_json(404, {"error": "not found"})

    def do_POST(self):
        """Handle POST requests (TTS synthesis)."""
        if self.path not in ("/api/tts", "/api/tts/stream"):
            self._send_json(404, {"error": "not found"})
            return

        content_length = int(self.headers.get("Content-Length", 0))
        if content_length == 0:
            self._send_json(400, {"error": "empty request body"})
            return

        body = self.rfile.read(content_length)
        try:
            data = json.loads(body)
        except json.JSONDecodeError:
            self._send_json(400, {"error": "invalid JSON"})
            return

        text = data.get("text", "").strip()
        if not text:
            self._send_json(400, {"error": "missing 'text' field"})
            return

        try:
            audio_data = self._synthesize(text)
            self.send_response(200)
            self.send_header("Content-Type", "audio/wav")
            self.send_header("Content-Length", str(len(audio_data)))
            self.end_headers()
            self.wfile.write(audio_data)
        except subprocess.CalledProcessError as e:
            self._send_json(500, {"error": f"piper failed: {e.stderr.decode()}"})
        except Exception as e:
            self._send_json(500, {"error": str(e)})

    def _synthesize(self, text):
        """Run Piper TTS and return WAV audio bytes."""
        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
            tmp_path = tmp.name

        try:
            subprocess.run(
                [PIPER_BIN, "--model", MODEL_PATH, "--output_file", tmp_path],
                input=text.encode("utf-8"),
                capture_output=True,
                check=True,
                timeout=30,
            )
            with open(tmp_path, "rb") as f:
                return f.read()
        finally:
            if os.path.exists(tmp_path):
                os.unlink(tmp_path)

    def _send_json(self, status, data):
        """Send a JSON response."""
        body = json.dumps(data).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        """Structured log output."""
        sys.stderr.write(f"[piper-http] {self.client_address[0]} {format % args}\n")


def main():
    """Start the Piper HTTP server."""
    if not os.path.exists(PIPER_BIN):
        sys.exit(f"Error: Piper binary not found at {PIPER_BIN}")
    if not os.path.exists(MODEL_PATH):
        sys.exit(f"Error: Model not found at {MODEL_PATH}")

    server = http.server.HTTPServer(("0.0.0.0", PORT), PiperHandler)
    print(f"[piper-http] Serving on port {PORT} with model {os.path.basename(MODEL_PATH)}")
    server.serve_forever()


if __name__ == "__main__":
    main()
