mod context;
mod event;
mod handler;
pub mod types;
mod versions;

pub type DBResult<T> = sqlx::Result<T>;

pub use event::DatabaseHelper;
pub use handler::DatabaseHandle;
