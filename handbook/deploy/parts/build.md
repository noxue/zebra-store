### 获取程序和网页

一共需要三样东西：

| 东西 | 是什么 | 怎么得到 |
|---|---|---|
| `zebra-store` | 后端程序（一个文件） | 在 `backend/` 编译 |
| `storefront/dist` | 用户前台网页 | 在 `storefront/` 打包 |
| `admin/dist` | 管理后台网页 | 在 `admin/` 打包，**必须带 `--base=/admin/`** |

在你自己的电脑（装好 Rust ≥ 1.90 和 Node.js ≥ 20）上执行：

```bash
git clone <仓库地址> zebra-store && cd zebra-store

# 1) 后端：给 Linux 服务器编译一个不依赖任何系统库的静态程序
cargo install cargo-zigbuild        # 第一次需要；macOS 还要 brew install zig
cd backend
rustup target add x86_64-unknown-linux-musl
cargo zigbuild --release -p zs-server --target x86_64-unknown-linux-musl
# 产物：backend/target/x86_64-unknown-linux-musl/release/zebra-store
cd ..

# 2) 用户前台
cd storefront && npm ci && npm run build && cd ..        # 产物：storefront/dist

# 3) 管理后台（放在 /admin/ 路径下）
cd admin && npm ci && npx vite build --base=/admin/ && cd ..   # 产物：admin/dist
```

::: tip 直接在 Linux 服务器上编译
如果你就在 x86_64 Linux 服务器上编译，后端可以简单地用
`cargo build --release -p zs-server`，产物在 `backend/target/release/zebra-store`。
服务器是 ARM（arm64）的话，把上面的 `x86_64-unknown-linux-musl` 换成 `aarch64-unknown-linux-musl`。
:::

::: tip 为什么后台要加 `--base=/admin/`
后台网页默认假设自己放在网站根目录。我们把它放在 `https://你的域名/admin/`，
所以打包时要告诉它“我的根路径是 /admin/”，否则打开后台时 JS 文件会 404，页面一片空白。
:::
