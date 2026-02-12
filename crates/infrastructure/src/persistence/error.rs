//! Shared error mapping for sqlx persistence layer

use application::error::ApplicationError;

/// Map a sqlx error to an application-layer error
pub fn map_sqlx_error(e: sqlx::Error) -> ApplicationError {
    match e {
        sqlx::Error::RowNotFound => {
            ApplicationError::NotFound("Database record not found".to_string())
        },
        sqlx::Error::Database(db_err) => {
            ApplicationError::Internal(format!("Database error: {db_err}"))
        },
        other => ApplicationError::Internal(format!("Database error: {other}")),
    }
}
