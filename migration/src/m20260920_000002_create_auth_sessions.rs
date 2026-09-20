use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Adopt tables created by the previous schema-sync setup without replacing data.
        manager
            .create_table(
                Table::create()
                    .table("auth_sessions")
                    .if_not_exists()
                    .col(ColumnDef::new("id").string().not_null().primary_key())
                    .col(ColumnDef::new("data").json().not_null())
                    .col(ColumnDef::new("expires_at").big_integer().not_null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-auth_sessions-expires_at")
                    .table("auth_sessions")
                    .col("expires_at")
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("auth_sessions").to_owned())
            .await
    }
}
