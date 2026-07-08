"""LingShu SDK — 数据模型."""

from dataclasses import dataclass, field
from typing import Any, Optional


@dataclass
class ChatMessage:
    role: str  # user | assistant | system
    content: str


@dataclass
class ChatRequest:
    model: str = "default"
    messages: list[ChatMessage] = field(default_factory=list)
    temperature: Optional[float] = None
    max_tokens: Optional[int] = None
    stream: bool = False


@dataclass
class ChatResponse:
    id: str
    content: str
    model: str
    usage: dict[str, int] = field(default_factory=dict)


@dataclass
class AgentRequest:
    agent_id: str
    input: str
    config: dict[str, Any] = field(default_factory=dict)


@dataclass
class AgentResponse:
    agent_id: str
    status: str
    output: str
    duration_ms: int = 0


@dataclass
class EvalRequest:
    suite_name: str
    categories: list[str] = field(default_factory=list)
    max_concurrency: int = 4


@dataclass
class EvalResult:
    suite_name: str
    total: int
    passed: int
    failed: int
    accuracy: float
    avg_latency_ms: float
    report_url: Optional[str] = None


@dataclass
class FederationNode:
    node_id: str
    name: str
    status: str
    capabilities: list[str] = field(default_factory=list)


@dataclass
class FederationStatus:
    connected_nodes: int
    total_nodes: int
    active_links: int
    uptime_seconds: int
