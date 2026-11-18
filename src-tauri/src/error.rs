use serde::{Serialize, Serializer, ser::SerializeStruct};

/// command가 프론트엔드로 돌려주는 오류.
///
/// `{ "kind": "internal", "message": "..." }` 형태로 직렬화된다.
/// TS 쪽 짝은 `src/lib/ipc.ts`의 `AppError`.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum AppError {
    #[error(
        "{}",
        crate::i18n::pick("내부 작업이 실패했어요.", "Something went wrong inside kcal.")
    )]
    Internal,
    /// 입력 검증 실패. 메시지는 사용자에게 그대로 보여 줄 수 있는 문장이다.
    #[error("{0}")]
    InvalidInput(String),
}

impl AppError {
    fn kind(&self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::InvalidInput(_) => "invalid_input",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_kind_and_message() {
        let json = serde_json::to_value(AppError::Internal).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "internal",
                "message": "내부 작업이 실패했어요."
            })
        );
    }
}
