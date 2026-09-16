"""Exercise the running debug desktop, only against an explicitly isolated /tmp vault.

Usage: python3 scripts/test-ui-sandbox.py /tmp/passvalet-ui.XXXXXX
The app must have PASSVALET_DEV_SKIP_PRESENCE=1. Never use real credentials.
"""
import concurrent.futures
import json
import pathlib
import socket
import sqlite3
import sys
import time

home = pathlib.Path(sys.argv[1]).resolve()
assert home.parent == pathlib.Path("/tmp").resolve() and home.name.startswith("passvalet-ui."), "isolated test vault required"


def request(method, params=None, connection=None):
    own = connection is None
    sock = connection or socket.socket(socket.AF_UNIX)
    if own:
        sock.connect(str(home / "passvalet.sock"))
    sock.settimeout(15)
    sock.sendall((json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}}) + "\n").encode())
    data = b""
    while b"\n" not in data:
        data += sock.recv(65536)
    if own:
        sock.close()
    return json.loads(data.split(b"\n")[0])


def ok(method, params=None):
    response = request(method, params)
    assert "error" not in response, (method, response)
    return response["result"]


status = ok("status")
assert not status["vault"]["initialized"], "Use a fresh disposable vault; refusing to alter an existing one"
ok("dev.setup")
assert ok("status")["vault"]["locked"] is False
print("PASS native debug setup; no Keychain/Touch ID used")

fixtures = [("supabase", "anon_key"), ("supabase", "service_role_key"), ("openai", "api_key"), ("anthropic", "api_key"), ("vercel", "token"), ("github", "personal_access_token")]
for service, key_type in fixtures:
    ok("dev.add_secret", {"service": service, "key_type": key_type, "value": f"FAKE-UI-TEST-{service}-{key_type}", "source": "manual", "label": "仅供界面测试"})
assert len(ok("list_keys")["keys"]) == 6
print("PASS six fake secrets, including two keys for one service")

manifest = {"agent": {"name": "测试 Claude Code", "client": "ui-test"}, "purpose": "验证本地测试数据库连接", "requests": [{"service": "supabase", "key_type": "anon_key", "access": "read"}], "ttl_seconds": 900, "project": "/tmp/FAKE-private-project"}
with concurrent.futures.ThreadPoolExecutor() as pool:
    for approve in [False, True]:
        pending = pool.submit(request, "request_permissions", {"manifest": manifest, "wait_seconds": 15})
        for _ in range(30):
            decision = request("dev.decide", {"approve": approve})
            if "error" not in decision:
                break
            time.sleep(.1)
        result = pending.result()
        if not approve:
            assert "error" in result
            assert ok("status")["vault"]["active_session_count"] == 0
            print("PASS denied request does not create a session")
        else:
            session = result["result"]
            token = session["session_token"]
            assert session["session"]["grants"][0]["access"] == "read"
            read = ok("get_key", {"session_token": token, "service": "supabase", "key_type": "anon_key"})
            assert read["value"] == "FAKE-UI-TEST-supabase-anon_key"
            assert "error" in request("get_key", {"session_token": token, "service": "openai", "key_type": "api_key"})
            print("PASS approved read returns exact fake value; ungranted service denied")

ext = socket.socket(socket.AF_UNIX)
ext.connect(str(home / "passvalet.sock"))
assert "result" in request("ext.hello", {"extension_version": "ui-test", "browser": "simulated Chrome bridge"}, ext)
assert ok("status")["extension_connected"] is True
ext.close()
for _ in range(30):
    if not ok("status")["extension_connected"]:
        break
    time.sleep(.1)
assert not ok("status")["extension_connected"]
print("PASS extension bridge attach/disconnect (simulated extension, real native socket)")

db = sqlite3.connect(f"file:{home / 'vault.sqlite'}?mode=ro", uri=True)
db.row_factory = sqlite3.Row
entries = [dict(row) for row in db.execute("SELECT * FROM audit_log ORDER BY id DESC LIMIT 500")]
reads = [e for e in entries if e["event"] == "key_read" and e["session_id"]]
assert len(reads) == 1
assert any(e["event"] == "session_denied" for e in entries)
assert any(e["event"] == "key_denied" for e in entries)
print("PASS audit has exactly one successful session read, separately recorded denials")
print("PASS sandbox path:", home)
