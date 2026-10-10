#!/usr/bin/env python3
"""Sync approved reseller domains into an Nginx include file.

Run periodically after configuring the HTTP ACME webroot and the include path.
Only this script's output file is modified. Certificate renewal is handled by
Certbot's timer; set its deploy hook to reload Nginx.
"""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.request import Request, urlopen


DOMAIN = re.compile(r"(?=.{1,253}$)(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\Z")


def run(*args: str) -> None:
    subprocess.run(args, check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--token-file", type=Path, required=True)
    parser.add_argument("--email", required=True, help="ACME account email")
    parser.add_argument("--webroot", type=Path, default=Path("/var/www/zebra-acme"))
    parser.add_argument("--output", type=Path, default=Path("/etc/nginx/zebra-resellers.conf"))
    parser.add_argument("--upstream", default="127.0.0.1:8081")
    args = parser.parse_args()
    token = args.token_file.read_text(encoding="utf-8").strip()
    if not token:
        parser.error("token file is empty")
    if not re.fullmatch(r"(?:127\.0\.0\.1|localhost):[0-9]{1,5}", args.upstream):
        parser.error("upstream must be a local host and port")

    request = Request(
        f"http://{args.upstream}/api/v1/internal/reseller-domains",
        headers={"Authorization": f"Bearer {token}"},
    )
    with urlopen(request, timeout=10) as response:
        payload = json.load(response)
    domains = payload.get("data", {}).get("domains")
    if not isinstance(domains, list) or any(not isinstance(name, str) or not DOMAIN.fullmatch(name) for name in domains):
        raise ValueError("invalid domain inventory")

    blocks = ["# Managed by sync-nginx-reseller-domains.py; do not edit.\n"]
    failures = []
    for name in sorted(set(domains)):
        cert = Path("/etc/letsencrypt/live") / name / "fullchain.pem"
        key = cert.with_name("privkey.pem")
        if not cert.is_file() or not key.is_file():
            try:
                run("certbot", "certonly", "--webroot", "-w", str(args.webroot), "-d", name,
                    "--non-interactive", "--agree-tos", "--email", args.email)
            except subprocess.CalledProcessError:
                failures.append(name)
                continue
        if not cert.is_file() or not key.is_file():
            failures.append(name)
            continue
        blocks.append(f"""server {{
    listen 443 ssl;
    server_name {name};
    ssl_certificate {cert};
    ssl_certificate_key {key};
    client_max_body_size 20m;
    location ^~ /api/v1/internal/ {{ return 404; }}
    location / {{
        proxy_pass http://{args.upstream};
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 120s;
    }}
}}
""")

    output = args.output
    old = output.read_bytes() if output.exists() else None
    new = "\n".join(blocks).encode()
    if old != new:
        temporary = output.with_name(output.name + ".tmp")
        temporary.write_bytes(new)
        os.replace(temporary, output)
        try:
            run("nginx", "-t")
            run("systemctl", "reload", "nginx")
        except subprocess.CalledProcessError:
            if old is None:
                output.unlink(missing_ok=True)
            else:
                output.write_bytes(old)
            raise
    if failures:
        print("Certificate issuance failed for: " + ", ".join(failures), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
