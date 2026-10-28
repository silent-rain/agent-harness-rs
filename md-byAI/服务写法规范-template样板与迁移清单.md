# 服务写法规范与迁移清单（template-service 样板）

> 2026-10-26 基于 `services/template-service` + `crates/grpc-proto/proto/v1/template/` 改造经验，
> 以及与 `services/system-service`（旧写法）的全量对比总结。
> 用途：后续优化其他服务的 proto 文件、dto、dao、service/controller 层时按本清单套用。

---

## 一、proto 消息定义规范（template.proto）

以 `proto/v1/template/template.proto` 为样板。

### 1.1 Req/Resp 严格成对

- 每个 rpc 都有专属的 `<Action><Resource>Resp`，**禁止通用 `XxxResp` 兜底**（template 原 `TemplateResp` 已删除）。
- 响应语义写实，不虚挂空 data：

| 场景 | data 字段 |
| --- | --- |
| 创建并返回实体（Create） | `data: <Entity>` |
| 返回统计数（Delete / BatchDelete / BatchCreate） | `data: uint64`（条数） |
| 纯操作无返回值（Update / UpdateEnabled / SoftDelete） | 只留 `code` / `message` |

### 1.2 响应字段号固定

所有 Resp 三件套字段号统一：`code = 1`、`message = 2`、`data = 3`。
列表分页载荷用嵌套消息且声明放消息尾部：

```proto
message GetTemplatesResp {
  uint32 code = 1;
  string message = 2;
  Data data = 3;

  // 分页载荷
  message Data {
    repeated Template data_list = 1;
    uint64 total = 2;
  }
}
```

### 1.3 注释规范

- **只允许行注释（`//`），禁止块注释（`/* */`）**。
  原因：proto 注释是跨语言契约文档，TS 侧由 ts-proto 从 descriptor 生成 `packages/proto-types`，
  **生成物只保留 `//` 行注释，块注释会被丢弃**——用块注释的字段在 TS 类型上没有文档，
  IDE 悬停与 OpenAPI 文档同步缺失。
- **注释必须独占一行、置于字段上方**，禁止行尾注释——ts-proto 只把**前置行注释**生成为
  TS 类型的 TSDoc，行尾注释会被丢弃：
  ````proto
  // 会员等级ID
  int64 id = 1;
  ````
- 逐字段补齐，消息头部一句定位（"添加模板 请求体" / "响应体"）。
- 注释文案与 dto 的 TSDoc 保持同源——proto 是跨语言契约，注释即文档。

### 1.4 类型与可选性对齐 dto

- dto `Option<String>` ↔ proto `optional string`（proto3 optional；build.rs 已配置
  `--experimental_allow_proto3_optional` 与全量 serde derive）。
- 整数宽度对齐：dto `i64` ↔ proto `int64`；分页 `u64` ↔ proto `uint64`。
- 时间字段当前用 `string`（与 dto NaiveDateTime 序列化口径一致）；如需跨语言强类型再评估
  `google.protobuf.Timestamp`，暂不统一改。

### 1.5 读 / 写模型分离

- 实体消息（如 `Template`）含服务端生成的 `id` / `template_key` / 时间戳（读模型）。
- Create 类请求只收客户端可传字段（写模型），服务端生成字段不收。
- 批量创建的条目消息独立定义（如 `BatchCreateTemplateItem`），不与 Create 请求复用。

---

## 二、service.proto 规范

以 `proto/v1/template/service.proto` 为样板。

1. 每个域一个 `service XxxService` 文件，rpc 按 CRUD 生命周期排序：
   `list → info → create → batch_create → update → update_enabled → delete → soft_delete → batch_delete`。
2. **每个 rpc 必带 `google.api.http` 注解**（REST 路由由 `tonic-rest-build` 从 descriptor 自动生成到
   `rest_routes.rs`），路径风格 `/api/v1/<域>/<资源>/<action>`：
   - 读：`get`（无 body）
   - 写：`post` / `put` / `patch`，`body: "*"`
3. **proto 注解是路由的唯一事实源**，手写 router 必须与其核对消除漂移。template 已发现的漂移（待修）：
   - `enabled`：proto `patch` vs `router.rs` `put`

### 2.1 HTTP 动词选型约定（2026-10-27 修订）

`update_enabled` → `patch`，不用 `put`：

- REST 语义：`PUT` 是**全量替换**，客户端须提交资源的完整表示，未传字段应被解释为「置空」；
  `PATCH` 是**部分更新**，只提交变更字段。
- 本接口只收 `{ id, enabled }`，服务端实现是
  `ActiveModel { id, enabled, ..Default::default() }.update()`（仅 update 非缺省字段），
  属典型局部更新，语义上就是 PATCH。
- 附带收益：`update_enabled` 实际幂等（同状态重复设置结果不变），不损失 PUT 的幂等优点。

`batch_delete` → `delete`（2026-10-27 由 `post` 统一改为 `delete`）：

- 与单条 `delete` 对齐协议语义，「删除就是 DELETE」；批量只是数量维度，不再建模为动作资源。
- 载荷 `ids` 仍走 JSON body（proto 注解必须保留 `body: "*"`）。
  ⚠ 生成器限制：tonic-rest-build 0.1.5 只有 `get` 生成 `Query<T>` 提取器，
  `delete` 不带 `body: "*"` 时请求体会被填成 default，query 传参绑不上——**DELETE 携带载荷必须写 `body: "*"`**。
- 已知取舍：RFC 未定义 DELETE body 语义，极少数代理/客户端可能丢弃；当前内网后台场景可接受，
  若未来出现兼容性问题再评估 `?ids=` 查询参数或回退 POST。

### 2.2 路径命名：URL path 段统一 kebab-case（2026-10-27）

依据 Zalando / Microsoft REST 指南与业界惯例（URL 大小写敏感、kebab 可读性最好），
`/api/v1/<域>/<资源>/<action>` 的**每一段都用小写连字符**：

- 资源段：`user-role-rels`、`image-captchas`（既有风格，不变）
- 动作段：`batch-create`、`batch-delete`、`soft-delete`、`delete-by-job`、`show-image`、
  `check-cookie`、`user-agent`（2026-10-27 由 snake 统一为 kebab）
- 例外：comfyui 域是 ComfyUI 上游原生 API 镜像（`object_info`、`upload_image`、`clear_queue` 等），
  **保持原样不改**，以便与上游行为逐字对齐。
- JSON body 字段仍是 snake_case（proto 字段名），与 path 段的 kebab 互不影响。

> 结论：路由与 proto `google.api.http` 注解一律按此约定书写——
> `enabled` = patch、`batch_create`/`batch_delete` = `delete` 协议 + kebab 路径；
> 改动任一侧必须同步另一侧并重新生成 TS 类型（`crates/grpc-proto/scripts/gen_docs.sh all`）。

---

## 三、gRPC 控制器规范

以 `services/template-service/src/controller/grpc.rs` 为样板。

1. 薄透传三步：`convert_grpc_req` 转 dto → 调 service → `Response::data(...).to_grpc_resp()`。
2. **service 有返回值必须透传到 data**，不要丢弃（本次修正了 create / delete / batch 系列丢弃返回值的问题）。
3. `to_grpc_resp` 走 serde JSON 桥（build.rs 已 `type_attribute(".", serde)`）：
   - Resp **含 `data` 标量字段**（如 `uint64 data`）时必须用 `Response::data(result)` 填值——
     `Response::<()>::ok()` 会产生 `data: null`，标量反序列化会失败。
   - Resp **无 `data` 字段**时才可用 `Response::<()>::ok()`。
4. lib.rs 需 re-export 生成 server：`pub use grpc_proto::nucleus::<域>::v1::<service>_server::<Service>Server;`

### 3.1 验证环节（已知缺口）

- 验证走 `validator` 特征：dto `#[derive(Validate)]` + 提取器 `T: Validate` bound 统一触发
  （`crates/axum/axum-validator`，Json/Query/Path 在提取时调 `.validate()`）。
- **各类型验证逻辑差异由各自 `Validate` impl 表达，验证环节统一收敛在提取器**——不要把校验内联散写到
  handler / service；需要查库的异步校验属于 service 业务校验，不算参数验证。
- **缺口**：gRPC 路径 `convert_grpc_req`（`axum-response/src/grpc.rs`）只做 JSON 转换、未调 `.validate()`，
  dto 上声明的规则只在 REST 生效。修复方向（待落地）：

```rust
pub fn convert_grpc_req<S, T>(req: &S) -> Result<T, Status>
where
    T: DeserializeOwned + Validate, // 补约束
{
    let target: T = /* 现有转换 */;
    target.validate().map_err(|e| Status::invalid_argument(e.to_string()))?;
    Ok(target)
}
```

  proto 生成类型**无需**加 `Validate`（convert 的输出是内部 dto，已 derive）。

---

## 四、dto 规范

| | 旧写法（system-service） | 新写法（template-service） |
| --- | --- | --- |
| 读模型 | 直接复用 `entity::Model`（`#[serde(flatten)] model: config::Model`、`GenericTree<config::Model>`） | 独立结构体 + `From<tpl_template::Model>` |
| 字段泄漏 | `deleted` / 关联字段 / 内部时间格式直接暴露 | 按"客户端需要看什么"裁剪 |
| 枚举 | dto 侧 `crate::enums` 重复定义 | 以 entity 侧为唯一事实源 |
| 转换位置 | 无（透传到 controller） | service 层完成 `Model → dto` |

规则：

1. dto 是对外契约结构体，**必须独立定义**，与 sea_orm 实体解耦（`From<entity::Model>` 单向转换）。
2. 读模型不含软删标记 `deleted`（存储层细节）、不含 belongs_to 关联字段。
3. 创建入参不复用读模型（服务端生成的 id / 唯一键 / 时间戳不在写模型上）。
4. 枚举去重：删除 dto 侧重复定义，直接引用 entity 的枚举。

---

## 五、entity 与 dao 规范

### 5.1 entity

| | 旧（system/config.rs） | 新（template/tpl_template.rs） |
| --- | --- | --- |
| 定义 | 手写 derives + Relation 枚举 + Related 实现 | `#[sea_orm::model]` dense 宏生成 |
| 字段元数据 | 无 | 字段级 `comment` / `default_value` |
| 软删 | 无 `deleted` 字段 | `deleted: bool`（**按需**，见下方说明） + `belongs_to` 关联 |

> **`deleted` 不是所有表都必须有**：软删只适合"业务对象可恢复、删除需要留痕"的主数据表
> （如 `t_tpl_template`、`t_schedule_config`）。日志 / 事件类 append-only 表
> （如 `t_schedule_log`、审计日志）生命周期就是"写入 → 查询 → 物理清理"，**不加 `deleted`**，
> 删除走物理 `delete`。判断标准：这条记录有没有"下线后还要恢复"的业务诉求。

### 5.2 dao（正式写法）

- `#[injectable]` + `PrimaryDb` 注入。
- 查询：`database::Pagination` 分页、`apply_if` 条件过滤、`count` + `offset/limit`。
- 命名统一：`all / list / info / create / batch_create / update / update_enabled / delete / soft_delete / batch_delete`。
- 错误处理：dao 层直接返回 `DbErr`，`map_err + error! + Error::DbXxx` 收口在 service 层。
- 软删（仅带 `deleted` 字段的表）：查询统一加 `deleted.eq(false)` 过滤；
  无软删字段的表（日志类）直接物理 `delete`，dao 命名用 `delete` / `delete_by_xxx`。

### 5.3 示例代码隔离

`template-service` 的 `dao/template_etx.rs`（paginate/into_json/save/set_from_json）与
`dao/template_sql_etx.rs`（raw SQL）是"仅示例、不参与业务"，但被 `lib.rs` re-export——
**待办：从公共导出移除或挪到 examples**，避免后续服务误当模板抄。

---

## 六、service 层规范

| | 旧（system-service） | 新（template-service） |
| --- | --- | --- |
| 返回类型 | `anyhow::Result` | `Result<_, err_code::Error>` |
| 返回值 | 透传 `entity::Model` | 返回 dto |
| 错误处理 | — | `map_err + error! + Error::DbXxx` |

- 错误码统一走 `err_code::Error`，可被 REST / gRPC 响应层翻译成业务码。
- **非测试代码禁止 `anyhow::Result`**（comfyui-service 已按此整改）：service / dao / controller
  签名一律 `Result<T, err_code::Error>`；`anyhow` 仅允许出现在 `[dev-dependencies]` 供测试使用。

### 6.1 service 层返回原始语义，`XxxResp` 在 controller 组装（2026-10-27 补充）

**规则**：service 返回**原始语义**——dto 条目、`(Vec<Item>, u64)`、`u64` 条数等，**不拼装 `XxxResp`**；
`XxxResp` 是接口层契约（定义保留在 dto，不删除），由 **controller** 负责组装并用
`Response` 系列函数收口：

| 返回语义 | service 返回值 | controller 收口（REST） | controller 收口（gRPC） |
| --- | --- | --- | --- |
| 分页列表 | `(Vec<Item>, u64)` | `Response::data_list(items, total).to_json()?` | `Response::data_list(results, total).to_grpc_resp()` |
| 单实体 | `dto Item` | `Response::data(GetXxxResp { data: result }).to_json()?` | `Response::data(result).to_grpc_resp()`（proto `data: Entity` 直挂） |
| 条数 / 标量 | `u64` 等 | `Response::data(DeleteXxxResp { data: result }).to_json()?` | `Response::data(result).to_grpc_resp()` |
| 无数据操作（含日志类 create） | `()` | `Response::ok()` | `Response::<()>::ok().to_grpc_resp()` |

**理由**：

1. **service 纯粹可复用**：service 不携带 `XxxResp`，其他 service / 定时任务 / CLI 直接消费
   条目、列表与条数，无需拆包；Resp 本质是"接口返回"，不属于业务层。
2. **dto `XxxResp` 是接口契约**：定义保留、线格式不变；只是组装动作从 service 挪到 controller，
   service 层不再出现任何 Resp 字面量。
3. **gRPC 走 proto Resp JSON 桥**：gRPC 控制器消费 service 原始返回值，
   `Response::data(原始值)` 的线形态与 proto `data 语义写实`（`data: Entity` / `data: uint64`）直接对齐。

**注意（to_json 类型桥 + serde flatten）**：`Responder<GetXxxResp>` 下 `to_json()` 的目标是信封类型
`Response<GetXxxResp>`，裸条目 `Response::data(result)` 要求 `GetXxxResp` 与 `data` 载荷同构。
做法：单实体包装 Resp 的 `data` 字段加 **`#[serde(flatten)]`** 平铺，controller 即可直接写
`Response::data(result).to_json()?`（wire `data:{...item...}`，与 proto `data: Entity` 直挂一致）。
**限制**：flatten 只支持 map 形态——标量包装（如 `DeleteXxxResp { data: u64 }`）无法 flatten，
delete 仍须显式组装 `Response::data(DeleteXxxResp { data: result })`。
列表不受影响——`GetXxxResp { data_list, total }` 与 `ListData` 同构。

**约束**（沿用既有约定）：

- 列表 REST 线格式保持 `data_list` / `total` 挂在 `data` 载荷上（前端依赖，勿改动）。
- 含标量 `data` 字段的 Resp 必须用 `Response::data` 填值，`ok()` 的 `data: null` 会导致反序列化失败（见第三节）。
- 多字段复合结果（如 `BatchCreateDocumentResp { success_count, fail_count, failures }`、
  `ProfileResp`、`CreateImageCaptchaResp`）属于领域读模型 / 结果结构，保留在 service 组装，不拆。

**已迁移（2026-10-27）**：audit-service 四个日志 service（list/info/create/delete 全部返回原始语义，
Resp 组装移至 controller）；template-service `info`；rag-service `batch_retry_failed`
（service 返回 `usize`，controller 组装 `BatchRetryResp`）。

---

## 七、system-service 迁移清单（按优先级）

1. **proto 对齐**（低风险，纯声明）：`config.proto` 等逐文件按第一、二节 checklist 过
   （已知：proto `code`/`data_type` vs dto `key`/`type`；`value`/`desc` 非 optional vs dto `Option<String>`）。
2. **补 gRPC controller**：照 template `controller/grpc.rs` 抄结构（现为空壳）。
3. **dto 读模型化**：8 个 dto 独立结构体 + `From<Model>`，service 层完成转换，剔除 entity Model 透传。
4. **service 错误收敛**：`anyhow::Result` → `Result<_, Error>`。
5. **（大动作，单独立项）**：system 实体迁 `#[sea_orm::model]` dense 格式 + `deleted` 软删 + dao 软删过滤——
   涉及 schema migration 与存量数据，不与其他项混做。
6. **顺手项（template-service）**：
   - 移除 etx 两个示例 dao 的 re-export；
   - 修 `service/template.rs::create` 未 `Set(data.enabled)` 的存量 bug（batch_create 有设置，行为不一致）。

---

## 八、通用 Checklist（优化任一服务 proto / 服务层时逐项过）

- [ ] 删除通用 Resp，Req/Resp 成对，字段号 `code=1 / message=2 / data=3`
- [ ] 响应 data 语义写实（实体 / 条数 / 无）
- [ ] 逐字段**行注释（`//`）**补齐，禁止块注释（ts-proto 生成 TS 只保留行注释），与 dto 注释对齐
- [ ] optional / Option、整数宽度与 dto 类型对齐
- [ ] 读模型 / 写模型分离，Create 不收服务端生成字段
- [ ] 每个 rpc 带 google.api.http 注解，路径 / HTTP 方法与 REST 路由核对消除漂移
- [ ] gRPC 控制器薄透传，service 返回值进 data；含标量 data 的 Resp 用 `Response::data`
- [ ] dto 独立读模型 + `From<Model>`，不透传 entity Model，不泄漏 deleted/关联字段
- [ ] dao 正式写法（Pagination/apply_if/命名统一），示例 dao 不在业务导出中
- [ ] service 返回 `Result<_, err_code::Error>`，map_err 收口
- [ ] 枚举以 entity 为唯一事实源，dto 侧不重复定义
- [ ] 改完跑 `cargo check -p <service>`（grpc-proto build.rs 自动重生成）

---

## 九、枚举字段的 proto/dto/entity 三层实践（2026-10-26 补充）

以 `t_tpl_template` 新增 `status`（数字枚举）/ `level`（字符串枚举）示例字段为样板。

### 9.1 entity 侧：两种枚举写法

- **数字枚举**：`rs_type = "i16"` + `db_type = "TinyInteger"`，**必须 `#[repr(i16)]`**；
  serde 走 `serde_repr::Serialize_repr / Deserialize_repr`（JSON 线格式为数字判别值），
  **不能**与 `serde::Serialize/Deserialize` derive 共存（重复 impl）。
  entity crate 需加 `serde_repr = { workspace = true }`（workspace 已有 0.1）。
- **字符串枚举**：`rs_type = "String"` + `db_type = "String(StringLen::N(32))"`，
  变体 `#[sea_orm(string_value = "...")]`，serde 走普通 derive + `#[serde(rename_all = "snake_case")]`，
  JSON 值与 DB 存储值天然同形。
- 两种枚举都建议：`#[derive(Default)]` + 首变体 `#[default]` 指定缺省；
  Model 字段上 `#[sea_orm(default_value = 0 / "basic")]`（宏支持，保证 entity-first 建表也有 DB 默认值）。
- 枚举定义在 entity 文件内 `pub mod enums`（沿用 `system/config.rs` 风格），域 `mod.rs` re-export。

### 9.2 proto 侧：用扁平字段，不用 proto enum

**结论：proto 侧用 `int32`/`string` 扁平字段 + 注释标注取值，不定义 proto enum。**

原因——`to_grpc_resp` / `convert_grpc_req` 的 JSON 桥：

```text
dto ──serde①──▶ serde_json::Value ──serde②──▶ prost 类型
```

1. 桥是"两个互不知晓的 serde schema 借无 schema 的 Value 中转"，靠两端表示约定逐字段同形维系。
2. prost 枚举的 serde 是默认 derive（proto3 JSON 规范），枚举序列化为**变体名字符串**；
   proto 的数字判别值只存在于 encode/decode 二进制层，serde 层看不到。
3. dto 数字枚举（serde_repr）产出 `1`，prost 枚举要 `"Active"`——JSON 树对不上，双向都断。
4. 字段名（蛇形）和 i64/int32（数字）能直通，是因为 prost 选择了 Rust 习惯的 serde 形态，
   恰好与 dto 一致；枚举是唯一两端约定来源不同且无法自动同形的维度。

三种修复姿势与取舍：

| 姿势 | 说明 | 取舍 |
| --- | --- | --- |
| proto 扁平字段（现状） | `int32`/`string` + 注释标取值 | ✅ 零转换；❌ 取值不内建于契约 |
| 枚举字段手写映射 | proto enum + `From/TryFrom` match（4 行/枚举） | ✅ 编译期安全；每枚举一次成本 |
| pbjson-build / prost-reflect | 按 proto3 JSON 规范重生成 serde（枚举名/数字双收） | ❌ 全仓 JSON 形态变（驼峰、int64→字符串），dto 侧连带重构，需专项 |

什么时候值得引入 proto enum：出现真正的跨语言 gRPC 客户端（Go/TS 直接消费强类型枚举）时，
配合姿势②做专项；纯内部 Rust 调用 + JSON 桥场景，扁平字段最稳。

### 9.3 为什么 TS 侧没有这个问题（佐证）

`web/packages/proto-types` 由 ts-proto 从 descriptor **单源生成唯一类型**，
枚举表示法由生成器一次性决定，序列化/反序列化同源——不存在第二套手写类型，
自然没有"双 schema 桥对齐"问题。这反证了 Rust 侧冲突的根源是**手写 dto 与生成类型并行的双 schema 架构**，
对齐成本只能靠纪律（本文档 checklist）压制。

### 9.4 新增枚举字段的操作清单（完整链路）

1. entity：`pub mod enums` 定义枚举（按 9.1 两种写法之一）+ Model 字段（带 `default_value`）+ 域 `mod.rs` re-export。
2. migration crate：`migration/{域}/m{日期}_{序号}_{描述}.rs`，sea_query `add_column_if_not_exists`（带 DB 默认值）+ 可回滚 down；注册进 `migration/mod.rs`（按域子目录组织）。
3. migration-sql crate：同名文件，三后端裸 SQL（MySQL `TINYINT` / PG `SMALLINT` / SQLite `INTEGER`，注意 PG 用 `SMALLINT` 没有 TINYINT；注释语法各后端不同）；注册进 `migration/mod.rs`。
4. dto：读模型 + 写模型（Create/BatchItem/Update）加字段，写模型字段 `#[serde(default)]` 兜底缺省值。
5. service：`create` / `batch_create` / `update` 显式 `Set` 新字段。
6. proto：实体消息与写请求消息加字段（**新增字段号顺延，wire 兼容**），注释标注取值范围。
7. 验证：`cargo check -p entity -p migration -p migration-sql -p <service>`；存量环境需跑迁移。

已知坑：

- serde_repr 要求 `#[repr]`，且不能与 serde 的 Serialize/Deserialize derive 叠加；
- prost 生成类型的 Iden 列引用用 `tpl_template::Column`（`Entity` 别名上没有列常量）；
- 存量 `gRPC 控制器` 若后续引入 proto enum，`convert_grpc_req`/`to_grpc_resp` 的直通对该字段失效，需在控制器层补映射（另注意 `convert_grpc_req` 尚未调 `.validate()` 的验证缺口，动它时一并规划）。

---

## 十、实体与迁移的最新写法（参考 template 示例）

> 最新写法 = SeaORM 2.0 **dense 实体宏** + **entity-first 迁移**（实体属性为单一真相源）。
> 唯一样本：`crates/core/entity/src/template/tpl_template.rs` 及其配套迁移。

### 10.1 Entity：`#[sea_orm::model]` dense 格式

对照旧写法（`system/config.rs`：手写 derives + 手写 `Relation` 枚举 + `impl Related`），
dense 格式由宏生成 Relation/Related，字段元数据内联在字段上：

```rust
//! 应用模板表（2.0 dense 新格式示例）
use chrono::Local;
use sea_orm::Set;
use sea_orm::entity::prelude::*;
use sea_orm::prelude::async_trait::async_trait;
use serde::{Deserialize, Serialize};
use self::enums::{TemplateLevel, TemplateStatus}; // 同文件内 enums 模块
use crate::user::user_base;

#[sea_orm::model]                                  // ← dense 宏：生成 Relation/Related/Column
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel)]
#[sea_orm(table_name = "t_tpl_template")]          // 表名必须 t_ 前缀
pub struct Model {
    /// 模板 ID（主键）
    #[sea_orm(primary_key, auto_increment = true, comment = "模板ID")]
    pub id: i64,
    /// 用户 ID
    #[sea_orm(comment = "用户ID", indexed)]        // indexed → 自动普通索引
    pub user_id: i64,
    /// 模板唯一键
    #[sea_orm(column_type = "String(StringLen::N(64))", unique, comment = "模板唯一键")]
    pub template_key: String,                      // unique → 自动唯一索引
    /// 描述信息（可空字段 = Option）
    #[sea_orm(column_type = "String(StringLen::N(200))", comment = "描述信息")]
    pub desc: Option<String>,
    /// 是否启用
    #[sea_orm(default_value = true, comment = "是否启用")]
    pub enabled: bool,
    /// 是否删除（软删除标记）
    #[sea_orm(default_value = false, comment = "是否删除")]
    pub deleted: bool,
    /// 创建时间
    #[sea_orm(comment = "创建时间")]
    pub created_at: DateTime,
    /// 更新时间
    #[sea_orm(comment = "更新时间")]
    pub updated_at: DateTime,
    /// 归属用户（belongs_to 关系，Relation/Related 由宏生成）
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_update = "Cascade", on_delete = "Cascade")]
    pub user: BelongsTo<user_base::Entity>,
}
```

规范要点：

1. **元数据全部内联字段上**：`comment`（列注释）、`default_value`（DB 默认值）、
   `indexed` / `unique`（索引）、`column_type`（长度与类型）——这些是 entity-first 迁移的 DDL 来源。
2. **软删（按需）**：主数据表加 `deleted: bool` 默认 false，查询侧统一过滤（见第五节 dao 规范）；
   日志 / 事件类 append-only 表不加（如 `t_schedule_log`），删除走物理删除。
3. **时间戳**：`ActiveModelBehavior::before_save` 自动填充；**批量 `insert_many` 不经过该钩子**，
   必须提供并调用 `ensure_timestamps()`（template 示例完整实现，照抄即可）。
4. **枚举**：定义在同文件 `pub mod enums`（写法见第九节 9.1），域 `mod.rs` re-export：
   `pub use tpl_template::enums::{TemplateLevel, TemplateStatus};`
5. 域 `mod.rs` 的 Entity 别名：`pub use tpl_template::Entity as Template;`

### 10.2 建表迁移：entity-first（schema 层）

表结构由实体属性派生，**不再手写 `DeriveIden` 枚举与逐列 `Table::create()`**：

```rust
//! crates/core/migration/src/schema/{域}/{表名}.rs
use entity::template::Template;
use sea_orm::sea_query::Table;
use sea_orm_migration::prelude::*;
use crate::utils::if_not_exists_create_index;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // 实体优先：列类型/默认值/唯一约束/索引/注释均由实体属性派生
        db.get_schema_builder().register(Template).apply(db).await?;
        // 实体属性覆盖不到的（如联合索引、前缀索引）再手动补
        if_not_exists_create_index(manager, Template, vec![Template::COLUMN.user_id]).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Template).if_exists().to_owned()).await
    }
}
```

- 注意建表迁移用 **Entity 别名**（`Template::COLUMN.user_id`），
  版本化迁移因无别名引用时用 **`{表}::Column`**（见 10.3）。
- `utils::if_not_exists_create_index` / `if_not_exists_create_unique_index` 是现成工具，勿重复造。

### 10.3 版本化迁移：`migration/{域}/m{日期}_{序号}_{描述}.rs`

表结构变更（加列/加索引/数据迁移）走 migration 层，**只增不减**：

```rust
//! crates/core/migration/src/migration/template/m20261026_0001_tpl_template_add_enum_columns.rs
use entity::template::tpl_template::{Column, Entity as Template};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.alter_table(
            Table::alter()
                .table(Template)
                .add_column_if_not_exists(ColumnDef::new(Column::Status).tiny_integer().not_null().default(0))
                .add_column_if_not_exists(ColumnDef::new(Column::Level).string_len(32).not_null().default("basic"))
                .to_owned(),
        ).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.alter_table(
            Table::alter().table(Template).drop_column(Column::Level).drop_column(Column::Status).to_owned(),
        ).await
    }
}
```

- Iden 引用：建表迁移用 Entity 别名（`Template::COLUMN.xxx`）；
  版本化迁移同文件内 `use entity::template::tpl_template::{Column, Entity as Template}` 两个都要。
- 列操作尽量 `add_column_if_not_exists` / `if_exists` 幂等化；**必须提供可回滚的 down**。

### 10.4 migration-sql（裸 SQL 镜像）：同名文件、三后端

`crates/core/migration-sql` 与 migration crate **文件名一一对应**，内容为三后端裸 SQL：

```rust
async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    let db = manager.get_connection();
    match manager.get_database_backend() {
        DatabaseBackend::MySql => db.execute_unprepared("
            ALTER TABLE `t_tpl_template`
                ADD COLUMN `status` TINYINT NOT NULL DEFAULT 0 COMMENT '模板状态',
                ADD COLUMN `level` VARCHAR(32) NOT NULL DEFAULT 'basic' COMMENT '模板级别';").await?,
        DatabaseBackend::Postgres => db.execute_unprepared(r#"
            ALTER TABLE t_tpl_template
                ADD COLUMN IF NOT EXISTS "status" SMALLINT NOT NULL DEFAULT 0,
                ADD COLUMN IF NOT EXISTS "level" VARCHAR(32) NOT NULL DEFAULT 'basic';
            COMMENT ON COLUMN t_tpl_template.status IS '模板状态';"#).await?,
        _ => db.execute_unprepared("
            ALTER TABLE `t_tpl_template`
                ADD COLUMN `status` INTEGER NOT NULL DEFAULT 0, -- 模板状态
                ADD COLUMN `level` TEXT NOT NULL DEFAULT 'basic'; -- 模板级别").await?,
    }
    Ok(())
}
```

三后端差异速查：MySQL `TINYINT` / PG `SMALLINT`（无 TINYINT）/ SQLite `INTEGER`；
字符串 PG 用 `VARCHAR(n)`、SQLite 用 `TEXT`；注释语法 MySQL 行内 `COMMENT`、PG `COMMENT ON`、SQLite 行内 `--`。

### 10.5 注册链路（三层顺序不可乱）

```rust
// migration/src/lib.rs 与 migration-sql/src/lib.rs（MigratorTrait）
m.extend(schema::migrations());     // 【1】建表
m.extend(seed::migrations());       // 【2】初始数据
m.extend(migration::migrations());  // 【3】版本化变更
```

- schema 层与 migration 层各自 `mod.rs` 维护 `migrations()`；migration 按域子目录（`pub mod template;`）。
- 新表：schema/{域}/ 加建表文件；改表：migration/{域}/ 加版本化文件——**不要改已发布的 schema 文件**。
- 命名：schema 文件与表名一致；migration `m{YYYYMMDD}_{序号}_{描述}.rs`；seed `s_{表名}.rs`。

### 10.6 迁移清单（entity/migration 相关）

- [ ] 实体为 dense `#[sea_orm::model]` 格式，元数据（comment/default_value/indexed/unique/column_type）内联字段上
- [ ] 按需的 `deleted` 软删字段（主数据表才加，日志类不加）+ `before_save` 时间戳 + `ensure_timestamps`（批量写用）
- [ ] 枚举在同文件 `pub mod enums`，域 mod.rs re-export
- [ ] 建表迁移 entity-first（`get_schema_builder().register(Entity).apply(db)`），不手写 DeriveIden
- [ ] 版本化迁移按域子目录 + 幂等 + 可回滚 down
- [ ] migration-sql 同名镜像文件，三后端 SQL 逐一核对类型映射
- [ ] 三层注册顺序 schema → seed → migration

---

## 十一、user 模块批量迁移实战复盘（2026-10-26）

> 以 user 域六实体 + user-service + proto/v1/user 全量迁移为样本，记录本次批量整改实际踩到的坑。
> 后续迁移其他域（system / permission / organization ...）时逐条对照，避免重踩。

### 11.1 dense 宏的隐藏约束

1. **`BelongsTo<T>` 要求目标实体也是 dense**：dense 宏为关联生成 `EntityLoader` / `ActiveModelEx` 等
   附属类型，若 `T` 还是旧格式（无 `#[sea_orm::model]`）直接编译失败
   （`cannot find type EntityLoader in module member_level`）。
   → 迁移带外键的实体时，**先确认外键目标实体已迁移**，否则连带扩大范围
   （本次 `user_member_ref` → `system/member_level.rs` 被迫一并迁移）。
2. **typed `Xxx::COLUMN.xxx` 别名会收紧过滤输入类型**：`COLUMN.created_at.gte("2024-01-01")`
   （String）不再编译（`String: DateTimeLikeValue` 未实现）。
   → dto Req 的时间过滤字段必须同步从 `Option<String>` 切到
   `#[serde(default)] Option<NaiveDateTime>`——template 一直这么写不是偏好而是配套约束。
   迁移清单里"列引用切 COLUMN 别名"必须和"dto 时间字段类型化"**成对完成**。
3. **`if_not_exists_create_index` 是同型泛型 `Vec<C>`**：
   - 复合索引两列 typed 类型不同（如 `BigIntColumn` + `BoolColumn`）→ E0308；
   - `into_iden()` 统一成 `DynIden` 也不行（`DynIden: sea_orm::Iden` 不满足工具 bound）；
   - ✅ 正解：dense 宏**仍会生成同型的无类型 `Column` 枚举**，复合索引传
     `user_xxx::Column::A, user_xxx::Column::B` 即可走工具函数。
   - 单列索引可继续用 `Xxx::COLUMN.col`。

### 11.2 entity-first 的元数据搬运责任

- 旧手写 DDL 里的 **column_type（长度）/ default_value / unique / indexed** 必须逐列精确搬进
  实体属性——entity-first 后它们是新建库 DDL 的唯一来源，漏搬 = 新老环境 schema 漂移
  （如 username 漏 N(32) 会建成 varchar(255)）。
- 搬运时的唯一可信来源是**旧 schema 迁移文件**（不是 migration-sql 的裸 SQL，两者已有出入，
  如 SQL 里 `id INT` vs 迁移里 `big_integer`，以 sea_query 迁移为准）。

### 11.3 枚举下沉 entity 的波及面

- `Model.字段` 从 `i16` 改为枚举后，**所有 `Set(数字)` / `Set(x as i16)` 写法全部编译失败**，
  包括最不起眼的测试代码和示例 dao（本次共 4 处：initialize-service、template-service 的
  `template.rs` / `template_etx.rs`）。
- 修复姿势：`Set(0)` → `Set(Gender::default())`；`Set(req.gender as i16)` → `Set(req.gender)`。
- JSON 线格式不变的前提是枚举走 `serde_repr`（数字判别值）——先确认再替换，否则 REST 契约静默变更。
- 验证手段：`grep -rn "gender: Set("` 逐个清零；改 entity 后必须 `cargo check --workspace`
  （示例 dao、initialize 等隐性依赖只有全仓编译才暴露）。

### 11.4 schema / seed / 版本化迁移的职责边界

- **schema 只建表**：预设数据放 `seed/s_{表名}.rs`；schema 文件里夹种子数据是旧债，迁移时一并剥离。
- **seed 用单条 `insert`**（走 `before_save` 自动填时间戳），不要 `insert_many`（不过钩子，
  Postgres/SQLite 下时间列无 DB 默认值会失败）；seed 的 `down` 按业务键 delete_many，可回滚。
- **migration-sql 建表 SQL 补列时**：三后端类型映射 MySQL `TINYINT(1)` / PG `BOOLEAN`（无 TINYINT）/
  SQLite `INTEGER`；PG 记得补 `COMMENT ON COLUMN`。
- **已知重叠**：schema 建表 SQL 里补了 `deleted` 后，全新环境 schema（有列）→ 版本化迁移
  `ADD COLUMN`（Rust migration 用 `add_column_if_not_exists` 幂等无碍；但 MySQL/SQLite 的
  migration-sql 镜像是裸 `ADD COLUMN`，同链重放会报重复列）。二选一：
  schema SQL 补齐 + 版本化迁移只服务存量环境，或 schema SQL 不补列统一靠版本化迁移。
  当前项目现状：migration-sql 建表 SQL 已补 `deleted`（user_base），其余五表待补。

### 11.5 proto 对齐的三个断点（JSON 桥视角）

桥是"两个互不知晓的 serde schema 借无 schema 的 Value 中转"，逐字段同形是唯一保障：

1. **实体消息的可空字段也要 `optional`**：dto 读模型 `Option<String>` 序列化出 `null`，
   prost 非 optional `string` 反序列化 `null` 直接失败。本次把 6 个实体消息的可空字段全部切
   `optional`（此前只有 Req 侧是 optional，Resp 实体消息侧漏了）。
2. **Decimal ≠ double**：rust_decimal serde JSON 形态是**字符串**，proto `double` 双向都断。
   契约用 `optional string` 承载（location 的经纬度已改），注释标注"Decimal 字符串形态"。
3. **wire 兼容性前提要写明**：int32→int64、`double`→`string`、路径 `/v1`→`/api/v1`、
   `walets` 拼写修正、`enabled` put→patch——这些破坏性变更之所以能做，是因为
   user 域 gRPC controller 为空壳、无已部署客户端。**迁移前先 grep 生成类型消费方**
   （Rust：`grpc_proto::nucleus::<域>`；TS：`proto-types` 生成物与 import），确认无消费再动。

### 11.6 session 迁出 user 域的处理清单

- 实体/表已迁 `t_sys_session`（system-service），但 proto 留在 user 域形成"契约悬空"。
- 处理：删除 `proto/v1/user/session.proto` + `service.proto` 移除对应 RPC/import，
  service.proto 头部注释声明归属；TS 重新生成（59→58 文件）自动移除 session.ts。
- 遗留：system-service 若需 gRPC 契约，在 `proto/v1/system/` 新建（package `nucleus.system.v1`），
  勿复用旧 `nucleus.user.v1::UserSession`。

### 11.7 批量整改的操作纪律

- **先改 entity → 立即 `cargo check --workspace`**，把枚举/字段类型变更的波及点一次收干净，
  不要等服务层改完才发现下游编译失败。
- **批量文本替换后必须重编译**：sed/perl 替换 `Column::` → `COLUMN.` 时字段名大小写
  （PascalCase → snake_case）会引入新错误，typed 列是"编译期校验的字典"。
- **文件可能被并发编辑**（IDE 在改）：replace 失败先重读文件再重试，不要凭记忆拼 old_str。
- 每完成一层（entity / migration / service / proto）跑一次 `cargo fmt` 收尾。
