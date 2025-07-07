use std::collections::HashMap;

use futures_util::StreamExt;
use itertools::Itertools as _;
use log::info;
use sqlx::SqliteConnection;

use crate::database::types::Account;

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

    let mut querier = sqlx::query_as::<_, (String, i64)>(r#"SELECT "ei", "user" FROM "player""#)
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
