use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    ops::Deref,
    sync::LazyLock,
};

use chrono::DateTime;
use itertools::Itertools as _;
use rand::distributions::{Alphanumeric, DistString as _};
use sqlx::{prelude::FromRow, sqlite::SqliteRow, Row};
use teloxide::types::ChatId;

use crate::{bot::replace_all, egg::types::ContractGradeSpec};

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
                                .inspect_err(|e| log::warn!("Parse user {s:?} failure: {e:?}"))
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
    contract_trace: bool,
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

    pub fn force_fetch(&self, current: i64) -> bool {
        self.contract_trace && current - self.last_fetch > 4 * 3600
    }

    pub fn contract_trace(&self) -> bool {
        self.contract_trace
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
            "{} *{}* {} {}{}",
            self.ei,
            replace_all(self.name()),
            replace_all(&timestamp_to_string(self.last_fetch)),
            return_tf_emoji(!self.disabled),
            self.contract_trace.then(|| " 📋").unwrap_or("")
        )
    }
}

#[derive(Clone, Debug, FromRow, Eq)]
pub struct SpaceShip {
    id: String,
    name: String,
    duration_type: i64,
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

    pub fn duration_type(&self) -> &str {
        Self::duration_type_to_str(self.duration_type)
    }

    pub fn duration_type_to_str(duration_type: i64) -> &'static str {
        match duration_type {
            0 => "Short",
            1 => "Long",
            2 => "Epic",
            3 => "Tutorial",
            _ => "Unknown",
        }
    }

    pub fn calc_time(&self, input: &DateTime<chrono::Utc>) -> String {
        if self.notified {
            return Default::default();
        }
        let time = DateTime::from_timestamp(self.land, 0).unwrap();
        fmt_time_delta(time - input)
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

    pub fn random(ei: String, land_time: i64) -> Self {
        Self {
            id: format!(
                "Faked_{}",
                Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
            ),
            name: "Faked".into(),
            duration_type: 4,
            belong: ei,
            land: land_time,
            notified: false,
        }
    }
}

impl PartialEq for SpaceShip {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Hash for SpaceShip {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

pub fn return_tf_emoji(input: bool) -> &'static str {
    input.then(|| "✅").unwrap_or("❌")
}

pub fn convert_set(v: Vec<HashSet<SpaceShip>>) -> Vec<SpaceShip> {
    v.into_iter()
        .reduce(|mut acc, x| {
            acc.extend(x.into_iter());
            acc
        })
        .map(|h| h.into_iter().collect_vec())
        .unwrap_or_default()
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

#[derive(Clone, Debug, FromRow)]
pub struct Contract {
    id: String,
    room: String,
    #[allow(unused)]
    belong: String,
    start_time: Option<f64>,
    finished: bool,
}

impl Contract {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn room(&self) -> &str {
        &self.room
    }

    /* pub fn belong(&self) -> &str {
        &self.belong
    } */

    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn start_time(&self) -> Option<f64> {
        self.start_time
    }
}

#[derive(Clone, Debug)]
pub struct ContractSpec {
    id: String,
    max_coop_size: i64,
    token_time: f64,
    spec: HashMap<crate::egg::proto::contract::PlayerGrade, ContractGradeSpec>,
}

impl ContractSpec {
    pub fn new(
        id: String,
        max_coop_size: i64,
        token_time: f64,
        spec: Vec<ContractGradeSpec>,
    ) -> Self {
        Self {
            id,
            max_coop_size,
            token_time,
            spec: spec.into_iter().map(|x| x.into_kv()).collect(),
        }
    }

    pub fn get_inner(&self) -> Vec<ContractGradeSpec> {
        self.spec.clone().into_values().collect_vec()
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn max_coop_size(&self) -> i64 {
        self.max_coop_size
    }

    pub fn token_time(&self) -> f64 {
        self.token_time
    }
}

impl Deref for ContractSpec {
    type Target = HashMap<crate::egg::proto::contract::PlayerGrade, ContractGradeSpec>;

    fn deref(&self) -> &Self::Target {
        &self.spec
    }
}

impl FromRow<'_, SqliteRow> for ContractSpec {
    fn from_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get(0)?,
            max_coop_size: row.try_get(1)?,
            token_time: row.try_get(2)?,
            spec: {
                let v: Vec<ContractGradeSpec> = minicbor_serde::from_slice(row.try_get(3)?)
                    .inspect_err(|e| log::error!("Deserialize CBOR data error: {e:?}"))
                    .unwrap();
                v.into_iter().map(|x| x.into_kv()).collect()
            },
        })
    }
}

#[allow(unused)]
#[derive(Clone, Debug, FromRow)]
pub struct ContractCache {
    id: String,
    room: String,
    body: Vec<u8>,
    timestamp: i64,
}

impl ContractCache {
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

impl From<ContractCache> for Vec<u8> {
    fn from(value: ContractCache) -> Self {
        value.body
    }
}

/* impl FromRow<'_, SqliteRow> for ContractCache {
    fn from_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get(0)?,
            room: row.try_get(1)?,
            body:
            ,
        })
    }
}
 */
