use std::{collections::HashMap, fmt::Debug};

use futures_util::StreamExt as _;
use log::error;
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};

pub mod v1 {
    pub const VERSION: &str = "1";
}

pub mod v2 {
    use std::collections::HashMap;

    use futures_util::StreamExt;
    use itertools::Itertools as _;
    use log::info;
    use sqlx::SqliteConnection;

    use crate::types::Account;

    const MERGE_STATEMENT_STAGE: &str = r#"
        CREATE TABLE "account" (
            "ei"        TEXT NOT NULL,
            "nickname"  TEXT,
            "last_fetch"    INTEGER NOT NULL DEFAULT 0,
            "disabled"  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY("ei")
        );

        CREATE TABLE "user" (
            "id" INTEGER NOT NULL,
            "accounts" TEXT NOT NULL,
            PRIMARY KEY("id")
        );

        CREATE TABLE "account_map" (
            "ei" TEXT NOT NULL,
            "users" TEXT NOT NULL,
            PRIMARY KEY("ei")
        );
    "#;

    pub const VERSION: &str = "2";

    pub async fn merge_v1(conn: &mut SqliteConnection) -> sqlx::Result<()> {
        info!("Performing database prepare stage (v2)");
        sqlx::raw_sql(MERGE_STATEMENT_STAGE)
            .execute(&mut *conn)
            .await?;
        let accounts = sqlx::query_as::<_, Account>(
            r#"SELECT "disabled", "ei", "last_fetch", "nickname" FROM "player""#,
        )
        .fetch_all(&mut *conn)
        .await?;

        for account in accounts {
            sqlx::query(r#"INSERT INTO "account" VALUES (?, ?, ?, ?)"#)
                .bind(account.ei())
                .bind(account.nickname())
                .bind(account.last_fetch())
                .bind(account.disabled())
                .execute(&mut *conn)
                .await?;
        }

        let mut m = HashMap::new();
        let mut a = HashMap::new();
        info!("Query `players', merge into `account'");

        let mut querier =
            sqlx::query_as::<_, (String, i64)>(r#"SELECT "ei", "user" FROM "player""#)
                .fetch(&mut *conn);

        while let Some((ei, user)) = querier.next().await.transpose()? {
            m.entry(user).or_insert_with(Vec::new).push(ei.clone());
            a.entry(ei).or_insert_with(Vec::new).push(user);
        }

        drop(querier);

        info!("Merge users, total {} user", m.len());

        for (user, eis) in m {
            sqlx::query(r#"INSERT INTO "user" VALUES (?, ?)"#)
                .bind(user)
                .bind(eis.join(","))
                .execute(&mut *conn)
                .await?;
        }

        info!("Merge accounts, total {} account", a.len());
        for (ei, users) in a {
            sqlx::query(r#"INSERT INTO "account_map" VALUES (?, ?)"#)
                .bind(ei)
                .bind(users.into_iter().map(|s| s.to_string()).join(","))
                .execute(&mut *conn)
                .await?;
        }
        info!("Clean old database structure");

        sqlx::raw_sql(
            r#"DROP TABLE "player";
            UPDATE "meta" SET "value" = '2' WHERE "key" = 'version';"#,
        )
        .execute(&mut *conn)
        .await?;
        info!("Merge completed");

        Ok(())
    }
}

pub mod v3 {
    use log::info;
    use sqlx::SqliteConnection;

    pub const VERSION: &str = "3";

    const MERGE_STATEMENT_STAGE: &str = r#"
        CREATE TABLE "spaceship_1" (
            "id"        TEXT NOT NULL,
            "name"      TEXT NOT NULL,
            "duration_type"  INTEGER NOT NULL,
            "belong"    TEXT NOT NULL,
            "land"      INTEGER NOT NULL,
            "notified" INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY("id")
        );
    "#;

    fn convert_duration(s: &str) -> i64 {
        let dur = s.to_lowercase();
        match dur.as_str() {
            "epic" => 2,
            "long" => 1,
            "short" => 0,
            "tutorial" => 3,
            _ => 4,
        }
    }

    pub async fn merge_v2(conn: &mut SqliteConnection) -> sqlx::Result<()> {
        info!("Performing database prepare stage (v3)");
        sqlx::raw_sql(MERGE_STATEMENT_STAGE)
            .execute(&mut *conn)
            .await?;
        let spaceships = sqlx::query_as::<_, (String, String, String, i64, i64)>(
            r#"SELECT "id", "name", "belong", "land", "notified" FROM "spaceship""#,
        )
        .fetch_all(&mut *conn)
        .await?;

        info!("Merge spaceships, total {} spaceships", spaceships.len());
        for (id, name, belong, land, notified) in spaceships {
            let (duration, name) = name.split_once(' ').unwrap();
            sqlx::query(r#"INSERT INTO "spaceship_1" VALUES (?, ?, ?, ?, ?, ?)"#)
                .bind(id)
                .bind(name)
                .bind(convert_duration(duration))
                .bind(belong)
                .bind(land)
                .bind(notified)
                .execute(&mut *conn)
                .await?;
        }

        info!("Clean old database structure");

        sqlx::raw_sql(
            r#"DROP TABLE "spaceship";
            ALTER TABLE "spaceship_1" RENAME TO "spaceship";
            UPDATE "meta" SET "value" = '3' WHERE "key" = 'version';"#,
        )
        .execute(&mut *conn)
        .await?;
        info!("Merge completed");

        Ok(())
    }
}

pub mod v4 {
    use log::info;
    use sqlx::SqliteConnection;

    pub const VERSION: &str = "4";

    const MERGE_STATEMENT_STAGE: &str = r#"
        CREATE TABLE "account_1" (
            "ei"        TEXT NOT NULL,
            "nickname"  TEXT,
            "last_fetch"    INTEGER NOT NULL DEFAULT 0,
            "contract_trace" INTEGER NOT NULL DEFAULT 0,
            "disabled"  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY("ei")
        );

        CREATE TABLE "player_contract" (
            "id"	TEXT NOT NULL,
            "room"	TEXT NOT NULL,
            "belong"	TEXT NOT NULL,
            "start_time" REAL,
            "finished"	INTEGER NOT NULL,
            PRIMARY KEY("id", "belong")
        );

        CREATE TABLE "contract" (
            "id"	TEXT NOT NULL,
            "size"  INTEGER NOT NULL,
            "token_time" REAL NOT NULL,
            "body"	BLOB NOT NULL,
            PRIMARY KEY("id")
        );

        CREATE TABLE "contract_cache" (
            "id"	TEXT NOT NULL,
            "room"	TEXT NOT NULL,
            "body"	BLOB NOT NULL,
            "timestamp"	INTEGER NOT NULL,
            PRIMARY KEY("id")
        );

    "#;

    pub async fn merge_v3(conn: &mut SqliteConnection) -> sqlx::Result<()> {
        info!("Performing database prepare stage (v4)");
        sqlx::raw_sql(MERGE_STATEMENT_STAGE)
            .execute(&mut *conn)
            .await?;

        let accounts = sqlx::query_as::<_, (String, Option<String>, i64, bool)>(
            r#"SELECT "ei", "nickname", "last_fetch", "disabled" FROM "account""#,
        )
        .fetch_all(&mut *conn)
        .await?;

        info!("Merge account, total {} account", accounts.len());
        for (ei, nickname, last, disabled) in accounts {
            sqlx::query(r#"INSERT INTO "account_1" VALUES (?, ?, ?, ?, ?)"#)
                .bind(ei)
                .bind(nickname)
                .bind(last)
                .bind(false)
                .bind(disabled)
                .execute(&mut *conn)
                .await?;
        }

        sqlx::raw_sql(
            r#"DROP TABLE "account";
                ALTER TABLE "account_1" RENAME TO "account";
                UPDATE "meta" SET "value" = '4' WHERE "key" = 'version';"#,
        )
        .execute(&mut *conn)
        .await?;
        info!("Merge completed");
        Ok(())
    }
}

pub mod v5 {
    use log::info;
    use sqlx::SqliteConnection;

    use crate::egg::is_contract_cleared;

    pub const VERSION: &str = "5";

    const MERGE_STATEMENT_STAGE: &str = r#"
        CREATE TABLE "contract_cache_1" (
            "id"	TEXT NOT NULL,
            "room"	TEXT NOT NULL,
            "body"	BLOB NOT NULL,
            "timestamp"	INTEGER NOT NULL,
            "cleared"	INTEGER NOT NULL,
            PRIMARY KEY("id")
        );
    "#;

    pub async fn merge_v4(conn: &mut SqliteConnection) -> sqlx::Result<()> {
        info!("Performing database prepare stage (v5)");
        sqlx::raw_sql(MERGE_STATEMENT_STAGE)
            .execute(&mut *conn)
            .await?;

        let contracts = sqlx::query_as::<_, (String, String, Vec<u8>, i64)>(
            r#"SELECT "id", "room", "body", "timestamp" FROM "contract_cache""#,
        )
        .fetch_all(&mut *conn)
        .await?;

        info!("Merge contract, total {} contracts", contracts.len());
        for (id, room, body, timestamp) in contracts {
            let cleared = is_contract_cleared(&body);
            sqlx::query(r#"INSERT INTO "contract_cache_1" VALUES (?, ?, ?, ?, ?)"#)
                .bind(id)
                .bind(room)
                .bind(body)
                .bind(timestamp)
                .bind(cleared)
                .execute(&mut *conn)
                .await?;
        }

        sqlx::raw_sql(
            r#"DROP TABLE "contract_cache";
                ALTER TABLE "contract_cache_1" RENAME TO "contract_cache";
                UPDATE "meta" SET "value" = '5' WHERE "key" = 'version';"#,
        )
        .execute(&mut *conn)
        .await?;
        info!("Merge completed");
        Ok(())
    }
}

pub mod v6 {
    use log::info;
    use sqlx::SqliteConnection;

    use crate::types::ContractCache;

    pub const VERSION: &str = "6";
    pub const CREATE_STATEMENT: &str = r#"
        CREATE TABLE "account" (
            "ei"        TEXT NOT NULL,
            "nickname"  TEXT,
            "last_fetch"    INTEGER NOT NULL DEFAULT 0,
            "contract_trace" INTEGER NOT NULL DEFAULT 0,
            "disabled"  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY("ei")
        );

        CREATE TABLE "user" (
            "id" INTEGER NOT NULL,
            "accounts" TEXT NOT NULL,
            PRIMARY KEY("id")
        );

        CREATE TABLE "account_map" (
            "ei" TEXT NOT NULL,
            "users" TEXT NOT NULL,
            PRIMARY KEY("ei")
        );

        CREATE TABLE "meta" (
            "key"       TEXT NOT NULL,
            "value"     TEXT,
            PRIMARY KEY("key")
        );

        CREATE TABLE "spaceship" (
            "id"        TEXT NOT NULL,
            "name"      TEXT NOT NULL,
            "duration_type"  INTEGER NOT NULL,
            "belong"    TEXT NOT NULL,
            "land"      INTEGER NOT NULL,
            "notified" INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY("id")
        );

        CREATE TABLE "player_contract" (
            "id"	TEXT NOT NULL,
            "room"	TEXT NOT NULL,
            "belong"	TEXT NOT NULL,
            "start_time" REAL,
            "finished"	INTEGER NOT NULL,
            PRIMARY KEY("id", "belong")
        );

        CREATE TABLE "contract" (
            "id"	TEXT NOT NULL,
            "size"  INTEGER NOT NULL,
            "token_time" REAL NOT NULL,
            "body"	BLOB NOT NULL,
            PRIMARY KEY("id")
        );

        CREATE TABLE "contract_cache" (
            "id"	TEXT NOT NULL,
            "room"	TEXT NOT NULL,
            "body"	BLOB NOT NULL,
            "timestamp"	INTEGER NOT NULL,
            "cleared"	INTEGER NOT NULL,
            PRIMARY KEY("id", "room")
        );
    "#;

    const MERGE_STATEMENT_STAGE: &str = r#"
        CREATE TABLE "contract_cache_1" (
            "id"	TEXT NOT NULL,
            "room"	TEXT NOT NULL,
            "body"	BLOB NOT NULL,
            "timestamp"	INTEGER NOT NULL,
            "cleared"	INTEGER NOT NULL,
            PRIMARY KEY("id", "room")
        );
    "#;

    pub async fn merge_v5(conn: &mut SqliteConnection) -> sqlx::Result<()> {
        info!("Performing database prepare stage (v6)");
        sqlx::raw_sql(MERGE_STATEMENT_STAGE)
            .execute(&mut *conn)
            .await?;

        let contracts = sqlx::query_as::<_, ContractCache>(r#"SELECT * FROM "contract_cache""#)
            .fetch_all(&mut *conn)
            .await?;

        info!("Merge contract, total {} contracts", contracts.len());
        for contract in contracts {
            sqlx::query(r#"INSERT INTO "contract_cache_1" VALUES (?, ?, ?, ?, ?)"#)
                .bind(contract.id())
                .bind(contract.room())
                .bind(contract.body())
                .bind(contract.timestamp())
                .bind(contract.cleared())
                .execute(&mut *conn)
                .await?;
        }

        sqlx::raw_sql(
            r#"DROP TABLE "contract_cache";
                ALTER TABLE "contract_cache_1" RENAME TO "contract_cache";
                UPDATE "meta" SET "value" = '6' WHERE "key" = 'version';"#,
        )
        .execute(&mut *conn)
        .await?;
        info!("Merge completed");
        Ok(())
    }
}

#[derive(Debug)]
pub struct Database {
    conn: sqlx::SqliteConnection,
    init: bool,
}

#[async_trait::async_trait]
pub trait DatabaseCheckExt {
    fn conn_(&mut self) -> &mut sqlx::SqliteConnection;

    async fn check_database_table(&mut self) -> sqlx::Result<bool> {
        Ok(
            sqlx::query(r#"SELECT 1 FROM sqlite_master WHERE type='table' AND "name" = 'meta'"#)
                .fetch_optional(self.conn_())
                .await?
                .is_some(),
        )
    }

    async fn check_database_version(&mut self) -> sqlx::Result<Option<String>> {
        Ok(
            sqlx::query_as::<_, (String,)>(r#"SELECT "value" FROM "meta" WHERE "key" = 'version'"#)
                .fetch_optional(self.conn_())
                .await?
                .map(|(x,)| x),
        )
    }

    async fn insert_database_version(&mut self) -> sqlx::Result<()> {
        sqlx::query(r#"INSERT INTO "meta" VALUES ("version", ?)"#)
            .bind(current::VERSION)
            .execute(self.conn_())
            .await?;
        Ok(())
    }

    async fn create_db(&mut self) -> sqlx::Result<()> {
        let mut executer = sqlx::raw_sql(current::CREATE_STATEMENT).execute_many(self.conn_());
        while let Some(ret) = executer.next().await {
            ret?;
        }
        Ok(())
    }
}

impl Database {
    pub async fn connect(database: &str) -> DBResult<Self> {
        let conn = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .create_if_missing(true)
                .filename(database),
        )
        .await?;
        Ok(Self { conn, init: false })
    }

    pub async fn init(&mut self) -> sqlx::Result<bool> {
        self.init = true;
        if !self.check_database_table().await? {
            self.create_db().await?;
            self.insert_database_version().await?;
        }
        loop {
            if let Some(version) = self.check_database_version().await? {
                match version.as_str() {
                    v1::VERSION => {
                        v2::merge_v1(&mut self.conn).await?;
                    }
                    v2::VERSION => {
                        v3::merge_v2(&mut self.conn).await?;
                    }
                    v3::VERSION => {
                        v4::merge_v3(&mut self.conn).await?;
                    }
                    v4::VERSION => {
                        v5::merge_v4(&mut self.conn).await?;
                    }
                    v5::VERSION => {
                        v6::merge_v5(&mut self.conn).await?;
                    }
                    current::VERSION => break,
                    _ => {
                        panic!("Unknown database version: {version}, exit")
                    }
                }
            }
        }
        Ok(true)
    }

    pub async fn query_ei(&mut self, user: i64) -> DBResult<Vec<Account>> {
        let mut v = vec![];
        let Some(m_user) = self.query_user(user).await? else {
            return Ok(v);
        };

        for ei in m_user.accounts() {
            v.push(
                sqlx::query_as(r#"SELECT * FROM "account" WHERE "ei" = ? ORDER BY "ei""#)
                    .bind(ei)
                    .fetch_one(&mut self.conn)
                    .await?,
            );
        }
        Ok(v)
    }

    async fn modify_user(&mut self, user: User) -> DBResult<()> {
        sqlx::query(r#"UPDATE "user" SET "accounts" = ? WHERE "id" = ? "#)
            .bind(user.account_to_str())
            .bind(user.id())
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }
    async fn add_user(&mut self, user: User) -> DBResult<()> {
        sqlx::query(r#"INSERT INTO "user" VALUES (?, ?) "#)
            .bind(user.id())
            .bind(user.account_to_str())
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn delete_account(&mut self, user: i64, ei: &str) -> DBResult<()> {
        let Some(mut m_user) = self.query_user(user).await? else {
            return Ok(());
        };

        let mut map = self.query_account_map(ei).await?;

        m_user.remove_account(ei.to_string());
        map.remove_user(user);

        self.modify_user(m_user).await?;
        self.modify_account_map(map).await?;

        Ok(())
    }

    pub async fn query_all_account(&mut self) -> DBResult<Vec<Account>> {
        sqlx::query_as(r#"SELECT * FROM "account" "#)
            .fetch_all(&mut self.conn)
            .await
    }

    pub async fn query_account(&mut self, ei: &str) -> DBResult<Option<Account>> {
        sqlx::query_as(r#"SELECT * FROM "account" WHERE "ei" = ? "#)
            .bind(ei)
            .fetch_optional(&mut self.conn)
            .await
    }
    pub async fn query_user(&mut self, user: i64) -> DBResult<Option<User>> {
        sqlx::query_as(r#"SELECT * FROM "user" WHERE "id" = ? "#)
            .bind(user)
            .fetch_optional(&mut self.conn)
            .await
    }

    pub async fn query_all_user(&mut self) -> DBResult<Vec<User>> {
        sqlx::query_as(r#"SELECT * FROM "user" "#)
            .fetch_all(&mut self.conn)
            .await
    }

    async fn insert_account_map(&mut self, map: AccountMap) -> DBResult<()> {
        sqlx::query(r#"INSERT INTO "account_map" VALUES (?, ?) "#)
            .bind(map.ei())
            .bind(map.user_to_str())
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }
    async fn modify_account_map(&mut self, map: AccountMap) -> DBResult<()> {
        sqlx::query(r#"UPDATE "account_map" SET "users" = ? WHERE "ei" = ? "#)
            .bind(map.user_to_str())
            .bind(map.ei())
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn query_account_map(&mut self, ei: &str) -> DBResult<AccountMap> {
        sqlx::query_as(r#"SELECT * FROM "account_map" WHERE "ei" = ?"#)
            .bind(ei)
            .fetch_one(&mut self.conn)
            .await
    }

    pub async fn insert_account(&mut self, ei: &str, user: i64) -> DBResult<bool> {
        let account = self.query_account(ei).await?;

        if account.is_none() {
            sqlx::query(r#"INSERT INTO "account" VALUES (?, NULL, 0, 0, 0)"#)
                .bind(ei)
                .execute(&mut self.conn)
                .await?;
            self.insert_account_map(AccountMap::new(ei.to_string(), user))
                .await?;
        } else {
            let mut map = self.query_account_map(ei).await?;
            map.add_user(user);
            self.modify_account_map(map).await?;
        }

        let m_user = self.query_user(user).await?;
        match m_user {
            Some(mut user) => {
                user.add_account(ei.to_string());
                self.modify_user(user).await?;
            }
            None => {
                self.add_user(User::new(user, ei.to_string())).await?;
            }
        }
        Ok(true)
    }

    pub async fn set_account_nickname(&mut self, ei: &str, nickname: &str) -> DBResult<()> {
        sqlx::query(r#"UPDATE "account" SET "nickname" = ? WHERE "ei" = ?"#)
            .bind(nickname)
            .bind(ei)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn set_account_status(
        &mut self,
        ei: &str,
        last: i64,
        disabled: bool,
    ) -> DBResult<()> {
        sqlx::query(r#"UPDATE "account" SET "last_fetch" = ?, "disabled" = ? WHERE "ei" = ? "#)
            .bind(last)
            .bind(disabled)
            .bind(ei)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn set_account_contract_trace(&mut self, ei: &str, enabled: bool) -> DBResult<()> {
        sqlx::query(r#"UPDATE "account" SET "contract_trace" = ? WHERE "ei" = ? "#)
            .bind(enabled)
            .bind(ei)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn account_timestamp_reset(&mut self, ei: &str) -> DBResult<()> {
        sqlx::query(r#"UPDATE "account" SET "last_fetch" = 0 WHERE "ei" = ? "#)
            .bind(ei)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn account_status_reset(&mut self, ei: &str, disabled: bool) -> DBResult<()> {
        sqlx::query(r#"UPDATE "account" SET "disabled" = ? WHERE "ei" = ? "#)
            .bind(disabled)
            .bind(ei)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn account_mission_reset(&mut self, ei: &str, limit: usize) -> DBResult<()> {
        for spaceship in self
            .query_spaceship_by_ei(ei)
            .await?
            .into_iter()
            .take(limit)
        {
            sqlx::query(r#"UPDATE "spaceship" SET "notified" = 0 WHERE "id" = ?"#)
                .bind(spaceship.id())
                .execute(&mut self.conn)
                .await?;
        }
        Ok(())
    }

    pub async fn insert_spaceship(
        &mut self,
        id: String,
        name: String,
        duration: i64,
        belong: String,
        land: i64,
    ) -> DBResult<()> {
        sqlx::query(r#"INSERT INTO "spaceship" VALUES (?, ?, ?, ?, ?, 0)"#)
            .bind(id)
            .bind(name)
            .bind(duration)
            .bind(belong)
            .bind(land)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn query_spaceship_by_time(&mut self, deadline: i64) -> DBResult<Vec<SpaceShip>> {
        sqlx::query_as(
            r#"SELECT * FROM "spaceship" WHERE "land" <= ? AND "notified" = 0 ORDER BY "land" DESC LIMIT 20 "#,
        )
        .bind(deadline)
        .fetch_all(&mut self.conn)
        .await
    }

    pub async fn query_spaceship_by_id(&mut self, identifier: &str) -> DBResult<Option<SpaceShip>> {
        sqlx::query_as(r#"SELECT * FROM "spaceship" WHERE "id" = ?"#)
            .bind(identifier)
            .fetch_optional(&mut self.conn)
            .await
    }

    pub async fn query_spaceship_by_ei(&mut self, ei: &str) -> DBResult<Vec<SpaceShip>> {
        sqlx::query_as(
            r#"SELECT * FROM "spaceship" WHERE "belong" = ? ORDER BY "land" DESC LIMIT 6"#,
        )
        .bind(ei)
        .fetch_all(&mut self.conn)
        .await
    }

    pub async fn mark_spaceship(&mut self, id: &str) -> DBResult<()> {
        sqlx::query(r#"UPDATE "spaceship" SET "notified" = 1 WHERE "id" = ? "#)
            .bind(id)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn query_single_contract(
        &mut self,
        id: &str,
        ei: &str,
    ) -> DBResult<Option<Contract>> {
        sqlx::query_as(r#"SELECT * FROM "player_contract" WHERE "id" = ? AND "belong" = ?"#)
            .bind(id)
            .bind(ei)
            .fetch_optional(&mut self.conn)
            .await
    }

    pub async fn insert_user_contract(
        &mut self,
        id: &str,
        room: &str,
        ei: &str,
        finished: bool,
    ) -> DBResult<()> {
        sqlx::query(r#"INSERT INTO "player_contract" VALUES (?, ?, ?, NULL, ?)"#)
            .bind(id)
            .bind(room)
            .bind(ei)
            //.bind(start_time)
            .bind(finished)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn set_contract(
        &mut self,
        id: &str,
        ei: &str,
        room: &str,
        finished: bool,
    ) -> DBResult<()> {
        let start_time = self
            .query_id_room_with_start_time(id, room)
            .await?
            .and_then(|x| x.start_time());

        sqlx::query(
            r#"UPDATE "player_contract"
            SET "finished" = ?, "room" = ?, "start_time" = ?
            WHERE "id" = ? AND "belong" = ? "#,
        )
        .bind(finished)
        .bind(room)
        .bind(start_time)
        .bind(id)
        .bind(ei)
        .execute(&mut self.conn)
        .await?;
        Ok(())
    }

    pub async fn set_contract_start_time(
        &mut self,
        id: &str,
        room: &str,
        start_time: f64,
    ) -> DBResult<()> {
        sqlx::query(
            r#"UPDATE "player_contract" SET "start_time" = ? WHERE "id" = ? AND "room" = ? AND "start_time" IS NULL"#,
        )
        .bind(start_time)
        .bind(id)
        .bind(room)
        .execute(&mut self.conn)
        .await?;
        Ok(())
    }

    async fn query_id_room_with_start_time(
        &mut self,
        id: &str,
        room: &str,
    ) -> DBResult<Option<Contract>> {
        sqlx::query_as(r#"SELECT * FROM "player_contract" WHERE "id" = ? AND "room" = ? AND "start_time" IS NOT NULL LIMIT 1"#)
            .bind(id)
            .bind(room)
            .fetch_optional(&mut self.conn)
            .await
    }

    pub async fn query_contract(&mut self, ei: &str) -> DBResult<Vec<Contract>> {
        sqlx::query_as(
            r#"SELECT * FROM "player_contract"
            WHERE "belong" = ?
            ORDER BY "start_time" DESC LIMIT 10"#,
        )
        .bind(ei)
        .fetch_all(&mut self.conn)
        .await
    }

    pub async fn query_contract_spec(&mut self, id: &str) -> DBResult<Option<ContractSpec>> {
        sqlx::query_as(
            r#"SELECT * FROM "contract"
            WHERE "id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&mut self.conn)
        .await
    }

    pub async fn insert_contract_cache(
        &mut self,
        id: &str,
        room: &str,
        body: &[u8],
        timestamp: i64,
        // Cleared for exit
        cleared: bool,
    ) -> DBResult<()> {
        sqlx::query(r#"INSERT INTO "contract_cache" VALUES (?, ?, ?, ?, ?)"#)
            .bind(id)
            .bind(room)
            .bind(body)
            .bind(timestamp)
            .bind(cleared)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }
    pub async fn insert_contract_spec(
        &mut self,
        id: &str,
        size: i64,
        token_time: f64,
        body: &[u8],
    ) -> DBResult<()> {
        sqlx::query(r#"INSERT INTO "contract" VALUES (?, ?, ?, ?)"#)
            .bind(id)
            .bind(size)
            .bind(token_time)
            .bind(body)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn update_contract_cache(
        &mut self,
        id: &str,
        room: &str,
        body: &[u8],
        timestamp: i64,
        cleared: bool,
    ) -> DBResult<()> {
        sqlx::query(r#"UPDATE "contract_cache" SET "body" = ?, "timestamp" = ?, "cleared" = ? WHERE "id" = ? AND "room" = ?"#)
            .bind(body)
            .bind(timestamp)
            .bind(cleared)
            .bind(id)
            .bind(room)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }
    pub async fn update_contract_cache_timestamp(
        &mut self,
        id: &str,
        room: &str,
        timestamp: i64,
    ) -> DBResult<()> {
        sqlx::query(r#"UPDATE "contract_cache" SET "timestamp" = ? WHERE "id" = ? AND "room" = ?"#)
            .bind(timestamp)
            .bind(id)
            .bind(room)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn query_contract_cache(
        &mut self,
        id: &str,
        room: &str,
    ) -> DBResult<Option<ContractCache>> {
        sqlx::query_as(r#"SELECT * FROM "contract_cache" WHERE "id" = ? AND "room" = ?"#)
            .bind(id)
            .bind(room)
            .fetch_optional(&mut self.conn)
            .await
    }

    pub async fn close(self) -> DBResult<()> {
        self.conn.close().await
    }
}

impl DatabaseCheckExt for Database {
    fn conn_(&mut self) -> &mut sqlx::SqliteConnection {
        &mut self.conn
    }
}

type CheckerArg = ((f64, f64), fn(&[u8], (f64, f64)) -> bool);

kstool_helper_generator::oneshot_helper! {
#[derive(Debug)]
pub enum DatabaseEvent {

    #[ret(bool)]
    AccountAdd {
        ei: String,
        user: i64,
    },

    #[ret(Vec<User>)]
    UserQueryAll,

    #[ret(Vec<Account>)]
    AccountQuery {
        id: Option<i64>,
    },
    #[ret(AccountMap)]
    AccountQueryUsers {
        ei: String,
    },

    #[ret(Option<Account>)]
    AccountQueryEI {
        ei: String,
    },

    UserRemoveAccount {
        user: i64,
        ei: String,
    },

    AccountUpdate{
        ei: String,
        disabled: bool,
    },

    AccountContractUpdate{
        ei: String,
        enabled: bool,
    },

    AccountNameUpdate {
        ei: String,
        name: String,
    },

    AccountTimestampReset {
        ei: String,
    },
    AccountMissionReset {
        ei: String,
        limit: usize,
    },
    AccountStatusReset {
        ei: String,
        disabled: bool,
    },

    MissionAdd {
        id: String,
        name: String,
        duration_type: i64,
        belong: String,
        land: i64
    },

    #[ret(Vec<SpaceShip>)]
    MissionQuery{
        deadline: u64,
    },
    #[ret(HashMap<Account, Vec<SpaceShip>>)]
    MissionQueryByUser { id: i64, query_recent: bool },

    #[ret(Vec<SpaceShip>)]
    MissionQueryByAccount { ei: String },

    MissionUpdated { id: String },

    #[ret(Option<SpaceShip>)]
    MissionSingleQuery { identifier: String },

    #[ret(Option<Contract>)]
    ContractQuerySingle {
        id: String,
        ei: String
    },
    #[ret(Option<ContractSpec>)]
    ContractQuerySpec {
        id: String,
    },
    #[ret(Vec<Contract>)]
    AccountQueryContract {
        ei: String,
    },
    #[ret(bool)]
    AccountInsertContract {
        id: String,
        room: String,
        ei: String,
        finished: bool,
    },
    ContractUpdate {
        id: String,
        room: String,
        ei: String,
        finished: bool,
    },
    ContractStartTimeUpdate {
        id: String,
        room: String,
        start_time: f64,
    },
    #[ret(Option<ContractCache>)]
    ContractCacheQuery {
        id: String,
        room: String
    },
    #[ret(bool)]
    ContractCacheInsert {
        id: String,
        room: String,
        cache: Vec<u8>,
        cleared: bool,
        amount_to_check: Option<CheckerArg>,
    },
    ContractCacheUpdateTimestamp {
        id: String,
        room: String,
    },
    #[ret(bool)]
    ContractSpecInsert(ContractSpec),

    Terminate,
}
}

pub struct DatabaseHandle {
    handle: tokio::task::JoinHandle<DBResult<()>>,
}

impl DatabaseHandle {
    pub async fn connect(file: &str) -> anyhow::Result<(Self, DatabaseHelper)> {
        let mut database = Database::connect(file).await?;
        database.init().await?;
        let (sender, receiver) = DatabaseHelper::new(16);
        Ok((
            Self {
                handle: tokio::spawn(Self::run(database, receiver)),
            },
            sender,
        ))
    }

    async fn handle_event(database: &mut Database, event: DatabaseEvent) -> DBResult<()> {
        match event {
            DatabaseEvent::AccountAdd {
                ei,
                user,
                __private_sender,
            } => {
                __private_sender
                    .send(database.insert_account(&ei, user).await?)
                    .ok();
            }
            DatabaseEvent::AccountQuery {
                id,
                __private_sender,
            } => {
                match id {
                    Some(id) => __private_sender.send(database.query_ei(id).await?),
                    None => __private_sender.send(database.query_all_account().await?),
                }
                .ok();
            }
            DatabaseEvent::UserRemoveAccount { user, ei } => {
                database.delete_account(user, &ei).await?;
            }
            DatabaseEvent::AccountUpdate { ei, disabled } => {
                database
                    .set_account_status(&ei, kstool::time::get_current_second() as i64, disabled)
                    .await?;
            }
            DatabaseEvent::MissionAdd {
                belong,
                name,
                duration_type,
                id,
                land,
            } => {
                database
                    .insert_spaceship(id, name, duration_type, belong, land)
                    .await?;
            }
            DatabaseEvent::MissionQuery {
                deadline,
                __private_sender,
            } => {
                let r = database.query_spaceship_by_time(deadline as i64).await?;
                __private_sender.send(r).ok();
            }
            DatabaseEvent::AccountQueryEI {
                ei,
                __private_sender,
            } => {
                let r = database.query_account(&ei).await?;
                __private_sender.send(r).ok();
            }
            DatabaseEvent::Terminate => {
                unreachable!()
            }
            DatabaseEvent::MissionUpdated { id } => {
                database.mark_spaceship(&id).await?;
            }
            DatabaseEvent::AccountNameUpdate { ei, name } => {
                database.set_account_nickname(&ei, &name).await?;
            }
            DatabaseEvent::MissionSingleQuery {
                identifier,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_spaceship_by_id(&identifier).await?)
                    .ok();
            }
            DatabaseEvent::MissionQueryByUser {
                id,
                query_recent,
                __private_sender,
            } => {
                let current = kstool::time::get_current_second() as i64;
                let mut map = HashMap::new();
                for account in database.query_ei(id).await? {
                    let missions = database.query_spaceship_by_ei(account.ei()).await?;
                    map.insert(
                        account,
                        if query_recent {
                            missions
                                .into_iter()
                                .filter(|s| {
                                    let diff = s.land() - current;
                                    diff > 0 && diff <= 3600 && !s.notified()
                                })
                                .collect()
                        } else {
                            missions
                        },
                    );
                }
                __private_sender.send(map).ok();
            }
            DatabaseEvent::MissionQueryByAccount {
                ei,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_spaceship_by_ei(&ei).await?)
                    .ok();
            }
            DatabaseEvent::AccountTimestampReset { ei } => {
                database.account_timestamp_reset(&ei).await?;
            }
            DatabaseEvent::AccountMissionReset { ei, limit } => {
                database.account_mission_reset(&ei, limit).await?;
            }
            DatabaseEvent::AccountStatusReset { ei, disabled } => {
                database.account_status_reset(&ei, disabled).await?;
            }
            DatabaseEvent::UserQueryAll(sender) => {
                sender.send(database.query_all_user().await?).ok();
            }
            DatabaseEvent::AccountQueryUsers {
                ei,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_account_map(&ei).await?)
                    .ok();
            }
            DatabaseEvent::ContractCacheInsert {
                id,
                room,
                cache,
                cleared,
                amount_to_check,
                __private_sender,
            } => {
                let current = kstool::time::get_current_second() as i64;
                if let Some(original_cache) = database.query_contract_cache(&id, &room).await? {
                    __private_sender.send(true).ok();
                    if let Some((amount, checker)) = amount_to_check {
                        if !checker(original_cache.body(), amount) {
                            //log::warn!("Trying update outdated cache, skip");
                            return Ok(());
                        }
                    }
                    database
                        .update_contract_cache(&id, &room, &cache, current, cleared)
                        .await?;
                } else {
                    database
                        .insert_contract_cache(&id, &room, &cache, current, cleared)
                        .await?;
                    __private_sender.send(false).ok();
                }
            }
            DatabaseEvent::ContractSpecInsert(contract_spec, sender) => {
                if database
                    .query_contract_spec(contract_spec.id())
                    .await?
                    .is_some()
                {
                    sender.send(false).ok();
                    return Ok(());
                }

                let id = contract_spec.id().to_string();
                let body = minicbor_serde::to_vec(contract_spec.get_inner()).unwrap();
                database
                    .insert_contract_spec(
                        &id,
                        contract_spec.max_coop_size(),
                        contract_spec.token_time(),
                        &body,
                    )
                    .await?;
                sender.send(true).ok();
            }
            DatabaseEvent::ContractQuerySingle {
                id,
                ei,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_single_contract(&id, &ei).await?)
                    .ok();
            }
            DatabaseEvent::AccountInsertContract {
                id,
                room,
                ei,
                finished,
                __private_sender,
            } => {
                if let Some(contract) = database.query_single_contract(&id, &ei).await? {
                    let changed = contract.finished() != finished || !contract.room().eq(&room);
                    if changed {
                        database
                            .set_contract(&id, &ei, contract.room(), finished)
                            .await?;
                    }
                    __private_sender.send(changed).ok();
                } else {
                    database
                        .insert_user_contract(&id, &room, &ei, finished)
                        .await?;
                    __private_sender.send(true).ok();
                }
            }
            DatabaseEvent::ContractUpdate {
                id,
                room,
                ei,
                finished,
            } => {
                database.set_contract(&id, &ei, &room, finished).await?;
            }
            DatabaseEvent::AccountContractUpdate { ei, enabled } => {
                database.set_account_contract_trace(&ei, enabled).await?;
            }
            DatabaseEvent::AccountQueryContract {
                ei,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_contract(&ei).await?)
                    .ok();
            }
            DatabaseEvent::ContractQuerySpec {
                id,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_contract_spec(&id).await?)
                    .ok();
            }
            DatabaseEvent::ContractCacheQuery {
                id,
                room,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_contract_cache(&id, &room).await?)
                    .ok();
            }
            DatabaseEvent::ContractStartTimeUpdate {
                id,
                room,
                start_time,
            } => {
                database
                    .set_contract_start_time(&id, &room, start_time)
                    .await?;
            }
            DatabaseEvent::ContractCacheUpdateTimestamp { id, room } => {
                database
                    .update_contract_cache_timestamp(&id, &room, 0)
                    .await?;
            }
        }
        Ok(())
    }

    async fn run(mut database: Database, mut receiver: DatabaseEventReceiver) -> DBResult<()> {
        while let Some(event) = receiver.recv().await {
            if let DatabaseEvent::Terminate = event {
                break;
            }
            Self::handle_event(&mut database, event)
                .await
                .inspect_err(|e| error!("Sqlite error: {e:?}"))
                .ok();
        }
        database.close().await?;
        Ok(())
    }

    pub async fn wait(self) -> anyhow::Result<()> {
        Ok(self.handle.await??)
    }
}

pub type DBResult<T> = sqlx::Result<T>;
pub use v6 as current;

use crate::types::{Account, AccountMap, Contract, ContractCache, ContractSpec, SpaceShip, User};
