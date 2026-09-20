pub use sea_orm_migration::prelude::*;

mod m20260920_000001_create_users;
mod m20260920_000002_create_auth_sessions;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260920_000001_create_users::Migration),
            Box::new(m20260920_000002_create_auth_sessions::Migration),
        ]
    }
}
