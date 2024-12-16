use std::{hash::Hash, sync::LazyLock};

use chrono::DateTime;
use sqlx::prelude::FromRow;
use teloxide::types::ChatId;

pub static DEFAULT_NICKNAME: LazyLock<String> = LazyLock::new(|| "N/A".to_string());

pub fn timestamp_to_string(timestamp: i64) -> String {
    let time = DateTime::from_timestamp(timestamp, 0).unwrap();
    time.with_timezone(&chrono_tz::Asia::Taipei)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

#[derive(Clone, Debug, FromRow)]
pub struct Player {
    ei: String,
    user: i64,
    nickname: Option<String>,
    last_fetch: i64,
    disabled: bool,
}

impl Player {
    pub fn user(&self) -> i64 {
        self.user
    }

    pub fn chat_id(&self) -> ChatId {
        ChatId(self.user)
    }

    pub fn last_fetch(&self) -> i64 {
        self.last_fetch
    }

    pub fn disabled(&self) -> bool {
        self.disabled
    }

    pub fn nickname(&self) -> Option<&String> {
        self.nickname.as_ref()
    }

    pub fn name(&self) -> &String {
        self.nickname().unwrap_or(&*DEFAULT_NICKNAME)
    }

    pub fn ei(&self) -> &str {
        &self.ei
    }
}

impl Hash for Player {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ei.hash(state);
    }
}

impl PartialEq for Player {
    fn eq(&self, other: &Self) -> bool {
        self.ei.eq(other.ei())
    }
}

impl Eq for Player {}

impl std::fmt::Display for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            self.ei,
            self.name(),
            self.user,
            timestamp_to_string(self.last_fetch),
        )
    }
}

#[derive(Clone, Debug, FromRow)]
pub struct SpaceShip {
    id: String,
    name: String,
    belong: String,
    land: i64,
    notified: bool,
}

impl SpaceShip {
    pub fn belong(&self) -> &str {
        &self.belong
    }

    pub fn land(&self) -> i64 {
        self.land
    }

    pub fn notified(&self) -> bool {
        self.notified
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}
