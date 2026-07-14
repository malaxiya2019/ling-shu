//! 🧠 LingShu 自治引擎基础示例
//!
//! 演示：创建引擎 → 存储经验 → 反思分析 → 进化调整 → 获取报告

use lingshu_autonomy::{
    AgentParameters, AutonomyEngine, EvolutionConfig, ExperienceEntry, ExperienceOutcome,
    ExperienceSeverity, ExperienceType, ReflectionConfig,
};
use std::time::Duration;

#[tokio::main]
async fn main() {
    println!("🧠 LingShu 自治引擎示例");
    println!("========================\n");

    // 1. 创建自治引擎
    let reflection_config = ReflectionConfig {
        confidence_threshold: 0.3,
        failure_pattern_window: 10,
        degradation_window: 20,
        min_experiences_for_reflection: 3,
        auto_mark_analyzed: true,
    };

    let evolution_config = EvolutionConfig {
        auto_apply_threshold: 8,
        verification_wait: Duration::from_secs(60),
        max_concurrent_plans: 5,
        cooldown: Duration::from_secs(300),
        enable_auto_rollback: true,
        rollback_threshold: -0.3,
    };

    let engine = AutonomyEngine::new(
        reflection_config,
        evolution_config,
        1000, // 每 Agent 最大经验数
    );
    println!("✅ 自治引擎已创建");

    // 2. 注册 Agent 参数
    let params = AgentParameters {
        agent_id: "agent-alpha".into(),
        temperature: 0.7,
        max_tokens: 4096,
        timeout_secs: 300,
        max_retries: 3,
        collaboration_strategy: "default".into(),
        default_priority: 5,
        version: 1,
        updated_at: chrono::Utc::now().timestamp(),
    };

    engine
        .evolution_engine
        .register_agent("agent-alpha", params)
        .await;
    println!("✅ Agent 注册完成 (agent-alpha)");

    // 3. 存储成功经验
    for i in 0..5 {
        let entry = ExperienceEntry::new(
            "agent-alpha",
            ExperienceType::TaskExecution,
            format!("成功完成任务 #{}", i + 1),
            "任务执行成功",
            ExperienceOutcome::Success,
        )
        .with_severity(ExperienceSeverity::Info)
        .with_tag("task")
        .with_tag("success")
        .with_duration(1500 + i as u64 * 100);

        engine.experience_store.store(entry).await;
    }
    println!("✅ 已存储 5 条成功经验");

    // 4. 存储失败经验
    for i in 0..2 {
        let entry = ExperienceEntry::new(
            "agent-alpha",
            ExperienceType::TaskExecution,
            format!("任务 #{} 执行超时", i + 1),
            "任务执行超时，需要重试",
            ExperienceOutcome::Failure("timeout".into()),
        )
        .with_severity(ExperienceSeverity::Warning)
        .with_tag("timeout")
        .with_tag("retry")
        .with_context(serde_json::json!({
            "error": "timeout after 30s",
            "retry_count": 2,
        }));

        engine.experience_store.store(entry).await;
    }
    println!("✅ 已存储 2 条失败经验");

    // 5. 获取经验摘要
    let summary = engine.experience_store.summarize("agent-alpha").await;
    println!(
        "\n📊 经验摘要: 总计={}, 成功={}, 失败={}, 成功率={:.1}%",
        summary.total_count,
        summary.success_count,
        summary.failure_count,
        summary.success_rate * 100.0,
    );

    // 6. 执行反思
    let report = engine.reflect_only("agent-alpha").await;
    println!("\n🔍 反思报告:");
    println!("   Agent: {}", report.agent_id);
    println!("   分析经验数: {}", report.analyzed_count);
    println!("   健康评分: {:.1}/1.0", report.health_score);

    for insight in &report.insights {
        println!(
            "   💡 [{}] {} | 优先级={}, 置信度={:.1}",
            insight.insight_type.as_str(),
            insight.title,
            insight.priority,
            insight.confidence,
        );
    }

    // 7. 执行进化
    let outcomes = engine.evolve_only("agent-alpha").await;
    println!("\n🔄 进化结果:");
    for outcome in &outcomes {
        println!(
            "   ⚡ 成功={} | 效果评分={:.2} | {}",
            outcome.success, outcome.effect_score, outcome.observation,
        );
    }

    // 8. 查看更新后的参数
    let updated_params = engine.evolution_engine.get_parameters("agent-alpha").await;
    if let Some(params) = updated_params {
        println!("\n📋 更新后的参数:");
        println!("   temperature = {}", params.temperature);
        println!("   max_tokens = {}", params.max_tokens);
        println!("   timeout_secs = {}", params.timeout_secs);
        println!("   max_retries = {}", params.max_retries);
        println!(
            "   collaboration_strategy = {}",
            params.collaboration_strategy
        );
        println!("   default_priority = {}", params.default_priority);
        println!("   version = {}", params.version);
    }

    println!("\n✅ 自治引擎流程完成");
}
