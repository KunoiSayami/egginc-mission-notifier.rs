use std::collections::HashMap;

use futures_util::StreamExt as _;
use log::error;
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};

pub mod v1 {
    pub const CREATE_STATEMENT: &str = r#"
        CREATE TABLE "player" (
            "ei"        TEXT NOT NULL,
            "user"      INTEGER NOT NULL,
            "nickname"  TEXT,
            "last_fetch"        INTEGER NOT NULL DEFAULT 0,
            "disabled"  INTEGER NOT NULL DEFAULT 0,
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
            "belong"    TEXT NOT NULL,
            "land"      INTEGER NOT NULL,
            "notified" INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY("id")
        );
    "#;

    pub const VERSION: &str = "1";
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

    /* async fn check_database_version(&mut self) -> sqlx::Result<Option<String>> {
        Ok(
            sqlx::query_as::<_, (String,)>(r#"SELECT "value" FROM "meta" WHERE "key" = 'version'"#)
                .fetch_optional(self.conn_())
                .await?
                .map(|(x,)| x),
        )
    } */

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
        Ok(true)
    }

    pub async fn query_ei(&mut self, user: i64) -> DBResult<Vec<Player>> {
        sqlx::query_as(r#"SELECT * FROM "player" WHERE "user" = ? ORDER BY "ei""#)
            .bind(user)
            .fetch_all(&mut self.conn)
            .await
    }

    pub async fn delete_player(&mut self, ei: &str) -> DBResult<()> {
        sqlx::query(r#"DELETE FROM "player" WHERE "ei" = ? "#)
            .bind(ei)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn query_all_player(&mut self) -> DBResult<Vec<Player>> {
        sqlx::query_as(r#"SELECT * FROM "player" "#)
            .fetch_all(&mut self.conn)
            .await
    }

    pub async fn query_ei_by_player(&mut self, ei: &str) -> DBResult<Option<Player>> {
        sqlx::query_as(r#"SELECT * FROM "player" WHERE "ei" = ? "#)
            .bind(ei)
            .fetch_optional(&mut self.conn)
            .await
    }

    pub async fn insert_player(&mut self, ei: &str, user: i64) -> DBResult<bool> {
        if self.query_ei_by_player(ei).await?.is_some() {
            return Ok(false);
        }
        sqlx::query(r#"INSERT INTO "player" VALUES (?, ?, NULL, 0, 0)"#)
            .bind(ei)
            .bind(user)
            .execute(&mut self.conn)
            .await?;
        Ok(true)
    }

    pub async fn set_player_nickname(&mut self, ei: &str, nickname: &str) -> DBResult<()> {
        sqlx::query(r#"UPDATE "player" SET "nickname" = ? WHERE "ei" = ?"#)
            .bind(nickname)
            .bind(ei)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn set_player_status(&mut self, ei: &str, last: i64, disabled: bool) -> DBResult<()> {
        sqlx::query(r#"UPDATE "player" SET "last_fetch" = ?, "disabled" = ? WHERE "ei" = ? "#)
            .bind(last)
            .bind(disabled)
            .bind(ei)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn player_timestamp_reset(&mut self, ei: &str) -> DBResult<()> {
        sqlx::query(r#"UPDATE "player" SET "last_fetch" = 0 WHERE "ei" = ? "#)
            .bind(ei)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn player_status_reset(&mut self, ei: &str, disabled: bool) -> DBResult<()> {
        sqlx::query(r#"UPDATE "player" SET "disabled" = ? WHERE "ei" = ? "#)
            .bind(disabled)
            .bind(ei)
            .execute(&mut self.conn)
            .await?;
        Ok(())
    }

    pub async fn player_mission_reset(&mut self, ei: &str, limit: usize) -> DBResult<()> {
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
        belong: String,
        land: i64,
    ) -> DBResult<()> {
        sqlx::query(r#"INSERT INTO "spaceship" VALUES (?, ?, ?, ?, 0)"#)
            .bind(id)
            .bind(name)
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
            r#"SELECT * FROM "spaceship" WHERE "belong" = ? ORDER BY "land" DESC LIMIT 10"#,
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

    pub async fn close(self) -> DBResult<()> {
        self.conn.close().await
    }
}

impl DatabaseCheckExt for Database {
    fn conn_(&mut self) -> &mut sqlx::SqliteConnection {
        &mut self.conn
    }
}

kstool_helper_generator::oneshot_helper! {
#[derive(Debug)]
pub enum DatabaseEvent {

    #[ret(bool)]
    PlayerAdd {
        ei: String,
        user: i64
    },

    #[ret(Vec<Player>)]
    PlayerQuery {
        id: Option<i64>,
    },

    #[ret(Option<Player>)]
    PlayerQueryEI {
        ei: String,
    },

    PlayerRemove {
        ei: String,
    },

    PlayerUpdate{
        ei: String,
        disabled: bool,
    },

    PlayerNameUpdate {
        ei: String,
        name: String,
    },

    PlayerTimestampReset {
        ei: String,
    },
    PlayerMissionReset {
        ei: String,
        limit: usize,
    },
    PlayerStatusReset {
        ei: String,
        disabled: bool,
    },

    MissionAdd {
        id: String,
        name: String,
        belong: String,
        land: i64
    },

    #[ret(Vec<SpaceShip>)]
    MissionQuery,
    #[ret(HashMap<Player, Vec<SpaceShip>>)]
    MissionQueryByUser { id: i64 },
    #[ret(Vec<SpaceShip>)]
    MissionQueryByPlayer { ei: String },
    MissionUpdated { id: String },

    #[ret(Option<SpaceShip>)]
    MissionSingleQuery { identifier: String },


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
            DatabaseEvent::PlayerAdd {
                ei,
                user,
                __private_sender,
            } => {
                __private_sender
                    .send(database.insert_player(&ei, user).await?)
                    .ok();
            }
            DatabaseEvent::PlayerQuery {
                id,
                __private_sender,
            } => {
                match id {
                    Some(id) => __private_sender.send(database.query_ei(id).await?),
                    None => __private_sender.send(database.query_all_player().await?),
                }
                .ok();
            }
            DatabaseEvent::PlayerRemove { ei } => {
                database.delete_player(&ei).await?;
            }
            DatabaseEvent::PlayerUpdate { ei, disabled } => {
                database
                    .set_player_status(&ei, kstool::time::get_current_second() as i64, disabled)
                    .await?;
            }
            DatabaseEvent::MissionAdd {
                belong,
                name,
                id,
                land,
            } => {
                database.insert_spaceship(id, name, belong, land).await?;
            }
            DatabaseEvent::MissionQuery(sender) => {
                let r = database
                    .query_spaceship_by_time(kstool::time::get_current_second() as i64)
                    .await?;
                sender.send(r).ok();
            }
            DatabaseEvent::PlayerQueryEI {
                ei,
                __private_sender,
            } => {
                let r = database.query_ei_by_player(&ei).await?;
                __private_sender.send(r).ok();
            }
            DatabaseEvent::Terminate => {
                unreachable!()
            }
            DatabaseEvent::MissionUpdated { id } => {
                database.mark_spaceship(&id).await?;
            }
            DatabaseEvent::PlayerNameUpdate { ei, name } => {
                database.set_player_nickname(&ei, &name).await?;
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
                __private_sender,
            } => {
                let mut map = HashMap::new();
                for player in database.query_ei(id).await? {
                    let missions = database.query_spaceship_by_ei(player.ei()).await?;
                    map.insert(player, missions);
                }
                __private_sender.send(map).ok();
            }
            DatabaseEvent::MissionQueryByPlayer {
                ei,
                __private_sender,
            } => {
                __private_sender
                    .send(database.query_spaceship_by_ei(&ei).await?)
                    .ok();
            }
            DatabaseEvent::PlayerTimestampReset { ei } => {
                database.player_timestamp_reset(&ei).await?;
            }
            DatabaseEvent::PlayerMissionReset { ei, limit } => {
                database.player_mission_reset(&ei, limit).await?;
            }
            DatabaseEvent::PlayerStatusReset { ei, disabled } => {
                database.player_status_reset(&ei, disabled).await?;
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
pub use v1 as current;

use crate::types::{Player, SpaceShip};
