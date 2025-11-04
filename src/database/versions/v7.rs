use log::info;
use sqlx::SqliteConnection;

pub const VERSION: &str = "7";

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
