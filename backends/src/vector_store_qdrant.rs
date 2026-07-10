//! QdrantVector — Qdrant 高性能向量数据库集成.

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::vector_store::*;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, DeleteCollectionBuilder, UpsertPointsBuilder, SearchPointsBuilder,
    PointStruct, Distance, VectorParamsBuilder,
};
use qdrant_client::Qdrant;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::RwLock;
use tracing::{debug, info};

/// Qdrant 高性能向量数据库后端.
pub struct QdrantVector {
    client: Qdrant,
    collection_names: RwLock<HashMap<LsId, String>>,
}

impl QdrantVector {
    pub async fn new(url: &str, api_key: Option<&str>) -> LsResult<Self> {
        let mut config = qdrant_client::config::QdrantConfig::from_url(url);
        if let Some(key) = api_key {
            config = config.api_key(key);
        }
        let client = Qdrant::new(config)
            .map_err(|e| LsError::Storage(format!("qdrant connect failed: {e}")))?;
        info!("Qdrant connected: {url}");
        Ok(Self {
            client,
            collection_names: RwLock::new(HashMap::new()),
        })
    }

    pub async fn from_env() -> LsResult<Self> {
        let url = std::env::var("QDRANT_URL")
            .unwrap_or_else(|_| "http://localhost:6334".to_string());
        let api_key = std::env::var("QDRANT_API_KEY").ok();
        Self::new(&url, api_key.as_deref()).await
    }

    fn collection_name(id: &LsId) -> String {
        format!("ls_{}", id.to_string().split('-').next().unwrap_or("col"))
    }

    fn records_to_points(records: Vec<VectorRecord>) -> Vec<PointStruct> {
        records.into_iter().map(|rec| {
            let pid = rec.id.to_string();
            let payload: HashMap<String, Value> = match &rec.metadata {
                Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                other => {
                    let mut m = HashMap::new();
                    m.insert("data".to_string(), other.clone());
                    m
                }
            };
            PointStruct::new(pid, rec.vector, payload)
        }).collect()
    }
}

impl std::fmt::Debug for QdrantVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.collection_names.read().map(|m| m.len()).unwrap_or(0);
        f.debug_struct("QdrantVector").field("collections", &count).finish()
    }
}

#[async_trait]
impl VectorStore for QdrantVector {
    async fn create_collection(&self, _ctx: LsContext, _name: &str, dimensions: usize) -> LsResult<LsId> {
        let id = LsId::new();
        let col_name = Self::collection_name(&id);
        self.client
            .create_collection(
                CreateCollectionBuilder::new(col_name.clone())
                    .vectors_config(
                        VectorParamsBuilder::new(dimensions as u64, Distance::Cosine)
                    ),
            )
            .await
            .map_err(|e| LsError::Storage(format!("qdrant create_collection failed: {e}")))?;
        self.collection_names.write()
            .map_err(|e| LsError::Internal(format!("rwlock poisoned: {e}")))?
            .insert(id, col_name.clone());
        info!("qdrant collection created: {col_name}");
        Ok(id)
    }

    async fn delete_collection(&self, _ctx: LsContext, collection_id: LsId) -> LsResult<()> {
        let col_name = {
            let guard = self.collection_names.read()
                .map_err(|e| LsError::Internal(format!("rwlock poisoned: {e}")))?;
            guard.get(&collection_id).cloned()
        };
        if let Some(name) = col_name {
            self.client.delete_collection(DeleteCollectionBuilder::new(name.clone()))
                .await.map_err(|e| LsError::Storage(format!("qdrant delete_collection failed: {e}")))?;
            self.collection_names.write()
                .map_err(|e| LsError::Internal(format!("rwlock poisoned: {e}")))?
                .remove(&collection_id);
            info!("qdrant collection deleted: {name}");
        }
        Ok(())
    }

    async fn upsert(&self, _ctx: LsContext, collection_id: LsId, records: Vec<VectorRecord>) -> LsResult<()> {
        let col_name = {
            let guard = self.collection_names.read()
                .map_err(|e| LsError::Internal(format!("rwlock poisoned: {e}")))?;
            guard.get(&collection_id).cloned()
                .ok_or_else(|| LsError::NotFound(format!("collection {collection_id}")))?
        };
        let points = Self::records_to_points(records);
        let count = points.len();
        self.client.upsert_points(UpsertPointsBuilder::new(col_name.clone(), points))
            .await.map_err(|e| LsError::Storage(format!("qdrant upsert failed: {e}")))?;
        debug!("upserted {count} points into {col_name}");
        Ok(())
    }

    async fn search(&self, _ctx: LsContext, collection_id: LsId, query: Vec<f32>, top_k: u64) -> LsResult<VectorSearchResult> {
        let col_name = {
            let guard = self.collection_names.read()
                .map_err(|e| LsError::Internal(format!("rwlock poisoned: {e}")))?;
            guard.get(&collection_id).cloned()
                .ok_or_else(|| LsError::NotFound(format!("collection {collection_id}")))?
        };
        let search_result = self.client
            .search_points(SearchPointsBuilder::new(col_name.clone(), query, top_k))
            .await
            .map_err(|e| LsError::Storage(format!("qdrant search failed: {e}")))?;

        let records: Vec<VectorRecord> = search_result.result.into_iter().map(|sp| {
            let metadata = sp.payload.into_iter().fold(
                serde_json::Map::new(),
                |mut acc, (k, v)| { acc.insert(k, convert_qdrant_value(&v)); acc },
            );
            let point_id = sp.id.map(|pid| { use qdrant_client::qdrant::point_id::PointIdOptions; match pid.point_id_options { Some(PointIdOptions::Uuid(u)) => u.parse::<LsId>().unwrap_or_else(|_| LsId::new()), Some(PointIdOptions::Num(_)) => LsId::new(), None => LsId::new(), } }).unwrap_or_else(LsId::new);
            VectorRecord {
                id: point_id,
                vector: vec![],
                metadata: Value::Object(metadata),
                score: Some(sp.score as f64),
            }
        }).collect();

        Ok(VectorSearchResult { total: records.len() as u64, records })
    }
}

fn convert_qdrant_value(v: &qdrant_client::qdrant::Value) -> Value {
    use qdrant_client::qdrant::value::Kind;
    match &v.kind {
        Some(Kind::NullValue(_)) => Value::Null,
        Some(Kind::DoubleValue(n)) => Value::from(*n),
        Some(Kind::IntegerValue(n)) => Value::from(*n),
        Some(Kind::StringValue(s)) => Value::from(s.clone()),
        Some(Kind::BoolValue(b)) => Value::from(*b),
        Some(Kind::ListValue(list)) => Value::Array(list.values.iter().map(convert_qdrant_value).collect()),
        Some(Kind::StructValue(s)) => Value::Object(s.fields.iter().map(|(k, v)| (k.clone(), convert_qdrant_value(v))).collect()),
        None => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_create_and_delete_collection() {
        let store = QdrantVector::from_env().await.unwrap();
        let ctx = LsContext::with_session(LsId::new());
        let id = store.create_collection(ctx.clone(), "test", 384).await.unwrap();
        assert!(store.search(ctx.clone(), id, vec![0.1; 384], 10).await.is_ok());
        store.delete_collection(ctx.clone(), id).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_upsert_and_search() {
        let store = QdrantVector::from_env().await.unwrap();
        let ctx = LsContext::with_session(LsId::new());
        let col_id = store.create_collection(ctx.clone(), "vectors", 4).await.unwrap();
        let records = vec![
            VectorRecord { id: LsId::new(), vector: vec![1.0, 0.0, 0.0, 0.0], metadata: serde_json::json!({"label":"A"}), score: None },
            VectorRecord { id: LsId::new(), vector: vec![0.0, 1.0, 0.0, 0.0], metadata: serde_json::json!({"label":"B"}), score: None },
        ];
        store.upsert(ctx.clone(), col_id, records).await.unwrap();
        let result = store.search(ctx.clone(), col_id, vec![1.0, 0.0, 0.0, 0.0], 2).await.unwrap();
        assert_eq!(result.records.len(), 2);
    }
}
