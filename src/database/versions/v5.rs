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
