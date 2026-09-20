use migration::{Migrator, MigratorTrait, SchemaManager};
use sea_orm_migration::sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};

async fn database() -> DatabaseConnection {
    let mut options =
        ConnectOptions::new(std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL"));
    options.max_connections(2).sqlx_logging(false);
    let admin = Database::connect(options.clone()).await.unwrap();
    let schema = format!("migration_test_{}", uuid::Uuid::new_v4().simple());
    admin
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    admin.close().await.unwrap();
    options.set_schema_search_path(schema);
    Database::connect(options).await.unwrap()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn fresh_database_applies_each_migration_once() {
    let db = database().await;
    Migrator::up(&db, None).await.unwrap();
    Migrator::up(&db, None).await.unwrap();

    let schema = SchemaManager::new(&db);
    assert!(schema.has_table("users").await.unwrap());
    assert!(schema.has_table("auth_sessions").await.unwrap());
    assert!(
        schema
            .has_index("auth_sessions", "idx-auth_sessions-expires_at")
            .await
            .unwrap()
    );
    assert_eq!(
        Migrator::get_applied_migrations(&db).await.unwrap().len(),
        2
    );
    assert!(
        Migrator::get_pending_migrations(&db)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn rollback_and_reapply_follow_migration_order() {
    let db = database().await;
    Migrator::up(&db, None).await.unwrap();
    Migrator::down(&db, Some(1)).await.unwrap();
    let schema = SchemaManager::new(&db);
    assert!(schema.has_table("users").await.unwrap());
    assert!(!schema.has_table("auth_sessions").await.unwrap());
    assert_eq!(
        Migrator::get_applied_migrations(&db).await.unwrap().len(),
        1
    );

    Migrator::down(&db, Some(1)).await.unwrap();
    assert!(!schema.has_table("users").await.unwrap());
    Migrator::up(&db, None).await.unwrap();
    assert!(schema.has_table("users").await.unwrap());
    assert!(schema.has_table("auth_sessions").await.unwrap());
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn existing_schema_and_data_are_preserved() {
    let db = database().await;
    db.execute_unprepared(include_str!("fixtures/legacy_schema.sql"))
        .await
        .unwrap();
    Migrator::up(&db, None).await.unwrap();

    let row = db
        .query_one_raw(sea_orm_migration::sea_orm::Statement::from_string(
            sea_orm_migration::sea_orm::DbBackend::Postgres,
            "SELECT email, password_hash FROM users",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<String>("", "email").unwrap(),
        "existing@example.com"
    );
    assert_eq!(
        row.try_get::<String>("", "password_hash").unwrap(),
        "existing-hash"
    );
    let row = db
        .query_one_raw(sea_orm_migration::sea_orm::Statement::from_string(
            sea_orm_migration::sea_orm::DbBackend::Postgres,
            "SELECT id FROM auth_sessions",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "id").unwrap(), "existing-session");
    assert_eq!(
        Migrator::get_applied_migrations(&db).await.unwrap().len(),
        2
    );
}
