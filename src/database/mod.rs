mod context;
mod event;
mod handler;
mod versions;

pub type DBResult<T> = sqlx::Result<T>;

pub use event::DatabaseHelper;
pub use handler::DatabaseHandle;
