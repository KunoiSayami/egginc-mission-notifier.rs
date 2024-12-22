use std::{hash::Hash, sync::LazyLock};

use chrono::DateTime;
use itertools::Itertools as _;
use sqlx::{prelude::FromRow, sqlite::SqliteRow, Row};
use teloxide::types::ChatId;

use crate::bot::replace_all;

pub static DEFAULT_NICKNAME: LazyLock<String> = LazyLock::new(|| "N/A".to_string());

pub fn timestamp_to_string(timestamp: i64) -> String {
    let time = DateTime::from_timestamp(timestamp, 0).unwrap();
    time.with_timezone(&chrono_tz::Asia::Taipei)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

#[derive(Clone, Debug)]
pub struct User {
    id: i64,
    accounts: Vec<String>,
}

impl User {
    /// Only use in add user
    pub fn new(id: i64, ei: String) -> Self {
        Self {
            id,
            accounts: vec![ei],
        }
    }

    /* pub fn chat_id(&self) -> ChatId {
        ChatId(self.id)
    } */

    pub fn accounts(&self) -> &[String] {
        &self.accounts
    }

    pub fn remove_account(&mut self, ei: String) -> bool {
        let begin = self.accounts.len();
        let v = std::mem::take(&mut self.accounts);
        self.accounts.extend(v.into_iter().filter(|x| x.eq(&ei)));
        self.accounts.len() != begin
    }

    pub fn add_account(&mut self, ei: String) -> bool {
        if self.accounts().iter().any(|s| s.eq(&ei)) {
            return false;
        }
        self.accounts.push(ei);
        true
    }

    pub fn account_to_str(&self) -> String {
        self.accounts().join(",")
    }
    pub fn id(&self) -> i64 {
        self.id
    }
}

impl FromRow<'_, SqliteRow> for User {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            accounts: {
                let row = row.try_get::<String, _>("accounts")?;
                if row.is_empty() {
                    vec![]
                } else {
                    row.split(",")
                        .map(std::string::ToString::to_string)
                        .collect()
                }
            },
        })
    }
}

#[derive(Clone, Debug)]
pub struct AccountMap {
    ei: String,
    users: Vec<i64>,
}

impl AccountMap {
    /// Only for add new ei
    pub fn new(ei: String, id: i64) -> Self {
        Self {
            ei,
            users: vec![id],
        }
    }

    pub fn chat_ids(&self) -> Vec<ChatId> {
        self.users.clone().into_iter().map(ChatId).collect()
    }

    pub fn users(&self) -> &[i64] {
        &self.users
    }

    pub fn remove_user(&mut self, id: i64) -> bool {
        let begin = self.users.len();
        let v = std::mem::take(&mut self.users);
        self.users.extend(v.into_iter().filter(|x| x.eq(&id)));
        self.users.len() != begin
    }

    pub fn add_user(&mut self, id: i64) -> bool {
        if self.users().iter().any(|s| s.eq(&id)) {
            return false;
        }
        self.users.push(id);
        true
    }

    pub fn user_to_str(&self) -> String {
        self.users().iter().map(|s| s.to_string()).join(",")
    }

    pub fn ei(&self) -> &str {
        &self.ei
    }
}

impl FromRow<'_, SqliteRow> for AccountMap {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            ei: row.try_get("ei")?,
            users: {
                let row = row.try_get::<String, _>("users")?;
                if row.is_empty() {
                    vec![]
                } else {
                    row.split(",")
                        .filter_map(|s| {
                            s.parse()
                                .inspect_err(|e| log::warn!("Parse {s:?} failure: {e:?}"))
                                .ok()
                        })
                        .collect()
                }
            },
        })
    }
}

#[derive(Clone, Debug, FromRow, Eq)]
pub struct Account {
    ei: String,
    nickname: Option<String>,
    last_fetch: i64,
    disabled: bool,
}

impl Account {
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

impl Hash for Account {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ei.hash(state);
    }
}

impl PartialEq for Account {
    fn eq(&self, other: &Self) -> bool {
        self.ei.eq(other.ei())
    }
}

impl std::fmt::Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} *{}* {} {}",
            self.ei,
            replace_all(self.name()),
            replace_all(&timestamp_to_string(self.last_fetch)),
            return_tf_emoji(!self.disabled)
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

pub fn return_tf_emoji(input: bool) -> &'static str {
    if input {
        "✅"
    } else {
        "❌"
    }
}
