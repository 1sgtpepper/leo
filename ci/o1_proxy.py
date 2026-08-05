#!/usr/bin/env python3

import argparse
from http.client import HTTPConnection
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class ProxyHandler(BaseHTTPRequestHandler):
    broadcast_count = 0
    target_host = "127.0.0.1"
    target_port = 3030

    def _forward(self, body=None):
        if body is None:
            body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        headers = {
            key: value
            for key, value in self.headers.items()
            if key.lower() not in {"connection", "content-length", "host"}
        }
        if body:
            headers["Content-Length"] = str(len(body))

        connection = HTTPConnection(self.target_host, self.target_port)
        connection.request(self.command, self.path, body=body or None, headers=headers)
        response = connection.getresponse()
        payload = response.read()

        self.send_response(response.status)
        for key, value in response.getheaders():
            if key.lower() not in {"connection", "content-length", "transfer-encoding"}:
                self.send_header(key, value)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self):
        self._forward()

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        if self.path.endswith("/transaction/broadcast"):
            type(self).broadcast_count += 1
            if type(self).broadcast_count == 1:
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", "2")
                self.end_headers()
                self.wfile.write(b"{}")
                return
        self._forward(body)

    def log_message(self, *_args):
        return


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--listen-port", type=int, required=True)
    parser.add_argument("--target-port", type=int, required=True)
    args = parser.parse_args()
    ProxyHandler.target_port = args.target_port
    ThreadingHTTPServer(("127.0.0.1", args.listen_port), ProxyHandler).serve_forever()


if __name__ == "__main__":
    main()
