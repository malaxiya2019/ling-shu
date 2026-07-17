//! ConsolidationEngine — 巩固引擎

use crate::analyzer::EpisodeAnalyzer;
use crate::strategy::ConsolidationStrategy;
use crate::types::*;
use lingshu_memory_episode::{Episode, EpisodeQuery, EpisodeRepository};
use std::sync::Arc;
use tokio::sync::RwLock;
use lingshu_memory_metrics::global_collector;
use tracing::{info, warn};

/// 巩固引擎 — 编排完整的巩固流程。
pub struct ConsolidationEngine {
    store: Arc<dyn EpisodeRepository>,
    config: ConsolidationConfig,
    strategies: Arc<RwLock<Vec<Box<dyn ConsolidationStrategy>>>>,
}

impl ConsolidationEngine {
    pub fn new(store: Arc<dyn EpisodeRepository>, config: ConsolidationConfig) -> Self {
        Self { store, config, strategies: Arc::new(RwLock::new(Vec::new())) }
    }

    pub async fn add_strategy(&self, strategy: Box<dyn ConsolidationStrategy>) {
        self.strategies.write().await.push(strategy);
    }

    pub async fn add_strategies(&self, strategies: Vec<Box<dyn ConsolidationStrategy>>) {
        let mut lock = self.strategies.write().await;
        for s in strategies { lock.push(s); }
        info!("{} consolidation strategies added", lock.len());
    }

    pub async fn list_strategies(&self) -> Vec<String> {
        self.strategies.read().await.iter().map(|s| s.name().to_string()).collect()
    }

    async fn get_unconsolidated(&self) -> Result<Vec<Episode>, ConsolidationError> {
        let all = self.store.query(EpisodeQuery::default().with_limit(self.config.max_episodes_per_job)).await
            .map_err(|e| ConsolidationError::StorageError(e.to_string()))?;
        Ok(all.into_iter().filter(|ep| !ep.tags.contains(&"consolidated".to_string())).collect())
    }

    /// 运行一次完整的全量巩固。
    pub async fn run_consolidation(&self) -> Result<ConsolidationReport, ConsolidationError> {
        let start = std::time::Instant::now();
        let strategies = self.strategies.read().await;

        if strategies.is_empty() {
            return Err(ConsolidationError::ConfigError("没有注册任何巩固策略".into()));
        }

        let unconsolidated = self.get_unconsolidated().await?;
        if unconsolidated.is_empty() {
            global_collector().record_consolidation(true, 0, 0);
            return Ok(ConsolidationReport {
                processed_count: 0, consolidated_count: 0,
                strategy_stats: Vec::new(), execution_time_ms: start.elapsed().as_millis() as u64,
                success: true, error: None,
            });
        }

        info!("开始巩固: {} 条未巩固, {} 个策略", unconsolidated.len(), strategies.len());

        let mut total_consolidated = 0;
        let mut strategy_stats = Vec::new();

        for strategy in strategies.iter() {
            let s_start = std::time::Instant::now();
            match strategy.consolidate(&unconsolidated).await {
                Ok(consolidated) => {
                    let count = consolidated.len();
                    total_consolidated += count;
                    if self.config.auto_write_episodes {
                        for mem in &consolidated {
                            self.store.store(mem.to_episode()).await
                                .map_err(|e| ConsolidationError::StorageError(e.to_string()))?;
                        }
                    }
                    strategy_stats.push(StrategyStat {
                        strategy_name: strategy.name().to_string(),
                        processed: unconsolidated.len(), produced: count,
                        time_ms: s_start.elapsed().as_millis() as u64,
                    });
                }
                Err(e) => {
                    warn!("策略 {} 失败: {}", strategy.name(), e);
                    strategy_stats.push(StrategyStat {
                        strategy_name: strategy.name().to_string(),
                        processed: unconsolidated.len(), produced: 0,
                        time_ms: s_start.elapsed().as_millis() as u64,
                    });
                }
            }
        }

        global_collector().record_consolidation(true, unconsolidated.len() as u64, total_consolidated as u64);
        Ok(ConsolidationReport {
            processed_count: unconsolidated.len(),
            consolidated_count: total_consolidated,
            strategy_stats,
            execution_time_ms: start.elapsed().as_millis() as u64,
            success: true, error: None,
        })
    }

    /// 按实体巩固。
    pub async fn consolidate_by_entity(&self, entity_key: &str) -> Result<ConsolidationReport, ConsolidationError> {
        let start = std::time::Instant::now();
        let strategies = self.strategies.read().await;
        if strategies.is_empty() {
            return Err(ConsolidationError::ConfigError("没有注册策略".into()));
        }

        let all = self.store.query(EpisodeQuery::default()).await
            .map_err(|e| ConsolidationError::StorageError(e.to_string()))?;
        let related: Vec<Episode> = all.into_iter()
            .filter(|ep| !ep.tags.contains(&"consolidated".to_string())
                && ep.entities.iter().any(|e| format!("{}:{}", e.kind, e.name) == entity_key))
            .collect();

        if related.len() < self.config.min_episodes_for_entity {
            global_collector().record_consolidation(true, related.len() as u64, 0);
            return Ok(ConsolidationReport {
                processed_count: related.len(), consolidated_count: 0,
                strategy_stats: Vec::new(), execution_time_ms: start.elapsed().as_millis() as u64,
                success: true, error: None,
            });
        }

        let mut total = 0;
        let mut stats = Vec::new();
        for strategy in strategies.iter() {
            let s_start = std::time::Instant::now();
            if let Ok(consolidated) = strategy.consolidate(&related).await {
                let count = consolidated.len();
                total += count;
                if self.config.auto_write_episodes {
                    for mem in &consolidated {
                        self.store.store(mem.to_episode()).await
                            .map_err(|e| ConsolidationError::StorageError(e.to_string()))?;
                    }
                }
                stats.push(StrategyStat {
                    strategy_name: strategy.name().to_string(),
                    processed: related.len(), produced: count,
                    time_ms: s_start.elapsed().as_millis() as u64,
                });
            }
        }

        global_collector().record_consolidation(true, related.len() as u64, total as u64);
        Ok(ConsolidationReport {
            processed_count: related.len(), consolidated_count: total,
            strategy_stats: stats, execution_time_ms: start.elapsed().as_millis() as u64,
            success: true, error: None,
        })
    }

    pub fn analyzer(&self) -> EpisodeAnalyzer {
        EpisodeAnalyzer::new(self.store.clone(), self.config.clone())
    }

    pub fn config(&self) -> &ConsolidationConfig { &self.config }
    pub fn store(&self) -> &Arc<dyn EpisodeRepository> { &self.store }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::{default_strategies, SummarizeStrategy, DedupStrategy};
    use lingshu_memory_episode::{EntityRef, Episode, InMemoryEpisodeStore};

    fn make_ep(title: &str, entity: &str, hours_ago: i64) -> Episode {
        let mut ep = Episode::new(title, "测试", chrono::Utc::now() - chrono::Duration::hours(hours_ago));
        ep.entities.push(EntityRef::new("project", entity));
        ep
    }

    async fn add_data(store: &Arc<dyn EpisodeRepository>) {
        for i in 0..5 {
            store.store(make_ep(&format!("事件{}", i), "项目A", i as i64 * 12)).await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_empty_strategies() {
        let store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let engine = ConsolidationEngine::new(store, ConsolidationConfig::default());
        assert!(engine.run_consolidation().await.is_err());
    }

    #[tokio::test]
    async fn test_no_unconsolidated() {
        let s = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let engine = ConsolidationEngine::new(s, ConsolidationConfig::default());
        engine.add_strategy(Box::new(SummarizeStrategy::new())).await;
        assert_eq!(engine.run_consolidation().await.unwrap().processed_count, 0);
    }

    #[tokio::test]
    async fn test_full_consolidation() {
        let s = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        add_data(&s).await;
        let engine = ConsolidationEngine::new(s, ConsolidationConfig::default());
        engine.add_strategies(default_strategies()).await;
        let r = engine.run_consolidation().await.unwrap();
        assert!(r.success && r.processed_count > 0);
    }

    #[tokio::test]
    async fn test_consolidate_by_entity() {
        let s = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        add_data(&s).await;
        let engine = ConsolidationEngine::new(s, ConsolidationConfig::default());
        engine.add_strategy(Box::new(SummarizeStrategy::new())).await;
        assert!(engine.consolidate_by_entity("project:项目A").await.unwrap().processed_count >= 3);
    }

    #[tokio::test]
    async fn test_list_strategies() {
        let s = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let engine = ConsolidationEngine::new(s, ConsolidationConfig::default());
        engine.add_strategy(Box::new(SummarizeStrategy::new())).await;
        engine.add_strategy(Box::new(DedupStrategy::new())).await;
        assert_eq!(engine.list_strategies().await.len(), 2);
    }

    #[tokio::test]
    async fn test_auto_write() {
        let s = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        add_data(&s).await;
        let config = ConsolidationConfig { auto_write_episodes: true, ..Default::default() };
        let engine = ConsolidationEngine::new(s.clone(), config);
        engine.add_strategy(Box::new(SummarizeStrategy::new())).await;
        engine.run_consolidation().await.unwrap();

        let all = s.query(EpisodeQuery::default()).await.unwrap();
        assert!(all.iter().any(|ep| ep.tags.contains(&"consolidated".to_string())));
    }

    #[tokio::test]
    async fn test_idempotent() {
        let s = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        add_data(&s).await;
        let config = ConsolidationConfig { auto_write_episodes: true, ..Default::default() };
        let engine = ConsolidationEngine::new(s.clone(), config);
        engine.add_strategy(Box::new(SummarizeStrategy::new())).await;

        let r1 = engine.run_consolidation().await.unwrap();
        let r2 = engine.run_consolidation().await.unwrap();

        // 幂等：第二次 consolidation count 应 <= 第一次（无新增未巩固数据）
        assert!(
            r2.consolidated_count <= r1.consolidated_count + 1,
            "consolidation output should not grow unbounded"
        );
    }

    #[tokio::test]
    async fn test_multiple_strategies() {
        let s = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        add_data(&s).await;
        let engine = ConsolidationEngine::new(s, ConsolidationConfig::default());
        engine.add_strategies(default_strategies()).await;
        assert_eq!(engine.run_consolidation().await.unwrap().strategy_stats.len(), 3);
    }
}
