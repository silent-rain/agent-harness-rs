# inject

基于 [nject](https://docs.rs/nject) 的依赖注入容器，提供全局服务注册与解析能力。

## 功能特性

- 声明式依赖注入（`#[injectable]` / `#[provider]`）
- 多数据库实例管理（`PrimaryDb` / `ReadDb`）

## 主要类型

| 类型                              | 说明                           |
| --------------------------------- | ------------------------------ |
| [`PrimaryDb`](./src/lib.rs)       | 主库连接包装类型（读写）       |
| [`ReadDb`](./src/lib.rs)          | 从库连接包装类型（只读）       |
| [`InjectProvider`](./src/lib.rs)  | 注入容器，持有多个数据库实例   |
| [`AInjectProvider`](./src/lib.rs) | `Arc<InjectProvider>` 类型别名 |

## 使用示例

```rust
use inject::{InjectProvider, injectable, PrimaryDb, ReadDb};

// 创建注入容器（主库 + 从库）
let provider = Arc::new(InjectProvider::new(primary_db, read_db));

// 在 Axum 中作为 Extension 注入
let app = Router::new()
    .route("/", get(handler))
    .layer(Extension(provider));
```
