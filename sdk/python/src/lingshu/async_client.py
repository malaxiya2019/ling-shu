"""LingShu Python SDK — 异步 HTTP 客户端."""

from __future__ import annotations

from typing import Any, Optional

import httpx

from .models import (
    AgentRequest,
    AgentResponse,
    ChatRequest,
    ChatResponse,
    EvalRequest,
    EvalResult,
    FederationNode,
    FederationStatus,
)


class AsyncLingShuClient:
    """LingShu API 异步客户端."""

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
        self._client: httpx.AsyncClient | None = None

    async def __aenter__(self) -> AsyncLingShuClient:
        self._client = httpx.AsyncClient(
            base_url=self._base_url,
            headers={
                "Content-Type": "application/json",
                "User-Agent": "lingshu-py/0.1.0",
            },
            timeout=self._timeout,
        )
        return self

    async def __aexit__(self, *args: Any) -> None:
        if self._client:
            await self._client.aclose()

    async def _ensure_client(self) -> httpx.AsyncClient:
        if self._client is None:
            self._client = httpx.AsyncClient(
                base_url=self._base_url,
                headers={
                    "Content-Type": "application/json",
                    "User-Agent": "lingshu-py/0.1.0",
                },
                timeout=self._timeout,
            )
        return self._client

    # ── Chat Completion ──

    async def chat(self, request: ChatRequest) -> ChatResponse:
        client = await self._ensure_client()
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

        resp = await client.post("/v1/chat/completions", json=payload)
        resp.raise_for_status()
        data = resp.json()
        return ChatResponse(
            id=data.get("id", ""),
            content=data["choices"][0]["message"]["content"],
            model=data.get("model", request.model),
            usage=data.get("usage", {}),
        )

    # ── Agent Operations ──

    async def run_agent(self, request: AgentRequest) -> AgentResponse:
        client = await self._ensure_client()
        resp = await client.post(
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

    async def get_agent_status(self, agent_id: str) -> AgentResponse:
        client = await self._ensure_client()
        resp = await client.get(f"/agents/{agent_id}/status")
        resp.raise_for_status()
        data = resp.json()
        return AgentResponse(
            agent_id=data.get("agent_id", agent_id),
            status=data.get("status", "unknown"),
            output=data.get("output", ""),
            duration_ms=data.get("duration_ms", 0),
        )

    # ── Evaluation ──

    async def run_eval(self, request: EvalRequest) -> EvalResult:
        client = await self._ensure_client()
        resp = await client.post(
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

    async def get_federation_status(self) -> FederationStatus:
        client = await self._ensure_client()
        resp = await client.get("/federation/status")
        resp.raise_for_status()
        data = resp.json()
        return FederationStatus(
            connected_nodes=data.get("connected_nodes", 0),
            total_nodes=data.get("total_nodes", 0),
            active_links=data.get("active_links", 0),
            uptime_seconds=data.get("uptime_seconds", 0),
        )

    async def list_federation_nodes(self) -> list[FederationNode]:
        client = await self._ensure_client()
        resp = await client.get("/federation/nodes")
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

    async def health_check(self) -> dict[str, Any]:
        client = await self._ensure_client()
        resp = await client.get("/health")
        resp.raise_for_status()
        return resp.json()
