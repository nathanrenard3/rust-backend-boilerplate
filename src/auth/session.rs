use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set, SqlErr,
    sea_query::Expr,
};
use time::OffsetDateTime;
use tower_sessions::{
    SessionStore,
    session::{Id, Record},
    session_store::{self, ExpiredDeletion},
};

pub(crate) mod model {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "auth_sessions")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub data: Json,
        #[sea_orm(indexed)]
        pub expires_at: i64,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}

#[derive(Clone, Debug)]
// Persistence adapter only; tower-sessions manages the session lifecycle.
pub struct SeaOrmSessionStore(pub DatabaseConnection);

fn backend_error(_: sea_orm::DbErr) -> session_store::Error {
    session_store::Error::Backend("Session database operation failed".to_owned())
}

fn encode(record: &Record) -> session_store::Result<serde_json::Value> {
    serde_json::to_value(&record.data)
        .map_err(|error| session_store::Error::Encode(error.to_string()))
}

#[async_trait]
impl SessionStore for SeaOrmSessionStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        loop {
            let result = model::ActiveModel {
                id: Set(record.id.to_string()),
                data: Set(encode(record)?),
                expires_at: Set(record.expiry_date.unix_timestamp()),
            }
            .insert(&self.0)
            .await;
            match result {
                Ok(_) => return Ok(()),
                Err(error)
                    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) =>
                {
                    // On collision, generate a new ID rather than overwrite another session.
                    record.id = Id::default()
                }
                Err(error) => return Err(backend_error(error)),
            }
        }
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        // No upsert: a concurrent request must not recreate a revoked session.
        let result = model::Entity::update_many()
            .col_expr(model::Column::Data, Expr::value(encode(record)?))
            .col_expr(
                model::Column::ExpiresAt,
                Expr::value(record.expiry_date.unix_timestamp()),
            )
            .filter(model::Column::Id.eq(record.id.to_string()))
            .filter(model::Column::ExpiresAt.gt(OffsetDateTime::now_utc().unix_timestamp()))
            .exec(&self.0)
            .await
            .map_err(backend_error)?;
        if result.rows_affected != 1 {
            return Err(session_store::Error::Backend(
                "Session expired or revoked".to_owned(),
            ));
        }
        Ok(())
    }

    async fn load(&self, id: &Id) -> session_store::Result<Option<Record>> {
        // Reject expired sessions immediately, without waiting for periodic row cleanup.
        let row = model::Entity::find_by_id(id.to_string())
            .filter(model::Column::ExpiresAt.gt(OffsetDateTime::now_utc().unix_timestamp()))
            .one(&self.0)
            .await
            .map_err(backend_error)?;
        row.map(|row| {
            Ok(Record {
                id: *id,
                data: serde_json::from_value(row.data)
                    .map_err(|error| session_store::Error::Decode(error.to_string()))?,
                expiry_date: OffsetDateTime::from_unix_timestamp(row.expires_at)
                    .map_err(|error| session_store::Error::Decode(error.to_string()))?,
            })
        })
        .transpose()
    }

    async fn delete(&self, id: &Id) -> session_store::Result<()> {
        model::Entity::delete_by_id(id.to_string())
            .exec(&self.0)
            .await
            .map_err(backend_error)?;
        Ok(())
    }
}

#[async_trait]
impl ExpiredDeletion for SeaOrmSessionStore {
    async fn delete_expired(&self) -> session_store::Result<()> {
        model::Entity::delete_many()
            .filter(model::Column::ExpiresAt.lte(OffsetDateTime::now_utc().unix_timestamp()))
            .exec(&self.0)
            .await
            .map_err(backend_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires a disposable PostgreSQL database in TEST_DATABASE_URL"]
    async fn store_handles_collisions_expiry_and_revocation() {
        let db = sea_orm::Database::connect(std::env::var("TEST_DATABASE_URL").unwrap())
            .await
            .unwrap();
        use migration::MigratorTrait;
        migration::Migrator::up(&db, None).await.unwrap();
        let store = SeaOrmSessionStore(db);
        let mut first = Record {
            id: Id::default(),
            data: Default::default(),
            expiry_date: OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        first
            .data
            .insert("owner".to_owned(), serde_json::json!("first"));
        store.create(&mut first).await.unwrap();
        let mut collision = first.clone();
        collision
            .data
            .insert("owner".to_owned(), serde_json::json!("second"));
        store.create(&mut collision).await.unwrap();
        assert_ne!(first.id, collision.id);
        assert_eq!(
            store.load(&first.id).await.unwrap().unwrap().data["owner"],
            "first"
        );

        first
            .data
            .insert("updated".to_owned(), serde_json::json!(true));
        store.save(&first).await.unwrap();
        assert_eq!(
            store.load(&first.id).await.unwrap().unwrap().data["updated"],
            true
        );
        store.delete(&first.id).await.unwrap();
        assert!(store.save(&first).await.is_err());
        assert!(store.load(&first.id).await.unwrap().is_none());

        collision.expiry_date = OffsetDateTime::now_utc() - time::Duration::seconds(1);
        store.save(&collision).await.unwrap();
        assert!(store.load(&collision.id).await.unwrap().is_none());
        assert!(store.save(&collision).await.is_err());
        store.delete_expired().await.unwrap();
        assert!(
            model::Entity::find_by_id(collision.id.to_string())
                .one(&store.0)
                .await
                .unwrap()
                .is_none()
        );
    }
}
