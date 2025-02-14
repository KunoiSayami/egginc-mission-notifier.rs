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
