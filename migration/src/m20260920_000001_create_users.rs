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
                    .table("users")
                    .if_not_exists()
                    .col(ColumnDef::new("id").uuid().not_null().primary_key())
                    .col(ColumnDef::new("email").string().not_null().unique_key())
                    .col(ColumnDef::new("password_hash").text().not_null())
                    .col(ColumnDef::new("created_at").big_integer().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("users").to_owned())
            .await
    }
}
