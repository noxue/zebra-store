# zebra-lab — multi-site test topology

One Docker Compose project (`zebra-lab`) that runs a complete supply chain around Zebra Store
on one server, configures it end-to-end through each system's HTTP APIs and proves it with real
orders.

| Host (`<name>.${LAB_DOMAIN}`) | What | Role |
|---|---|---|
| `store` | Zebra Store (storefront `/`, admin `/admin/`, API `/api`, `/uploads`, `/shared/*`, `/plugin/open-api/*`) | main shop, reseller feature on (`subdomain_base = LAB_DOMAIN`) |
| `acg` | 异次元发卡 acg-faka, built from its official Dockerfile (bundled MariaDB + Redis) | upstream of `store` (protocol `acg-faka`); also buys from `store` through `/shared/*` |
| `dujiao` | compatibility test service (fullstack binary + Redis) | upstream of `store` (protocol `dujiao-next`) |
| `zs2` | a second Zebra Store | downstream of `store` (protocol `zebra-store`, connection code) |
| `sakura`, `neon`, `matcha` | reseller subsites of `store` (same backend, tenant chosen by `Host`) | three reseller users with their own name, logo, favicon, announcement, SEO and markup |
| `docs` | static VitePress handbook (`../handbook` → `deploy/docs/`) | documentation; no container (host Caddy serves the files in proxy mode) |

```
            buyer ─► store ◄──────── zs2 (zebra-store protocol, pays from its wallet on store)
                     │  ▲ ▲
  acg-faka protocol  │  │ └──────── acg-faka 共享店铺 (/shared/*)
            ┌────────┘  │
            ▼           └── sakura / neon / matcha .LAB_DOMAIN (reseller hosts, same backend)
     acg-faka      dujiao-next ◄── store (dujiao-next protocol)
```

Everything sits behind one **edge** Caddy container that serves the two SPAs and routes by host.
Services talk to each other **through the public hostnames** (exactly as independent shops would).

## Files

| Path | Purpose |
|---|---|
| `compose.yml` | the topology; never used alone |
| `compose.{rehearsal,proxy,caddy}.yml` | one overlay per mode (see below) |
| `caddy/Caddyfile.tmpl` | edge routes; `caddy/zebra.caddy.tmpl` / `host-snippet.nginx.conf.tmpl` for an existing host proxy |
| `config/*.tmpl` | Zebra (`store`, `zs2`) and dujiao-next configs, rendered from `.env` into `build/runtime/` |
| `docker/*.Dockerfile` | Zebra runtime (prebuilt static binary), edge (Caddy + SPAs), tools (provision/verify runner), Zebra-in-Docker fallback builder |
| `init-secrets.sh` | creates `.env` from `.env.example` with random secrets / passwords (keeps existing values) |
| `lab.sh` | `up`, `provision`, `verify`, `all`, `creds`, `ps`, `logs`, `down`, `reset` on the machine that runs the lab |
| `deploy.sh` | one-command remote deploy / uninstall over ssh |
| `scripts/build.sh` | builds all images for one platform |
| `scripts/preflight.sh`, `scripts/host-caddy.sh` | read-only server checks; safe host-Caddy import / rollback / removal |
| `provision/` | `provision.py` (idempotent end-to-end setup), `verify.py` (e2e PASS/FAIL table), `lab.py`, `logos.py` |

`.env` and `build/` are gitignored. Secrets exist only in `.env` on the machine that runs the lab.

## Modes (`LAB_MODE` in `.env`)

| Mode | Edge listens on | TLS | `allow_private_addresses` |
|---|---|---|---|
| `rehearsal` | `127.0.0.1:${EDGE_HTTP_PORT}` (default 18480) | none (plain HTTP) | **true** |
| `proxy` | `127.0.0.1:${PROXY_PORT}` (default 18180) | the host's existing proxy (host Caddy / nginx) | false |
| `caddy` | `:80` + `:443` | edge gets one Let's Encrypt cert per host via HTTP-01 | false |

* **rehearsal** — every lab hostname is a Docker network alias of the edge, so inside the network
  `store.lvh.me`, `acg.lvh.me`, … resolve to the edge's *private* IP. Zebra's SSRF guard refuses
  private addresses for supplier calls, callbacks and pushed events, which is why rehearsal (and only
  rehearsal) sets `integration.allow_private_addresses=true`. With `LAB_DOMAIN=lvh.me` (public DNS
  `*.lvh.me → 127.0.0.1`) the same names also work in a browser on the host:
  `http://store.lvh.me:18480/`, `http://sakura.lvh.me:18480/`, …
  dujiao-next refuses to call back private addresses, so in rehearsal the store→dujiao connection
  has no callback URL and relies on polling (first poll after 30 s).
* **proxy** — our Caddy snippet contains only site blocks for our seven hosts, all
  `reverse_proxy 127.0.0.1:${PROXY_PORT}`; the host proxy keeps TLS and certificates. The edge
  trusts `X-Forwarded-*` only from private ranges (the Docker bridge gateway the host proxy connects from).
* **caddy** — for a clean box: the edge owns :80/:443 and obtains certificates itself (no wildcard cert needed).

Switch modes by editing `LAB_MODE` (and `LAB_DOMAIN`) in `.env`, then `./lab.sh up` (recreates only what changed).

### Real server: hairpin traffic

In `proxy` / `caddy` mode containers resolve `store.<domain>` etc. through public DNS, i.e. to the
server's public IP, and connect back into the host proxy / edge ("hairpin"). Because those addresses
are public, `allow_private_addresses` stays `false`. This works when the public IP is configured on
the server's own interface (typical VPS: `ip addr` shows it) and no host firewall drops traffic from
the Docker bridge to the host's :443.

If the provider NATs the public IP (some clouds) or a firewall blocks it, inter-site calls time out.
Fallback: in `caddy` mode copy the `aliases:` block of `compose.rehearsal.yml` into `compose.caddy.yml`
(names then resolve to the edge inside the network, and the edge still presents the real
certificates) **and** set `ALLOW_PRIVATE_ADDRESSES=true` for that mode in `lab.sh` — knowingly: it
opens Zebra's SSRF guard to private networks, acceptable for a test lab only. In `proxy` mode the
edge has no certificates, so the fallback there is `caddy` mode on a free IP / port pair, or opening
the hairpin path (e.g. `iptables -I INPUT -i br-+ -p tcp --dport 443 -j ACCEPT`).

## Local rehearsal

```bash
cd deploy
./init-secrets.sh                 # .env with random secrets; LAB_MODE=rehearsal, LAB_DOMAIN=lvh.me
scripts/build.sh                  # native images zebra-lab/*:local (first run ~20-30 min)
./lab.sh all                      # up + provision + verify
./lab.sh creds                    # URLs + credentials (also build/state/credentials.txt)
./lab.sh reset                    # delete this project's containers, volumes and state
```

If your resolver fakes `*.lvh.me` (proxy tools with fake-IP DNS), use the Host header:
`curl -H 'Host: store.lvh.me' http://127.0.0.1:18480/api/v1/public/config`.

Build prerequisites: Docker with buildx, Node 20+ (the SPAs), Rust + `zig` + `cargo-zigbuild`
(`brew install zig && cargo install cargo-zigbuild`) for the backend. Without zig:
`ZEBRA_BUILD=docker scripts/build.sh` compiles the backend inside Docker (slower, ~3 GB cache in the Docker VM).
The acg-faka and dujiao-next sources are cloned at pinned commits (`ACG_FAKA_REF`, `DUJIAO_REF` in `scripts/build.sh`).

## Deploy to a server

Prerequisites on the server: Docker + Compose v2, rsync, x86_64, ≥ 10 GB disk, ~2.5 GB RAM free.
DNS: an A record (grey cloud / DNS-only on Cloudflare, so HTTP-01 works) for each host:
`store acg dujiao zs2 sakura neon matcha docs` `.dot2.com → <server IP>`.

```bash
cd deploy
./deploy.sh root@107.174.142.210            # proxy mode behind the host Caddy, LAB_DOMAIN=dot2.com
ssh root@107.174.142.210 /opt/zebra-lab/lab.sh creds
```

`deploy.sh` runs a read-only **preflight** first (arch, docker/compose, disk ≥ 10 GB, memory,
port `127.0.0.1:18180` free, no Docker network / route in our `/16`, host Caddyfile validates) and
aborts on any doubt. Then: build the Zebra images (`zebra`, `edge`, `tools`) locally for
`linux/amd64` (cross-compiled static binary, no emulation) → upload only changed images
(`docker save | ssh docker load`, ~130 MB compressed) → rsync this directory plus the pinned
acg-faka / dujiao-next sources to `/opt/zebra-lab` (never `.env`) → build those two images **on the
server** from their official Dockerfiles (native amd64; peak ~1.5 GB RAM for dujiao-next's node + go
stages, BuildKit cache makes redeploys fast) → `init-secrets.sh` **on the server** → `lab.sh up` →
host Caddy integration → `provision` → `verify`.

Host Caddy integration (`scripts/host-caddy.sh install`): copies our snippet to
`/opt/zebra-lab/caddy/zebra.caddy`, appends exactly one `import /opt/zebra-lab/caddy/zebra.caddy`
line to the Caddyfile named by the systemd unit's `--config` (backup `Caddyfile.bak.zebra-<ts>` first,
idempotent), runs `caddy validate` on the full config and only then `systemctl reload caddy` (never
restart); any failure restores the backup and re-validates.

DNS: add `docs` to the A records as well. The handbook is built locally (`scripts/build-docs.sh`:
`npm ci && npm run docs:build` in `../handbook`, skipped when it does not exist) and rsynced to
`/opt/zebra-lab/docs` (world-readable, so the host Caddy's user can serve it). Republish only the docs:
`./deploy.sh root@107.174.142.210 --docs`. Locally: `./lab.sh docs`.

Other variants:

```bash
./deploy.sh root@host --mode caddy --domain example.com --acme-email you@example.com   # clean box
./deploy.sh root@host --host-caddyfile none      # nginx etc.: use build/runtime/nginx-zebra.conf yourself
./deploy.sh root@host --skip-build               # redeploy with the images already built
./deploy.sh root@host --uninstall                # stack + volumes + import line + /opt/zebra-lab
```

Isolation on a shared server: only `zebra-lab` resources are created or removed — compose project
`zebra-lab` (containers `zebra-lab-*`, network `zebra-lab_lab`, volumes `zebra-lab_*`; `lab.sh
reset/destroy` = `down -v` of that project only), images `zebra-lab/*`, and the dedicated BuildKit
builder `zebra-lab-builder` (container `buildx_buildkit_zebra-lab-builder0`, stopped after each build,
removed with its cache by `--uninstall`, so nothing lands in the shared default build cache). Every
container has a memory / CPU cap (sum ≈ 2.3 GiB) and rotated json logs (3 × 10 MB); no daemon
config, no prune, no host networking; in proxy mode the only published port is `127.0.0.1:18180`
(pick another in 18100–18199 with `--proxy-port`).

## What provisioning sets up (`./lab.sh provision`, idempotent)

1. **acg-faka**: install via `/install/submit` (bundled DB). The admin-login image captcha is on by
   default and cannot be passed over HTTP, so `lab.sh` flips that one setting in acg-faka's database
   (`admin_login_verification=0`) — the only non-HTTP step; the other captchas are turned off through
   the admin API. Category + auto-delivery commodity (price 10, member price 9, `api_status=1`) +
   120 test cards; member `zebrasupply` (balance 5000, `app_key`) = store's upstream account.
2. **store**: settings (site name / `site_url` / registration without e-mail code), the test catalogue
   from `backend/scripts/seed_store.py` + covers, a dedicated `lab-e2e-card` (10.00, 300 cards),
   buyer `buyer@lab.test` with wallet 5000 (**test payment path = admin wallet top-up + balance payment**).
3. **dujiao-next**: compliance acknowledgement, registration without e-mail code, category + auto product
   (12.50) + 120 cards; user `zebra-supply@lab.test` → API credential applied / approved / secret; wallet 5000.
4. **store connections**: `acg-faka` and `dujiao-next` (+20 % markup), upstream products imported and activated.
5. **zs2**: store user `zs2-supply@lab.test` (approved credential, wallet 5000) issues a connection code;
   zs2 parses it, handshakes, connects (`zebra-store`, +10 %), imports `lab-e2e-card`; zs2 buyer with wallet.
6. **resellers**: `reseller-{sakura,neon,matcha}@lab.test` apply → admin approves (default markup 10/20/30 %,
   max 50 %) → system subdomain → site config (name, generated logo/favicon, announcement, support,
   SEO) → per-product markup 15/25/35 % on `lab-e2e-card`.
   Theme colours are shared by all resellers, so reseller `theme` is always `{}`
   and the storefront theme comes from the main site.
7. **acg-faka → store**: store user `acg-down@lab.test` issues an acg-faka compat key; acg-faka adds
   store as 共享店铺 (type 异次元), imports `lab-e2e-card` (+1), member `acgbuyer` gets balance.

Reseller settlement: `SETTLEMENT_CONFIRM_DAYS=0` in `.env` (default for the lab) makes profit
available on the next minute's settlement job, so `verify` can withdraw immediately; set it to 7 for
production-like behaviour (then case d stops at `pending_confirm`).

## Verification (`./lab.sh verify`)

Real orders, paid from wallets / balances; prints a PASS/FAIL table (exit code 1 on failure):

| Case | Asserts |
|---|---|
| a | store order of the acg-sourced product → procurement fulfilled from acg-faka → card `ACG-LAB-…` delivered |
| b | same for the dujiao-sourced product (`DJ-LAB-…`, async delivery via polling / callback) |
| c | zs2 order of the store product → store order paid from zs2's wallet on store (exact debit) → card `ZS-LAB-…` at zs2 |
| d (×3) | order on each reseller host: public config shows the reseller's site name; price = base × (1 + markup) ; reseller order snapshot profit and ledger entry = markup; entry becomes `available`; reseller withdraws; admin marks it paid |
| e | acg-faka member buys the imported store item → acg calls store `/shared/*` → card delivered, store wallet of the acg account debited |

Each run consumes one card per case; provisioning restocks the store product when it drops below 100.

## Third-party quirks handled by the package

| System | Issue | Handling |
|---|---|---|
| acg-faka image (Debian trixie, MariaDB 11.8) | first-boot DB init intermittently fails: the 11.8 client verifies the server's just-generated TLS cert → `certificate is not yet valid`, user `acg` is never created, install then fails with `Access denied` | `config/acg-mariadb-client.cnf` (`[client] skip-ssl`) mounted into the container |
| acg-faka | admin-login image captcha is on after install, no HTTP way past it | `lab.sh` sets `admin_login_verification=0` in its DB once; all other captchas are switched off via `/admin/api/config/setting` |
| dujiao-next | `GET /admin/api-credentials?status=…&search=…` → SQL "ambiguous column name: status" (500) | provisioner filters by `user_id` instead |
| dujiao-next | refuses to call back private addresses | rehearsal: store→dujiao connection without callback URL (polling) |
| reseller sites | per-reseller theme colours are not stored (original project: `theme` is always `{}`) | subsites differ by name, logo/favicon, announcement, support, SEO and pricing |

## Resources

About 2.3 GiB RAM cap in total (acg-faka 768 MB incl. MariaDB, store 384 MB, zs2 384 MB, dujiao 384 MB,
redis 128 MB, edge 128 MB, tools 256 MB while running), ~2.5 GB of images, a few hundred MB of volumes.

## Reset / credentials

* Credentials: `./lab.sh creds` (from `build/state/credentials.txt`, mode 600); secrets in `.env`.
* Start over: `./lab.sh reset && ./lab.sh all` (secrets in `.env` are kept).
* New secrets: `./lab.sh reset && ./init-secrets.sh --force && ./lab.sh all`.
