# err-code

全局业务错误码枚举，为整个项目提供统一的错误分类与编号体系。

## 使用示例

```rust
use err_code::Error;

fn get_user(id: i32) -> Result<User, Error> {
    if id <= 0 {
        return Err(Error::InvalidParameter("id must > 0".into()));
    }
    // ...
}

// 获取错误码和消息
let err = Error::DbQueryEmptyError("user not found".to_string());
assert_eq!(err.code(), 10203);
assert_eq!(err.msg(), "db query empty error: user not found");
```
