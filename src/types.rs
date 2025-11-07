use chrono::DateTime;

pub fn timestamp_to_string(timestamp: i64) -> String {
    timestamp_fmt(timestamp, "%Y-%m-%d %H:%M:%S")
}

pub fn timestamp_fmt(timestamp: i64, fmt: &str) -> String {
    let Some(time) = DateTime::from_timestamp(timestamp, 0) else {
        log::warn!("Invalid timestamp: {timestamp}");
        return "N/A".into();
    };
    time.with_timezone(&chrono_tz::Asia::Taipei)
        .format(fmt)
        .to_string()
}

pub const BASE64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD_NO_PAD;

pub fn return_tf_emoji(input: bool) -> &'static str {
    if input { "✅" } else { "❌" }
}

pub fn fmt_time_delta(delta: chrono::TimeDelta) -> String {
    let days = delta.num_days();
    let day_str = format!("{days} day{}, ", if days > 1 { "s" } else { "" });
    format!(
        "{}{:02}:{:02}:{:02}",
        if days > 0 { day_str.as_str() } else { "" },
        delta.num_hours() % 24,
        delta.num_minutes() % 60,
        delta.num_seconds() % 60,
    )
}
pub fn fmt_time_delta_short(delta: chrono::TimeDelta) -> String {
    if delta.num_seconds() < 0 {
        return "0h0m0s".into();
    }
    let days = delta.num_days();
    let day_str = format!("{days}d");
    format!(
        "{}{}h{}m{}s",
        if days > 0 { day_str.as_str() } else { "" },
        delta.num_hours() % 24,
        delta.num_minutes() % 60,
        delta.num_seconds() % 60,
    )
}

#[derive(Debug)]
pub enum QueryError {
    System(reqwest::Error),
    User(reqwest::Error),
    Other(anyhow::Error),
}

impl QueryError {
    pub fn is_user_error(&self) -> bool {
        matches!(self, Self::User(_))
    }

    pub fn is_system_error(&self) -> bool {
        matches!(self, Self::System(_))
    }

    pub fn err_type(&self) -> &str {
        match self {
            QueryError::System(_) => "system",
            QueryError::User(_) => "user",
            QueryError::Other(_) => "other",
        }
    }
}

impl From<QueryError> for anyhow::Error {
    fn from(value: QueryError) -> Self {
        match value {
            QueryError::System(error) | QueryError::User(error) => error.into(),
            QueryError::Other(error) => error,
        }
    }
}

impl From<anyhow::Error> for QueryError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(value)
    }
}
