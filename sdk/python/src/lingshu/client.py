"""LingShu Python SDK — 同步 HTTP 客户端."""

from __future__ import annotations

from typing import Any, Optional

import httpx

from .models import (
    AgentRequest,
    AgentResponse,
    ChatMessage,
    ChatRequest,
    ChatResponse,
    EvalRequest,
    EvalResult,
    FederationNode,
    FederationStatus,
)


class LingShuClient:
    """LingShu API 同步客户端."""

    def __init__(
        self,
        base_url: str = "http://localhost:8080",
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._timeout = timeout
        headers: dict[str, str] = {
            "Content-Type": "application/json",
            "User-Agent": "lingshu-py/0.1.0",
        }
        if api_key:
            headers["Authorization"] = f"Bearer {api_key}"
        self._client = httpx.Client(base_url=self._base_url, headers=headers, timeout=timeout)

    def close(self) -> None:
        self._client.close()

    def __enter__(self) -> LingShuClient:
        return self

    def __exit__(self, *args: Any) -> None:
        self.close()

    # ── Chat Completion ──

    def chat(self, request: ChatRequest) -> ChatResponse:
        payload = {
            "model": request.model,
            "messages": [{"role": m.role, "content": m.content} for m in request.messages],
        }
        if request.temperature is not None:
            payload["temperature"] = request.temperature
        if request.max_tokens is not None:
            payload["max_tokens"] = request.max_tokens
        if request.stream:
            payload["stream"] = True

        resp = self._client.post("/v1/chat/completions", json=payload)
        resp.raise_for_status()
        data = resp.json()
        return ChatResponse(
            id=data.get("id", ""),
            content=data["choices"][0]["message"]["content"],
            model=data.get("model", request.model),
            usage=data.get("usage", {}),
        )

    # ── Agent Operations ──

    def run_agent(self, request: AgentRequest) -> AgentResponse:
        resp = self._client.post(
            f"/agents/{request.agent_id}/run",
            json={"input": request.input, "config": request.config},
        )
        resp.raise_for_status()
        data = resp.json()
        return AgentResponse(
            agent_id=data.get("agent_id", request.agent_id),
            status=data.get("status", "completed"),
            output=data.get("output", ""),
            duration_ms=data.get("duration_ms", 0),
        )

    def get_agent_status(self, agent_id: str) -> AgentResponse:
        resp = self._client.get(f"/agents/{agent_id}/status")
        resp.raise_for_status()
        data = resp.json()
        return AgentResponse(
            agent_id=data.get("agent_id", agent_id),
            status=data.get("status", "unknown"),
            output=data.get("output", ""),
            duration_ms=data.get("duration_ms", 0),
        )

    # ── Evaluation ──

    def run_eval(self, request: EvalRequest) -> EvalResult:
        resp = self._client.post(
            "/eval/run",
            json={
                "suite_name": request.suite_name,
                "categories": request.categories,
                "max_concurrency": request.max_concurrency,
            },
        )
        resp.raise_for_status()
        data = resp.json()
        return EvalResult(
            suite_name=data.get("suite_name", request.suite_name),
            total=data.get("total", 0),
            passed=data.get("passed", 0),
            failed=data.get("failed", 0),
            accuracy=data.get("accuracy", 0.0),
            avg_latency_ms=data.get("avg_latency_ms", 0.0),
            report_url=data.get("report_url"),
        )

    # ── Federation ──

    def get_federation_status(self) -> FederationStatus:
        resp = self._client.get("/federation/status")
        resp.raise_for_status()
        data = resp.json()
        return FederationStatus(
            connected_nodes=data.get("connected_nodes", 0),
            total_nodes=data.get("total_nodes", 0),
            active_links=data.get("active_links", 0),
            uptime_seconds=data.get("uptime_seconds", 0),
        )

    def list_federation_nodes(self) -> list[FederationNode]:
        resp = self._client.get("/federation/nodes")
        resp.raise_for_status()
        data = resp.json()
        return [
            FederationNode(
                node_id=n.get("node_id", ""),
                name=n.get("name", ""),
                status=n.get("status", "unknown"),
                capabilities=n.get("capabilities", []),
            )
            for n in data.get("nodes", [])
        ]

    # ── Health ──

    def health_check(self) -> dict[str, Any]:
        resp = self._client.get("/health")
        resp.raise_for_status()
        return resp.json()
