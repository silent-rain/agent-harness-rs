// 业务码
use std::io;

use serde::{Serialize, ser::Serializer};

/// 错误种类
#[derive(Debug, thiserror::Error)]
#[repr(u16)]
pub enum Error {
    #[error("ok")]
    Ok = 0,
    #[error("unknown error, {0}")]
    Unknown(String) = 10001,
    #[error("internal server error, {0}")]
    InternalServer(String),
    #[error("invalid request parameter, {0}")]
    InvalidParameter(String),
    #[error("Validate Errorr, {0}")]
    ValidateError(String) = 10106,
    #[error("request error, {0}")]
    RequestError(String),
    #[error("parse request body error, {0}")]
    RequestBodyError(String),
    #[error("json serialization error, {0}")]
    JsonSerialization(String) = 10151,
    #[error(transparent)]
    Io(io::Error),
    #[error("io error, no data available")]
    NoDataAvailable,

    // 数据库操作
    #[error("db query error: {0}")]
    DbQueryError(String),
    #[error("db query empty error: {0}")]
    DbQueryEmptyError(String),
    #[error("db add error: {0}")]
    DbAddError(String),
    #[error("db update error: {0}")]
    DbUpdateError(String),
    #[error("db delete error: {0}")]
    DbDeleteError(String),
    #[error("db data exist error: {0}")]
    DbDataExistError(String),
    #[error("db data exist children error: {0}")]
    DbDataExistChildrenError(String),

    // 其他
    #[error(transparent)]
    ColorEyreReport(#[from] color_eyre::Report),
    #[error(transparent)]
    Any(#[from] anyhow::Error) = 65535,
}

impl Error {
    /// 返回错误码
    pub fn code(&self) -> u16 {
        unsafe {
            let ptr = self as *const Error as *const u16;
            ptr.read_volatile()
        }
    }
    /// 返回错误码信息
    pub fn msg(&self) -> String {
        self.to_string()
    }
}

/// 业务码序列化
impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

/// IO 错误转换
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        if err.kind() == io::ErrorKind::UnexpectedEof {
            return Error::NoDataAvailable;
        }
        Error::Io(err)
    }
}

/// axum 响应体错误转换
impl From<Error> for axum_response::ResponseErr {
    fn from(e: Error) -> Self {
        axum_response::ResponseErr::new(e.code(), &e.to_string())
    }
}

/// tonic 状态码转换
impl From<Error> for tonic::Status {
    fn from(e: Error) -> Self {
        match e {
            Error::InvalidParameter(msg) => tonic::Status::invalid_argument(msg),
            Error::ValidateError(msg) => tonic::Status::invalid_argument(msg),
            Error::RequestError(msg) => tonic::Status::invalid_argument(msg),
            _ => tonic::Status::internal(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code() {
        let mut err = Error::Unknown("0".to_string());
        assert!(err.to_string() == "unknown error, 0");

        let code = unsafe {
            let mul_err = &mut err;
            let ptr: *const u16 = mul_err as *mut Error as *const u16;
            ptr.read_volatile()
        };
        assert!(code == 10001);
    }

    #[test]
    fn test_error_code2() {
        let err = Error::Unknown("0".to_string());
        let code = err.code();
        assert!(code == 10001);
    }
}
