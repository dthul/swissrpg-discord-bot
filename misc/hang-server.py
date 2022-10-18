import http.server


def main():
    server_address = ("127.0.0.1", 8080)
    httpd = http.server.HTTPServer(server_address, RequestHandler)
    httpd.serve_forever()


class RequestHandler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    # def __init__(request, client_address, server):
    #     super().__init__(request, client_address, server)
    def do_POST(self):
        if self.path == "/hang":
            self.send_response(200)
            self.send_header("Content-Length", 1)
            self.end_headers()

    def do_GET(self):
        print(self.path)
        if self.path == "/hang":
            self.send_response(200)
            self.send_header("Content-Length", 1)
            self.end_headers()


if __name__ == "__main__":
    main()
