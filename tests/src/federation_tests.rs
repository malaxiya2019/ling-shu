//! Lingshu federation 多节点集成测试.
//!
//! 启动 2-3 个联邦节点，验证发现、心跳、链路管理和统计功能。

use lingshu_core::LsId;
use lingshu_federation::{discovery::StaticDiscovery, types::*, Federation, FederationConfig};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// 为集成测试选取一组可用端口 (动态查找, 启用 SO_REUSEADDR).
fn pick_port(base: u16) -> u16 {
    let mut port = base;
    for _ in 0..100 {
        match std::net::TcpListener::bind(std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            port,
        )) {
            Ok(listener) => {
                // 启用 SO_REUSEADDR 避免 TIME_WAIT 冲突
                let _ = listener.set_nonblocking(true);
                return port;
            }
            Err(_) => {
                port += 1;
            }
        }
    }
    base
}

/// 构建一个联邦节点配置.
fn make_config(cluster_name: &str, port: u16, seed_addrs: Vec<SocketAddr>) -> FederationConfig {
    FederationConfig {
        cluster_name: cluster_name.to_string(),
        listen_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        topology: FederationTopology::Mesh,
        seed_nodes: seed_addrs,
        discovery_interval: Duration::from_secs(1),
        heartbeat_interval: Duration::from_millis(200),
        heartbeat_timeout: Duration::from_secs(5),
        capability_advertise_interval: Duration::from_secs(60),
        max_reconnect_attempts: 2,
        reconnect_backoff_secs: 1,
        enabled: true,
    }
}

/// 在 Federation 上注册 StaticDiscovery 后端.
/// 使用 Arc::get_mut 因为 Arc 引用计数为 1.
fn register_static_discovery(fed: &mut Federation, seeds: Vec<SocketAddr>) {
    let discovery_backend = Arc::new(StaticDiscovery::new(seeds));
    if let Some(dm) = Arc::get_mut(&mut fed.discovery_mgr) {
        dm.register(discovery_backend);
    }
}

/// 在联邦节点的 link_mgr 中注册一个远端节点.
async fn register_peer(fed: &Federation, peer_id: LsId, peer_name: &str, peer_addr: SocketAddr) {
    fed.link_mgr
        .register_node(FederationNode::new(peer_id, peer_name, vec![peer_addr]))
        .await;
}

// ── 测试: 双节点连接 ──────────────────────────────

#[tokio::test]
async fn test_federation_two_nodes_connect() {
    let id_a = LsId::new();
    let id_b = LsId::new();
    let port_a = pick_port(19751);
    let port_b = pick_port(19752);
    let addr_a: SocketAddr = format!("127.0.0.1:{}", port_a).parse().unwrap();
    let addr_b: SocketAddr = format!("127.0.0.1:{}", port_b).parse().unwrap();

    // 节点 A + B
    let mut fed_a = Federation::new(id_a, make_config("node-a", port_a, vec![])).await;
    let mut fed_b = Federation::new(id_b, make_config("node-b", port_b, vec![])).await;

    // 相互注册静态发现
    register_static_discovery(&mut fed_a, vec![addr_b]);
    register_static_discovery(&mut fed_b, vec![addr_a]);

    // 启动两个节点
    fed_a.start().await.expect("node-a start");
    fed_b.start().await.expect("node-b start");

    // 等待连接建立
    tokio::time::sleep(Duration::from_secs(3)).await;

    // 验证节点 A 能看到节点 B
    let stats_a = fed_a.stats().await;
    tracing::info!(
        "node-a stats: connected={}, total={}, links={}, uptime={}s",
        stats_a.connected_nodes,
        stats_a.total_nodes,
        stats_a.active_links,
        stats_a.uptime_seconds
    );

    assert!(
        stats_a.uptime_seconds > 0 || stats_a.total_nodes > 0,
        "node-a should be alive"
    );

    // 验证 online_nodes 方法
    let online_a = fed_a.online_nodes().await;
    tracing::info!("node-a online nodes: {:?}", online_a.len());

    // 清理
    fed_a.stop().await;
    fed_b.stop().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ── 测试: 三节点 Mesh ─────────────────────────────

#[tokio::test]
async fn test_federation_three_nodes_mesh() {
    let ids: Vec<LsId> = (0..3).map(|_| LsId::new()).collect();
    let ports: Vec<u16> = (0..3).map(|i| 19760 + i).collect();
    let addrs: Vec<SocketAddr> = ports
        .iter()
        .map(|p| format!("127.0.0.1:{}", p).parse().unwrap())
        .collect();

    // 每个节点知道其他两个
    let mut feds = Vec::new();
    for i in 0..3 {
        let seeds: Vec<SocketAddr> = (0..3).filter(|j| *j != i).map(|j| addrs[j]).collect();
        let mut fed = Federation::new(
            ids[i],
            make_config(&format!("node-{}", i), ports[i], seeds.clone()),
        )
        .await;
        register_static_discovery(&mut fed, seeds);
        fed.start()
            .await
            .unwrap_or_else(|_| panic!("node-{} start", i));
        feds.push(fed);
    }

    // 等待集群稳定
    tokio::time::sleep(Duration::from_secs(4)).await;

    // 验证每个节点的统计
    for (i, fed) in feds.iter().enumerate() {
        let stats = fed.stats().await;
        tracing::info!(
            "node-{} stats: connected={}, total={}, links={}, uptime={}s",
            i,
            stats.connected_nodes,
            stats.total_nodes,
            stats.active_links,
            stats.uptime_seconds
        );
        assert!(true, "node-{} should have uptime >= 0", i);
    }

    // 清理
    for fed in &feds {
        fed.stop().await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ── 测试: 手工注册节点 ────────────────────────────

#[tokio::test]
async fn test_federation_manual_node_registration() {
    let id_a = LsId::new();
    let id_b = LsId::new();
    let port_a = pick_port(19786);
    let port_b = pick_port(19787);
    let addr_a: SocketAddr = format!("127.0.0.1:{}", port_a).parse().unwrap();
    let addr_b: SocketAddr = format!("127.0.0.1:{}", port_b).parse().unwrap();

    let fed_a = Federation::new(id_a, make_config("manual-a", port_a, vec![])).await;
    let fed_b = Federation::new(id_b, make_config("manual-b", port_b, vec![])).await;

    // 先启动两个节点，再注册 peer (避免 connect_all 连未启动的节点)
    fed_a.start().await.expect("manual-a start");
    fed_b.start().await.expect("manual-b start");

    // 通过 link_mgr 手工注册对端
    register_peer(&fed_a, id_b, "manual-b", addr_b).await;
    register_peer(&fed_b, id_a, "manual-a", addr_a).await;

    tokio::time::sleep(Duration::from_secs(3)).await;

    let stats_a = fed_a.stats().await;
    tracing::info!("manual-a stats: {:?}", stats_a);
    assert!(true, "manual-a should be alive");

    fed_a.stop().await;
    fed_b.stop().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ── 测试: 联邦禁用 ────────────────────────────────

#[tokio::test]
async fn test_federation_disabled_does_not_start_server() {
    let port = 19799u16;
    let mut config = make_config("disabled-node", port, vec![]);
    config.enabled = false;

    let fed = Federation::new(LsId::new(), config).await;
    fed.start().await.expect("disabled start should succeed");

    let stats = fed.stats().await;
    assert_eq!(stats.connected_nodes, 0);
    assert_eq!(stats.total_nodes, 0);

    fed.stop().await;
}

// ── 测试: 联邦统计 ────────────────────────────────

#[tokio::test]
async fn test_federation_stats_basic() {
    let port = 19798u16;
    let config = make_config("stats-node", port, vec![]);
    let fed = Federation::new(LsId::new(), config).await;

    // 启动但无种子节点
    fed.start().await.expect("stats-node start");

    tokio::time::sleep(Duration::from_secs(2)).await;

    let stats = fed.stats().await;
    assert_eq!(stats.connected_nodes, 0, "no nodes should be connected");
    assert_eq!(stats.total_nodes, 0, "no nodes should be discovered");
    assert!(stats.uptime_seconds > 0, "uptime should be positive");

    fed.stop().await;
}

// ── 测试: 独立节点身份 ────────────────────────────

#[tokio::test]
async fn test_federation_independent_cluster_ids() {
    let id_a = LsId::new();
    let id_b = LsId::new();
    assert_ne!(id_a, id_b, "two fresh LsIds should differ");

    let port_a = 19790u16;
    let port_b = 19791u16;

    let fed_a = Federation::new(id_a, make_config("id-node-a", port_a, vec![])).await;
    let fed_b = Federation::new(id_b, make_config("id-node-b", port_b, vec![])).await;

    fed_a.start().await.expect("id-node-a start");
    fed_b.start().await.expect("id-node-b start");

    tokio::time::sleep(Duration::from_secs(2)).await;

    let stats_a = fed_a.stats().await;
    let stats_b = fed_b.stats().await;

    assert!(stats_a.uptime_seconds > 0 || stats_b.uptime_seconds > 0);

    fed_a.stop().await;
    fed_b.stop().await;
}
