"""Shared helpers for provision.py / verify.py: lab topology, HTTP clients, state file.

Only the public / admin HTTP APIs of each system are used. Every client talks to the
public hostname (<name>.<LAB_DOMAIN>) exactly like a browser or another shop would.
"""
import http.cookiejar
import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

DOMAIN = os.environ["LAB_DOMAIN"]
SCHEME = os.environ.get("LAB_SCHEME", "http")
MODE = os.environ.get("LAB_MODE", "rehearsal")
STATE_DIR = os.environ.get("LAB_STATE_DIR", "/lab/state")
STATE_FILE = os.path.join(STATE_DIR, "state.json")
USER_PASSWORD = os.environ["LAB_USER_PASSWORD"]
TIMEOUT = 90

RESELLERS = {
    # name: (default markup %, per-product markup %, look)
    "sakura": ("10", "15", {"title": "Sakura 樱花小铺", "letter": "S", "colors": ("#ff8fb8", "#ffc6dc"), "motif": "petals",
                            "announcement": "🌸 樱花季全场自动发货，满 100 送小惊喜", "type": "success"}),
    "neon": ("20", "25", {"title": "NEON 霓虹卡屋", "letter": "N", "colors": ("#7c3aed", "#06b6d4"), "motif": "grid",
                          "announcement": "⚡ 24h 在线 · 秒发卡密 · 夜猫子专属", "type": "info"}),
    "matcha": ("30", "35", {"title": "Matcha 抹茶杂货铺", "letter": "M", "colors": ("#4d7c0f", "#a3c585"), "motif": "leaves",
                            "announcement": "🍵 慢慢挑，抹茶味的数字小铺，售后请走工单", "type": "warning"}),
}


def url(name):
    return f"{SCHEME}://{name}.{DOMAIN}"


def host(name):
    return f"{name}.{DOMAIN}"


def log(msg):
    print(time.strftime("%H:%M:%S"), msg, flush=True)


# ---------------------------------------------------------------- state
def load_state():
    try:
        with open(STATE_FILE) as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def save_state(state):
    os.makedirs(STATE_DIR, exist_ok=True)
    tmp = STATE_FILE + ".tmp"
    with open(tmp, "w") as f:
        json.dump(state, f, indent=1, ensure_ascii=False)
    os.chmod(tmp, 0o600)
    os.replace(tmp, STATE_FILE)


class ApiError(Exception):
    def __init__(self, method, path, status, code, msg, data=None):
        super().__init__(f"{method} {path} -> http {status} status_code={code} msg={msg!r}")
        self.status, self.code, self.msg, self.data = status, code, msg, data


# ---------------------------------------------------------------- JSON API (Zebra and compatible services)
class Api:
    """Client for the shared `{status_code, msg, data}` JSON envelope."""

    def __init__(self, base, token="", name=""):
        self.base = base.rstrip("/")
        self.token = token
        self.name = name or base

    def raw(self, method, path, body=None, headers=None, form=None, token=None):
        data = None
        hdrs = {"Accept": "application/json"}
        if form is not None:
            data, ctype = form
            hdrs["Content-Type"] = ctype
        elif body is not None:
            data = json.dumps(body).encode()
            hdrs["Content-Type"] = "application/json"
        tok = self.token if token is None else token
        if tok:
            hdrs["Authorization"] = "Bearer " + tok
        hdrs.update(headers or {})
        req = urllib.request.Request(self.base + "/api/v1" + path, data=data, method=method, headers=hdrs)
        try:
            with urllib.request.urlopen(req, timeout=TIMEOUT) as res:
                return res.status, res.read()
        except urllib.error.HTTPError as e:
            return e.code, e.read()

    def call(self, method, path, body=None, ok_codes=(0,), **kw):
        status, raw = self.raw(method, path, body, **kw)
        try:
            out = json.loads(raw or b"null")
        except ValueError:
            raise ApiError(method, path, status, None, raw[:300].decode(errors="replace"))
        if not isinstance(out, dict) or "status_code" not in out:
            raise ApiError(method, path, status, None, str(out)[:300])
        if out["status_code"] not in ok_codes:
            raise ApiError(method, path, status, out["status_code"], out.get("msg"), out.get("data"))
        return out.get("data")

    def page(self, path, body=None):
        """Returns (data, pagination) for list endpoints."""
        status, raw = self.raw("GET", path, body)
        out = json.loads(raw)
        if out.get("status_code") != 0:
            raise ApiError("GET", path, status, out.get("status_code"), out.get("msg"))
        return out.get("data"), out.get("pagination")

    def get(self, path, **kw):
        return self.call("GET", path, **kw)

    def post(self, path, body=None, **kw):
        return self.call("POST", path, body, **kw)

    def put(self, path, body=None, **kw):
        return self.call("PUT", path, body, **kw)

    def patch(self, path, body=None, **kw):
        return self.call("PATCH", path, body, **kw)

    def upload(self, path, filename, content, fields=None, mime="image/png"):
        boundary = uuid.uuid4().hex
        parts = b""
        for k, v in (fields or {}).items():
            parts += f"--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n".encode()
        parts += (f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n"
                  f"Content-Type: {mime}\r\n\r\n").encode() + content + f"\r\n--{boundary}--\r\n".encode()
        return self.call("POST", path, form=(parts, f"multipart/form-data; boundary={boundary}"))


def zebra_admin(name, password, state, key):
    """Admin client for a compatible instance; reuses a cached token (login is rate limited)."""
    api = Api(url(name), name=name)
    tok = state.get("tokens", {}).get(key)
    if tok:
        api.token = tok
        status, raw = api.raw("GET", "/admin/authz/me")
        try:
            if status == 200 and json.loads(raw).get("status_code") == 0:
                return api
        except ValueError:
            pass
    api.token = api.post("/admin/login", {"username": "admin", "password": password}, token="")["token"]
    state.setdefault("tokens", {})[key] = api.token
    save_state(state)
    return api


def zebra_user(name, email, password, state, key, host_name=None, register=True):
    """Registers (if needed) and logs in a storefront user; token cached in state."""
    api = Api(url(host_name or name), name=f"{email}@{host_name or name}")
    tok = state.get("tokens", {}).get(key)
    if tok:
        api.token = tok
        status, raw = api.raw("GET", "/me")
        try:
            if status == 200 and json.loads(raw).get("status_code") == 0:
                return api
        except ValueError:
            pass
    body = {"email": email, "password": password}
    if register:
        try:
            out = api.post("/auth/register", {**body, "agreement_accepted": True}, token="")
            api.token = out["token"]
        except ApiError:
            api.token = ""
    if not api.token:
        api.token = api.post("/auth/login", {**body, "remember_me": True}, token="")["token"]
    state.setdefault("tokens", {})[key] = api.token
    save_state(state)
    return api


# ---------------------------------------------------------------- acg-faka (form + cookie session)
class Acg:
    """acg-faka admin / member session. Every /admin/api and /user/api call needs a Referer
    whose host equals the Host header (ManageSession interceptor)."""

    def __init__(self, role, state_key):
        self.base = url("acg")
        self.referer = self.base + ("/admin/" if role == "admin" else "/user/")
        self.jar = http.cookiejar.LWPCookieJar(os.path.join(STATE_DIR, f"acg-{state_key}.cookies"))
        try:
            self.jar.load(ignore_discard=True, ignore_expires=True)
        except OSError:
            pass
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))

    def request(self, method, path, data=None):
        body = urllib.parse.urlencode(data or [], doseq=True).encode() if method == "POST" else None
        req = urllib.request.Request(self.base + path, data=body, method=method)
        req.add_header("Referer", self.referer)
        if body is not None:
            req.add_header("Content-Type", "application/x-www-form-urlencoded")
        try:
            with self.opener.open(req, timeout=TIMEOUT) as res:
                out = res.read().decode(errors="replace")
        except urllib.error.HTTPError as e:
            out = e.read().decode(errors="replace")
            raise ApiError(method, path, e.code, None, out[:2000]) from e
        os.makedirs(STATE_DIR, exist_ok=True)
        self.jar.save(ignore_discard=True, ignore_expires=True)
        return out

    def post(self, path, data=None, expect_ok=True):
        raw = self.request("POST", path, data)
        try:
            out = json.loads(raw)
        except ValueError:
            raise ApiError("POST", path, 200, None, raw[:300])
        if expect_ok and out.get("code") != 200:
            raise ApiError("POST", path, 200, out.get("code"), out.get("msg"), out.get("data"))
        return out


def wait_for(fn, timeout, interval=2.0, what="condition"):
    """Polls fn() until it returns a truthy value; returns it or raises TimeoutError."""
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        try:
            last = fn()
            if last:
                return last
        except (ApiError, OSError) as e:  # OSError covers URLError / refused / TLS not ready
            last = e
        time.sleep(interval)
    raise TimeoutError(f"timeout after {timeout}s waiting for {what} (last: {last})")
