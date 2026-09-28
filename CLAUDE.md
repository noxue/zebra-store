# Zebra Store — 项目开发规范（强制）

Zebra Store 是独立开发的数字商品交易系统：Rust (axum + sea-orm) 后端 + 两个独立 Vue3 TSX 前端
（用户商店 / 管理后台），提供二次元风格 UI 和多种供应商协议适配。

参考资料（改功能前先查）：
- `docs/reference/backend-spec.md` — 数据表、全部路由、业务流程、支付网关、设置项
- `docs/reference/storefront-spec.md` — 用户前台页面/组件/API 规格
- `docs/reference/admin-spec.md` — 管理后台页面/字段/API 规格
- `docs/reference/screenshots/` — 验收截图（布局与信息结构的参考依据）
- `docs/BACKEND_GUIDE.md` — 后端模块实现模式、文件归属、跨模块协作、验证方式（实现任何后端模块前必读）
- `docs/reference/bugfix-lessons.md` — 历史 bug 修复教训与必测用例
- `docs/PLAN.md` 执行计划，`docs/TODO.md` 任务清单（完成一项就勾选一项）

## 仓库结构

```
backend/     Cargo workspace（Rust）
storefront/  用户前台 SPA（Vue3 + TSX, dev :5185）
admin/       管理后台 SPA（Vue3 + TSX, dev :5186）
docs/        计划、TODO、接口规格与截图
```

## 通用原则

1. **保持 API 契约稳定**：路径、方法、JSON 字段名、响应信封
   `{status_code, msg, data, pagination?}`、业务错误用 HTTP 200 + 非 0 status_code、
   金额为两位小数字符串（`"12.30"`）、多语言字段为 `{"zh-CN","zh-TW","en-US"}` 对象。
   有疑问时以仓库内的接口规格和兼容测试为准，不自行发明字段。
2. **小步提交、每步可验证**：每完成 TODO 中的一项，必须编译通过 + 测试通过后再勾选。
3. 不写与任务无关的"顺手重构"；不引入未在本文件列出的重量级依赖前先说明理由。

## Rust 后端规范

依据：Rust API Guidelines、Microsoft Pragmatic Rust Guidelines、Hexagonal Architecture。

### 分层与依赖方向（由 crate 依赖图强制，禁止绕过）

```
zs-shared  ← zs-domain ← zs-app ← zs-infra ← zs-api ← zs-server(bin)
                                   zs-migration ↗
```

| crate | 职责 | 允许依赖 | 禁止 |
|---|---|---|---|
| `zs-shared` | Money/Amount、LocalizedText、分页、序列号、加密/签名、Clock | std、serde、rust_decimal、thiserror | 任何框架/ORM |
| `zs-domain` | 各业务模块的实体、值对象、状态机、纯业务规则、**端口 trait**（Repository/Gateway/Notifier）、领域错误 | zs-shared | axum、sea-orm、reqwest、tokio IO |
| `zs-app` | 用例服务（Service），编排端口；跨模块工作流（下单→支付→发货） | zs-domain、zs-shared、async-trait | axum、sea-orm |
| `zs-infra` | 端口实现：sea-orm 实体与仓储、支付网关、邮件、缓存、任务队列、存储、上游客户端 | 以上 + sea-orm、reqwest、lettre 等 | axum |
| `zs-migration` | 版本化**数据**迁移（种子数据、角色、非增量结构变更）；表结构由 infra 的 sea-orm 实体经 entity-first `schema sync` 自动创建/增列 | sea-orm-migration | 方言专用 SQL（必须时按 backend 分支） |
| `zs-api` | axum 路由、handler、DTO、中间件（JWT/RBAC/限流/i18n/租户/合规） | zs-app、zs-domain、zs-shared | 直接使用 sea-orm / 仓储实现 |
| `zs-server` | 读取配置、组装依赖（唯一的 composition root）、CLI、启动 HTTP 与 worker | 全部 | 业务逻辑 |

- 业务模块在每个 crate 内以同名目录组织（`order/`, `payment/`, `catalog/` …），模块之间只通过 `zs-domain` 中公开的端口/类型交互。
- handler 必须"薄"：解析 DTO → 调用 Service → 转换响应。业务判断放在 domain/app。
- Service 以端口 trait 为依赖（`Arc<dyn XxxRepo>`），便于用 mock 单测。

### 错误处理
- 库 crate（shared/domain/app/infra）使用 `thiserror` 定义具体错误枚举；`zs-server` 可用 `anyhow`。
- 错误转换用 `From`/`#[from]`，避免到处 `map_err`（M-FROM-ERROR）。
- 领域错误 → `ApiError`（带 i18n 消息 key 与 status_code）在 `zs-api` 集中转换。
- **禁止** 在非测试代码中使用 `unwrap()` / `expect()`，除非是构造上不可能失败的不变量，并写明原因（`clippy::unwrap_used` 为 deny）。
- panic 只用于程序 bug（M-PANIC-ON-BUG），不用于可恢复错误。

### 类型与命名
- 金额统一使用 `zs_shared::money::Amount`（rust_decimal，2 位小数，序列化为字符串），禁止 f64 表示金钱。
- 状态、类型等字符串枚举用 Rust enum + `serde(rename_all = "snake_case")`，数据库存字符串。
- ID、订单号等使用 newtype（M-STRONG-TYPES）；newtype 构造时校验不变量。
- 命名简短、无 `Manager/Helper/Util` 之类空泛词（M-WEASEL-WORDS）；公开类型实现 `Debug`。
- 魔法数字必须是具名常量并注释来源（M-DOCUMENTED-MAGIC）。

### 数据库
- 通过配置 `database.url` 切换：`sqlite://data/zebra.db?mode=rwc`（默认）/ `mysql://…` / `postgres://…`。
  业务代码中**禁止**出现针对某一数据库的分支（迁移层除外）。
- 表结构唯一来源是 `zs-infra/src/db/entity/*`（sea-orm 2.0 entity-first，`unique_key` / `indexed` 声明索引），启动时 `sync` 增量建表建列；不要另写建表 SQL。
- 所有表：`id`(bigint 自增)、`created_at`、`updated_at`，需要时 `deleted_at`（软删除）；时间统一 UTC。
- 金额列 `decimal(20,2)`；JSON 列用 `json`（sqlite 下为 text），多语言字段存 JSON。
- 涉及库存/余额/优惠券计数的写操作必须在事务中完成，并做条件更新防超卖。

### 异步、日志与配置
- 使用 `tokio`；阻塞操作（bcrypt、图片处理）放 `spawn_blocking`。
- 使用 `tracing` 结构化日志（M-LOG-STRUCTURED），禁止 `println!`（M-LOG-NOT-PRINT）。
- 避免全局可变 static（M-AVOID-STATICS）；依赖通过 `AppState` 注入。
- 时间、随机数等通过可 mock 的端口获取（M-MOCKABLE-SYSCALLS），保证测试确定性。

### 测试
- domain 纯函数（定价、状态机、优惠计算）必须有单元测试，覆盖边界。
- app Service 用 mock 端口做单元测试。
- `backend/crates/api/tests/` 下为集成测试：内存 SQLite + 真实路由，按公开契约断言响应结构。
- 不写"同义反复"测试（M-TAUTOLOGICAL-TESTS）：断言具体期望值，而不是把实现再算一遍。
- 测试工具放在 `testkit` feature 或 `#[cfg(test)]` 中（M-TEST-UTIL）。

### 工具链（每次提交前必须通过）
```
cd backend
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
- workspace 级统一 `[workspace.lints]`、`[workspace.dependencies]`、edition 2024（M-CARGO-WORKSPACE）。
- lint 例外使用 `#[expect(lint, reason = "...")]`，不用 `#[allow]`（M-LINT-OVERRIDE-EXPECT）。
- 禁止 `unsafe`（`unsafe_code = "forbid"`）。

## 前端规范（storefront / admin 相同）

- Vue 3 + Vite + TypeScript strict + **TSX**（`defineComponent` + `setup` 返回渲染函数），不写 `.vue` SFC。
- 状态：Pinia；路由：vue-router；国际化：vue-i18n（zh-CN / zh-TW / en-US）；样式：Tailwind CSS v4 + CSS 变量主题。
- 分层：`api/`（纯请求函数 + 类型）→ `composables/`（页面逻辑，可测试）→ `views/`（仅渲染）→ `components/`（可复用 UI）。
  视图组件里不直接写 fetch；业务逻辑写进 composable。
- 类型：API 请求/响应类型集中在 `api/types.ts`，与后端 DTO 同名同字段；禁止 `any`（必要时 `unknown` + 收窄）。
- 站点名、Logo、Favicon、主题色、背景图等全部来自 `/api/v1/public/config`，不得硬编码。
- 二次元风格：统一使用 `src/styles/theme.css` 中的设计 token（颜色、圆角、阴影、渐变、动效），
  组件内不写零散的魔法颜色值；支持亮/暗主题；动效需尊重 `prefers-reduced-motion`。
- 检查：`npm run typecheck && npm run lint && npm run test && npm run build` 必须通过。
- localStorage key 保持向后兼容（见 storefront-spec.md 末尾）。

## 回归经验（强制）
- 实现或修改模块前，查看 `bugfix-lessons.md` 的相关记录，避免重犯同类 bug。
- `bugfix-lessons.md` 中的每个“必测用例”都必须有对应的自动化测试，并在测试中标注编号。

## 提交与流程
- 按 `docs/TODO.md` 顺序开发；开始一项前标记 `[~]`，完成并验证后标记 `[x]`。
- 新增 API 时同步：后端路由 + 集成测试 + 前端 `api/` 类型。
- 管理端新增路由必须同时加入 RBAC 内置角色种子与权限目录。
