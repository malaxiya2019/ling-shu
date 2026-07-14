//! 🌐 LingShu 分布式调度基础示例
//!
//! 演示：创建集群 → 启动调度器 → 提交任务 → 监控负载 → 停止

use lingshu_distributed::{
    Cluster, ClusterConfig, DistScheduleStrategy, DistScheduler, DistSchedulerConfig, DistTask,
};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    println!("🌐 LingShu 分布式调度示例");
    println!("=========================\n");

    // 1. 创建集群配置
    let cluster_config = ClusterConfig {
        node_id: "node-1".into(),
        bind_addr: "127.0.0.1:0".into(),
        seed_nodes: vec![],
        heartbeat_interval: Duration::from_secs(1),
        suspicion_mult: 3,
        cleanup_interval: Duration::from_secs(10),
        gossip_interval: Duration::from_millis(500),
        gossip_fanout: 3,
    };

    // 2. 创建并启动集群
    let cluster = Arc::new(Cluster::new(cluster_config));
    cluster.start().await;
    println!("✅ 集群已启动 (node-1)");

    // 3. 创建调度器配置 — 使用自适应策略
    let scheduler_config = DistSchedulerConfig {
        strategy: DistScheduleStrategy::Adaptive,
        local_node_id: "node-1".into(),
        max_retries: 3,
        task_timeout_secs: 60,
        node_timeout_secs: 30,
        batch_size: 10,
        enable_auto_failover: true,
        health_check_interval: Duration::from_secs(5),
    };

    // 4. 创建并启动调度器
    let scheduler = DistScheduler::new(scheduler_config, cluster.clone());
    scheduler.start().await;
    println!("✅ 调度器已启动 (策略=Adaptive)");

    // 5. 提交多个任务
    let tasks = vec![
        (
            "数据分析任务",
            "analysis",
            serde_json::json!({"dataset": "sales_2025", "metric": "revenue"}),
        ),
        (
            "模型训练任务",
            "training",
            serde_json::json!({"model": "bert-base", "epochs": 10}),
        ),
        (
            "报告生成任务",
            "report",
            serde_json::json!({"format": "pdf", "template": "monthly"}),
        ),
    ];

    for (name, task_type, payload) in &tasks {
        let task = DistTask::new(*name, *task_type, payload.clone());
        let result = scheduler.submit_task(task).await;
        match result {
            Ok(schedule) => {
                println!(
                    "📋 任务已调度: {} → 节点={}, 是否本地={}",
                    name, schedule.assigned_node_id, schedule.is_local,
                );
            }
            Err(e) => {
                eprintln!("❌ 调度失败: {}", e);
            }
        }
    }

    // 6. 查看集群状态
    let state = cluster.state().read().await;
    println!(
        "\n📊 集群状态: {} 个成员, {} 个存活",
        state.member_count(),
        state.live_members().len(),
    );

    // 7. 获取调度器统计
    let summary = scheduler.summary().await;
    println!(
        "📈 调度器统计: 在线节点={}/{} | 待处理={}, 活跃={} | 运行={}",
        summary.online_nodes,
        summary.total_nodes,
        summary.total_pending_tasks,
        summary.total_active_tasks,
        if summary.is_running {
            "运行中"
        } else {
            "已停止"
        },
    );

    // 8. 获取各节点负载
    let loads = scheduler.get_all_loads().await;
    for load in &loads {
        println!(
            "⚡ 节点 {}: 活跃任务={}, CPU={:.1}%, 内存={:.1}%, 成功率={:.1}%",
            load.node_id,
            load.active_tasks,
            load.cpu_usage * 100.0,
            load.memory_usage * 100.0,
            load.success_rate * 100.0,
        );
    }

    // 9. 停止调度器
    scheduler.stop().await;
    println!("\n👋 调度器已停止");
}
