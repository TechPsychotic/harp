#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("source: {0}")]
    Source(String),
}

pub type Result<T> = std::result::Result<T, Error>;
