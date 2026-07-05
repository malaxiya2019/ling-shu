//! DatabaseRepository — 基于 Database trait 的通用仓储实现.
//!
//! 桥接 `Repository<T>` + `Database`，提供类型安全的 CRUD 操作.
//! 支持任意实现了 `Database` trait 的后端 (Sqlite, Postgres).

use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use lingshu_traits::database::{Database, PaginatedResult, Pagination, QueryFilter};
use lingshu_traits::repository::Repository;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::sync::Arc;

/// 基于 Database 后端的通用仓储实现.
pub struct DatabaseRepository<T> {
    db: Arc<dyn Database>,
    collection: String,
    _marker: PhantomData<T>,
}

impl<T> DatabaseRepository<T> {
    pub fn new(db: Arc<dyn Database>, collection: impl Into<String>) -> Self {
        Self {
            db,
            collection: collection.into(),
            _marker: PhantomData,
        }
    }
}

#[async_trait]
impl<T> Repository<T> for DatabaseRepository<T>
where
    T: Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
{
    fn collection_name(&self) -> &str {
        &self.collection
    }

    async fn insert(&self, ctx: LsContext, entity: T) -> LsResult<T> {
        let value = serde_json::to_value(&entity)?;
        let result = self.db.insert(ctx, &self.collection, value).await?;
        let entity: T = serde_json::from_value(result)?;
        Ok(entity)
    }

    async fn get_by_id(&self, ctx: LsContext, id: &str) -> LsResult<Option<T>> {
        let result = self.db.get_by_id(ctx, &self.collection, id).await?;
        match result {
            Some(value) => {
                let entity: T = serde_json::from_value(value)?;
                Ok(Some(entity))
            }
            None => Ok(None),
        }
    }

    async fn query(
        &self,
        ctx: LsContext,
        filters: Vec<QueryFilter>,
        pagination: Pagination,
    ) -> LsResult<PaginatedResult> {
        self.db
            .query(ctx, &self.collection, filters, pagination)
            .await
    }

    async fn update(&self, ctx: LsContext, id: &str, entity: T) -> LsResult<Option<T>> {
        let value = serde_json::to_value(&entity)?;
        let result = self.db.update(ctx, &self.collection, id, value).await?;
        match result {
            Some(value) => {
                let entity: T = serde_json::from_value(value)?;
                Ok(Some(entity))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, ctx: LsContext, id: &str) -> LsResult<bool> {
        self.db.delete(ctx, &self.collection, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteDatabase;
    use lingshu_core::LsId;
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct User {
        id: Option<String>,
        name: String,
        email: String,
    }

    fn test_repo() -> DatabaseRepository<User> {
        let db = Arc::new(SqliteDatabase::in_memory().unwrap());
        DatabaseRepository::new(db, "test_users_2")
    }

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let repo = test_repo();
        let ctx = test_ctx();
        let user = User {
            id: None,
            name: "Alice".into(),
            email: "alice@example.com".into(),
        };
        let inserted = repo.insert(ctx.child(), user).await.unwrap();
        assert_eq!(inserted.name, "Alice");
        assert!(inserted.id.is_some(), "inserted entity should have an id");

        // Query all to verify
        let all = repo
            .query(
                ctx.child(),
                vec![],
                Pagination {
                    page: 1,
                    page_size: 100,
                },
            )
            .await
            .unwrap();
        assert_eq!(all.total, 1);
    }

    #[tokio::test]
    async fn test_update() {
        let repo = test_repo();
        let ctx = test_ctx();
        let user = User {
            id: None,
            name: "Bob".into(),
            email: "bob@example.com".into(),
        };
        let inserted = repo.insert(ctx.child(), user).await.unwrap();
        let id = inserted.id.clone().unwrap();

        let updated_user = User {
            id: Some(id.clone()),
            name: "Bobby".into(),
            email: "bobby@example.com".into(),
        };
        let result = repo.update(ctx.child(), &id, updated_user).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.as_ref().unwrap().name, "Bobby");
    }

    #[tokio::test]
    async fn test_delete() {
        let repo = test_repo();
        let ctx = test_ctx();
        let user = User {
            id: None,
            name: "Charlie".into(),
            email: "charlie@example.com".into(),
        };
        let inserted = repo.insert(ctx.child(), user).await.unwrap();
        let id = inserted.id.expect("inserted entity should have an id");

        let deleted = repo.delete(ctx.child(), &id).await.unwrap();
        assert!(deleted);

        let found = repo.get_by_id(ctx.child(), &id).await.unwrap();
        assert!(found.is_none());
    }
}
