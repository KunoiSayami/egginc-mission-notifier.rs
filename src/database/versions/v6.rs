use log::info;
use sqlx::SqliteConnection;

use crate::database::types::ContractCache;

pub const VERSION: &str = "6";

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
