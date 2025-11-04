use super::types::*;
use super::{DBResult, versions::prelude::*};
use futures_util::StreamExt as _;
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};

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
                    v6::VERSION => {
                        v7::merge_v6(&mut self.conn).await?;
                    }
                    v7::VERSION => {
                        v8::merge_v7(&mut self.conn).await?;
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

    pub async fn query_contract_cache_timestamp(
        &mut self,
        id: &str,
        room: &str,
    ) -> DBResult<Option<i64>> {
        Ok(sqlx::query_as::<_, (i64,)>(
            r#"SELECT "timestamp" FROM "contract_cache" WHERE "id" = ? AND "room" = ?"#,
        )
        .bind(id)
        .bind(room)
        .fetch_optional(&mut self.conn)
        .await?
        .map(|(x,)| x))
    }

    pub async fn query_subscribe_single(
        &mut self,
        id: &str,
        room: &str,
    ) -> DBResult<Option<SubscribeInfo>> {
        sqlx::query_as(r#"SELECT * FROM "subscriber" WHERE "id" = ? AND "room" = ?"#)
            .bind(id)
            .bind(room)
            .fetch_optional(&mut self.conn)
            .await
    }

    pub async fn query_subscribe(&mut self, timestamp: i64) -> DBResult<Vec<SubscribeInfo>> {
        sqlx::query_as(r#"SELECT * FROM "subscriber" WHERE "notified" = 0 AND "est" < ?"#)
            .bind(timestamp)
            .fetch_all(&mut self.conn)
            .await
    }

    pub async fn modify_subscribe(
        &mut self,
        id: &str,
        room: &str,
        user: i64,
        is_del: bool,
    ) -> DBResult<()> {
        let Some(mut sub) = self.query_subscribe_single(id, room).await? else {
            return self.insert_subscribe(id, room, user).await;
        };
        let exist = sub.check_user(user);
        if (is_del && !exist) || (!is_del && exist) {
            return Ok(());
        }
        if is_del {
            sub.add_user(user);
        } else {
            sub.delete_user(user);
        }
        self.update_subscribe(sub.id(), sub.room(), &sub.user_to_str(), sub.est())
            .await
    }

    async fn update_subscribe(
        &mut self,
        id: &str,
        room: &str,
        users: &str,
        est: i64,
    ) -> DBResult<()> {
        sqlx::query(
            r#"UPDATE "subscriber" SET "users" = ?, "est" = ? WHERE "id" = ? AND "room" = ?"#,
        )
        .bind(users)
        .bind(est)
        .bind(id)
        .bind(room)
        .execute(&mut self.conn)
        .await?;
        Ok(())
    }

    async fn insert_subscribe(&mut self, id: &str, room: &str, user: i64) -> DBResult<()> {
        sqlx::query(r#"INSERT INTO "subscriber" VALUES (?, ?, ?, ?)"#)
            .bind(id)
            .bind(room)
            .bind(user.to_string())
            .bind(0)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn update_subscribe_est(&mut self, id: &str, room: &str, est: i64) -> DBResult<()> {
        sqlx::query(r#"UPDATE "subscriber" SET "est" = ? WHERE "id" = ? AND "room" = ?"#)
            .bind(est)
            .bind(id)
            .bind(room)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn update_subscribe_notified(&mut self, id: &str, room: &str) -> DBResult<()> {
        sqlx::query(r#"UPDATE "subscriber" SET "notified" = ? WHERE "id" = ? AND "room" = ?"#)
            .bind(true)
            .bind(id)
            .bind(room)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn insert_account_cache(&mut self, ei: &str, cache: String) -> DBResult<()> {
        if self.query_account_cache(ei).await?.is_some() {
            sqlx::query(
                r#"UPDATE "account_cache" SET "cache" = ?, "timestamp" = ? WHERE "ei" = ?"#,
            )
            .bind(cache)
            .bind(kstool::time::get_current_second() as i64)
            .bind(ei)
            .execute(&mut self.conn)
            .await?;
            return Ok(());
        }

        sqlx::query(r#"INSERT INTO "account_cache" VALUES (?, ?, ?)"#)
            .bind(ei)
            .bind(cache)
            .bind(kstool::time::get_current_second() as i64)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn query_account_cache(&mut self, ei: &str) -> DBResult<Option<AccountCache>> {
        sqlx::query_as(r#"SELECT * FROM "account_cache" WHERE "ei" = ?"#)
            .bind(ei)
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
