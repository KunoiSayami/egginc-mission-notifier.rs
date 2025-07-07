use log::info;
use sqlx::SqliteConnection;

pub const VERSION: &str = "7";
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

        CREATE TABLE "subscriber" (
            "contract"	TEXT NOT NULL,
            "room"	TEXT NOT NULL,
            "users"	TEXT NOT NULL,
            "est"	INTEGER NOT NULL,
	        "notified"	INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY("contract", "room")
        );
    "#;

const MERGE_STATEMENT_STAGE: &str = r#"
        CREATE TABLE "subscriber" (
            "contract"	TEXT NOT NULL,
            "room"	TEXT NOT NULL,
            "users"	TEXT NOT NULL,
            "est"	INTEGER NOT NULL,
	        "notified"	INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY("contract", "room")
        );

        UPDATE "meta" SET "value" = '7' WHERE "key" = 'version';
    "#;

pub async fn merge_v6(conn: &mut SqliteConnection) -> sqlx::Result<()> {
    info!("Performing database prepare stage (v7)");
    sqlx::raw_sql(MERGE_STATEMENT_STAGE)
        .execute(&mut *conn)
        .await?;

    info!("Merge completed");
    Ok(())
}
