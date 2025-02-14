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
