"""Harp on iOS: runs yt-dlp inside the app.

iOS doesn't let apps start other programs, so instead of launching yt-dlp
(as Harp does elsewhere), this script runs inside the app's own Python
(embedded with serious_python) and answers Harp's Rust core on a loopback
port: one JSON line in ({"token", "args"}), one back ({"code", "stdout",
"stderr"}). The arguments are exactly what Harp passes to yt-dlp on every
other platform, so yt-dlp itself does all the work, unchanged.

yt-dlp is the official release zipapp (fetched and checked by Harp, or the
copy shipped in the app), imported straight from the zip.
"""

import contextlib
import io
import json
import os
import socketserver
import sys
import threading

PORT = int(os.environ["HARP_PY_PORT"])
TOKEN = os.environ["HARP_PY_TOKEN"]
ZIPAPP = os.environ.get("HARP_YTDLP", "")

if ZIPAPP and ZIPAPP not in sys.path:
    sys.path.insert(0, ZIPAPP)

import yt_dlp  # noqa: E402  (from the zipapp above)

# yt-dlp keeps process-wide state (stdout, its own settings), so one run at a time.
LOCK = threading.Lock()


def run(args):
    """Runs yt-dlp's command line with `args`; returns (code, stdout, stderr)."""
    out_buf, err_buf = io.BytesIO(), io.BytesIO()
    # Real text streams with a .buffer, as yt-dlp writes through it.
    out = io.TextIOWrapper(out_buf, encoding="utf-8", write_through=True)
    err = io.TextIOWrapper(err_buf, encoding="utf-8", write_through=True)
    code = 0
    with LOCK, contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            yt_dlp.main(list(args))
        except SystemExit as e:
            code = e.code if isinstance(e.code, int) else (0 if e.code is None else 1)
        except BaseException as e:  # anything else: report it like yt-dlp would
            err.write(f"ERROR: {type(e).__name__}: {e}\n")
            code = 1
    return code, out_buf.getvalue().decode("utf-8", "replace"), err_buf.getvalue().decode("utf-8", "replace")


class Handler(socketserver.StreamRequestHandler):
    def handle(self):
        line = self.rfile.readline(1 << 20)
        try:
            req = json.loads(line)
        except ValueError:
            return
        if not isinstance(req, dict) or req.get("token") != TOKEN:
            return
        args = req.get("args")
        if not isinstance(args, list) or not all(isinstance(a, str) for a in args):
            return
        code, out, err = run(args)
        self.wfile.write((json.dumps({"code": code, "stdout": out, "stderr": err}) + "\n").encode("utf-8"))


class Server(socketserver.ThreadingTCPServer):
    daemon_threads = True
    allow_reuse_address = True


with Server(("127.0.0.1", PORT), Handler) as server:
    server.serve_forever()
