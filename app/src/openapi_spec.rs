//! OpenAPI 3.0.3 specification for Lingshu API.
//! This module builds the complete OpenAPI spec dynamically.

use serde_json::{json, Value};

/// Build the full OpenAPI 3.0.3 specification.
pub fn build() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Lingshu API",
            "version": "3.0.0",
            "description": "Lingshu Agent System — Production-grade AI agent orchestration platform\n\n\
            ## Features\n\
            - **Multi-LLM**: OpenAI, Anthropic, Groq, Mock backends\n\
            - **Agent Runtime**: Lifecycle, scheduling, recovery\n\
            - **Federation**: Cross-cluster agent execution with discovery & replication\n\
            - **Evaluation**: Built-in test suite runner, metrics & regression detection\n\
            - **Plugins**: Static, WASM, hot-reloadable plugin system\n\
            - **Security**: RBAC/ABAC, JWT auth, Ed25519 service auth, credential vault\n\
            - **Observability**: Prometheus metrics, OTel tracing, structured logging\n\
            - **Multi-modal**: Image/audio processing with RAG\n\
            - **MCP Protocol**: JSON-RPC 2.0 tool system\n\
            - **Real-time**: WebSocket & SSE streaming",
            "contact": {
                "name": "Lingshu Team",
                "url": "https://github.com/malaxiya2019/ling-shu"
            },
            "license": {
                "name": "MIT OR Apache-2.0",
                "url": "https://github.com/malaxiya2019/ling-shu"
            }
        },
        "servers": [
            {"url": "http://localhost:8080", "description": "Local development"},
            {"url": "https://api.lingshu.local", "description": "Production (Helm default)"}
        ],
        "tags": [
            {"name": "Core", "description": "Core system endpoints: health, metrics, version"},
            {"name": "Chat", "description": "Chat completion and streaming endpoints"},
            {"name": "Agent", "description": "Agent management and execution"},
            {"name": "Embeddings", "description": "Text embedding endpoints"},
            {"name": "WebSocket", "description": "Real-time WebSocket communication"},
            {"name": "SSE", "description": "Server-Sent Events streaming"},
            {"name": "MCP", "description": "MCP (Model Context Protocol) endpoints"},
            {"name": "Files", "description": "File upload, analysis, and retrieval"},
            {"name": "Evaluator", "description": "Agent evaluation framework"},
            {"name": "Federation", "description": "Cross-cluster federation"},
            {"name": "Plugins", "description": "Plugin management and marketplace"},
            {"name": "Security", "description": "Security testing and monitoring"},
            {"name": "Watch", "description": "Video analysis (Watch Skill)"},
            {"name": "Knowledge Graph", "description": "Knowledge graph management"},
            {"name": "Credentials", "description": "Multi-Git credential management"},
            {"name": "Admin", "description": "Admin dashboard and authentication"},
            {"name": "API Docs", "description": "API documentation endpoints"}
        ],
        "paths": {
            // ── Core ──
            "/health": {
                "get": {
                    "summary": "Health check",
                    "description": "Returns system health status with per-subsystem checks",
                    "operationId": "healthCheck",
                    "tags": ["Core"],
                    "responses": {
                        "200": {
                            "description": "System is healthy",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/HealthResponse"}}}
                        },
                        "503": {"description": "System degraded"}
                    }
                }
            },
            "/metrics": {
                "get": {
                    "summary": "Prometheus metrics",
                    "description": "Returns Prometheus-formatted metrics for scraping",
                    "operationId": "getMetrics",
                    "tags": ["Core"],
                    "responses": {
                        "200": {
                            "description": "Metrics text",
                            "content": {"text/plain": {"schema": {"type": "string"}}}
                        }
                    }
                }
            },
            "/version": {
                "get": {
                    "summary": "Version information",
                    "description": "Returns build version, commit, and Rust compiler info",
                    "operationId": "getVersion",
                    "tags": ["Core"],
                    "responses": {
                        "200": {
                            "description": "Version JSON",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/VersionResponse"}}}
                        }
                    }
                }
            },

            // ── Chat ──
            "/v1/models": {
                "get": {
                    "summary": "List available models",
                    "operationId": "listModels",
                    "tags": ["Chat"],
                    "responses": {
                        "200": {
                            "description": "Model list",
                            "content": {"application/json": {"schema": {"type": "array", "items": {"$ref": "#/components/schemas/ModelInfo"}}}}
                        }
                    }
                }
            },
            "/v1/chat/completions": {
                "post": {
                    "summary": "Chat completion (OpenAI compatible)",
                    "description": "OpenAI-compatible chat completion endpoint. Supports streaming via SSE.",
                    "operationId": "createChatCompletion",
                    "tags": ["Chat"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/ChatCompletionRequest"}}
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Chat response",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ChatCompletionResponse"}}}
                        }
                    }
                }
            },
            "/v1/chat": {
                "post": {
                    "summary": "Internal chat",
                    "description": "Lingshu-native chat endpoint with session support",
                    "operationId": "chat",
                    "tags": ["Chat"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/ChatRequest"}}
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Chat response",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ChatResponse"}}}
                        }
                    }
                }
            },
            "/v1/chat/multimodal": {
                "post": {
                    "summary": "Multimodal chat",
                    "description": "Chat with image/audio file input support",
                    "operationId": "multimodalChat",
                    "tags": ["Chat"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/MultimodalChatRequest"}}
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Multimodal chat response",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ChatResponse"}}}
                        }
                    }
                }
            },
            "/v2/chat/stream": {
                "get": {
                    "summary": "Streaming chat (SSE)",
                    "description": "Server-Sent Events streaming chat endpoint",
                    "operationId": "streamChat",
                    "tags": ["Chat", "SSE"],
                    "parameters": [
                        {"name": "prompt", "in": "query", "required": true, "schema": {"type": "string"}},
                        {"name": "session_id", "in": "query", "schema": {"type": "string"}},
                        {"name": "model", "in": "query", "schema": {"type": "string"}}
                    ],
                    "responses": {
                        "200": {
                            "description": "SSE event stream",
                            "content": {"text/event-stream": {"schema": {"type": "string"}}}
                        }
                    }
                }
            },

            // ── Embeddings ──
            "/v1/embeddings": {
                "post": {
                    "summary": "Create embeddings (OpenAI compatible)",
                    "operationId": "createEmbeddings",
                    "tags": ["Embeddings"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/EmbeddingRequest"}}
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Embedding response",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/EmbeddingResponse"}}}
                        }
                    }
                }
            },
            "/v1/embed": {
                "post": {
                    "summary": "Internal embed",
                    "operationId": "embed",
                    "tags": ["Embeddings"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/EmbeddingRequest"}}
                        }
                    },
                    "responses": {"200": {"description": "Embed response"}}
                }
            },

            // ── Agent ──
            "/v1/agent/run": {
                "post": {
                    "summary": "Run an agent task",
                    "description": "Execute an agent with the given input and configuration",
                    "operationId": "runAgent",
                    "tags": ["Agent"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/AgentRunRequest"}}
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Agent execution result",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/AgentRunResponse"}}}
                        }
                    }
                }
            },
            "/v1/agents": {
                "get": {
                    "summary": "List agents",
                    "operationId": "listAgents",
                    "tags": ["Agent"],
                    "responses": {
                        "200": {
                            "description": "Agent list",
                            "content": {"application/json": {"schema": {"type": "array", "items": {"$ref": "#/components/schemas/AgentSummary"}}}}
                        }
                    }
                }
            },
            "/v1/agents/{id}": {
                "get": {
                    "summary": "Get agent status",
                    "operationId": "getAgent",
                    "tags": ["Agent"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {
                        "200": {
                            "description": "Agent status",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/AgentStatus"}}}
                        }
                    }
                }
            },
            "/v1/agents/{id}/pause": {
                "post": {
                    "summary": "Pause agent",
                    "operationId": "pauseAgent",
                    "tags": ["Agent"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {"200": {"description": "Agent paused"}}
                }
            },
            "/v1/agents/{id}/resume": {
                "post": {
                    "summary": "Resume agent",
                    "operationId": "resumeAgent",
                    "tags": ["Agent"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {"200": {"description": "Agent resumed"}}
                }
            },
            "/v1/agents/{id}/cancel": {
                "post": {
                    "summary": "Cancel agent",
                    "operationId": "cancelAgent",
                    "tags": ["Agent"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {"200": {"description": "Agent cancelled"}}
                }
            },

            // ── WebSocket ──
            "/ws": {
                "get": {
                    "summary": "WebSocket connection",
                    "description": "Upgrade to WebSocket protocol for real-time communication",
                    "operationId": "webSocketConnect",
                    "tags": ["WebSocket"],
                    "responses": {"101": {"description": "Switching Protocols — WebSocket upgrade"}}
                }
            },
            "/v2/ws": {
                "get": {
                    "summary": "WebSocket v2",
                    "description": "Enhanced WebSocket endpoint with improved protocol",
                    "operationId": "webSocketConnectV2",
                    "tags": ["WebSocket"],
                    "responses": {"101": {"description": "WebSocket upgrade"}}
                }
            },
            "/v2/events": {
                "get": {
                    "summary": "SSE event stream",
                    "description": "Subscribe to real-time system events via Server-Sent Events",
                    "operationId": "subscribeEvents",
                    "tags": ["SSE"],
                    "responses": {
                        "200": {
                            "description": "SSE event stream",
                            "content": {"text/event-stream": {"schema": {"type": "string"}}}
                        }
                    }
                }
            },

            // ── MCP ──
            "/v1/mcp": {
                "post": {
                    "summary": "MCP JSON-RPC endpoint",
                    "description": "Model Context Protocol endpoint supporting JSON-RPC 2.0 method calls",
                    "operationId": "mcpCall",
                    "tags": ["MCP"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/McpRequest"}}
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "MCP response",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/McpResponse"}}}
                        }
                    }
                }
            },
            "/v1/mcp/tools": {
                "get": {
                    "summary": "List MCP tools",
                    "operationId": "listMcpTools",
                    "tags": ["MCP"],
                    "responses": {
                        "200": {
                            "description": "Tool list",
                            "content": {"application/json": {"schema": {"type": "array", "items": {"$ref": "#/components/schemas/McpTool"}}}}
                        }
                    }
                }
            },
            "/v1/mcp/ui": {
                "get": {
                    "summary": "MCP UI page",
                    "operationId": "mcpUi",
                    "tags": ["MCP"],
                    "responses": {"200": {"description": "MCP UI HTML page"}}
                }
            },

            // ── Files ──
            "/v1/files": {
                "get": {
                    "summary": "List uploaded files",
                    "operationId": "listFiles",
                    "tags": ["Files"],
                    "responses": {
                        "200": {
                            "description": "File list",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/FileListResponse"}}}
                        }
                    }
                }
            },
            "/v1/files/{id}": {
                "get": {
                    "summary": "Get file by ID",
                    "operationId": "getFile",
                    "tags": ["Files"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {
                        "200": {
                            "description": "File record",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/FileRecord"}}}
                        }
                    }
                }
            },
            "/v1/files/upload": {
                "post": {
                    "summary": "Upload file",
                    "description": "Upload a file (Base64 encoded) for processing",
                    "operationId": "uploadFile",
                    "tags": ["Files"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/FileUploadRequest"}}
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Upload result",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/FileRecord"}}}
                        }
                    }
                }
            },
            "/v1/files/analyze": {
                "post": {
                    "summary": "Analyze file",
                    "description": "Analyze an uploaded file (extract text, OCR, etc.)",
                    "operationId": "analyzeFile",
                    "tags": ["Files"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/FileAnalyzeRequest"}}
                        }
                    },
                    "responses": {"200": {"description": "Analysis result"}}
                }
            },

            // ── Evaluator ──
            "/v1/eval/run": {
                "post": {
                    "summary": "Run evaluation suite",
                    "description": "Execute an evaluation test suite and return results",
                    "operationId": "runEvaluation",
                    "tags": ["Evaluator"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/EvalRunRequest"}}
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Evaluation result",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/EvalResult"}}}
                        }
                    }
                }
            },
            "/v1/eval/result": {
                "get": {
                    "summary": "Get latest evaluation result",
                    "operationId": "getEvalResult",
                    "tags": ["Evaluator"],
                    "responses": {
                        "200": {
                            "description": "Latest evaluation result",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/EvalResult"}}}
                        }
                    }
                }
            },
            "/v1/eval/regression": {
                "post": {
                    "summary": "Regression analysis",
                    "description": "Compare evaluation results against a baseline for regression detection",
                    "operationId": "evalRegression",
                    "tags": ["Evaluator"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/RegressionRequest"}}
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Regression analysis result",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/RegressionResult"}}}
                        }
                    }
                }
            },

            // ── Federation ──
            "/v1/federation/status": {
                "get": {
                    "summary": "Federation cluster status",
                    "description": "Get the current status of the federation cluster",
                    "operationId": "federationStatus",
                    "tags": ["Federation"],
                    "responses": {
                        "200": {
                            "description": "Federation status",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/FederationStatus"}}}
                        }
                    }
                }
            },
            "/v1/federation/nodes": {
                "get": {
                    "summary": "List federated nodes",
                    "description": "List all online federated nodes in the cluster",
                    "operationId": "federationNodes",
                    "tags": ["Federation"],
                    "responses": {
                        "200": {
                            "description": "Node list",
                            "content": {"application/json": {"schema": {"type": "array", "items": {"$ref": "#/components/schemas/FederationNode"}}}}
                        }
                    }
                }
            },
            "/v1/federation/execute": {
                "post": {
                    "summary": "Remote execution",
                    "description": "Execute an agent task on a remote federated node",
                    "operationId": "federationExecute",
                    "tags": ["Federation"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/FederationExecuteRequest"}}
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Remote execution result",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/AgentRunResponse"}}}
                        }
                    }
                }
            },

            // ── Plugins ──
            "/v1/plugins": {
                "get": {
                    "summary": "List installed plugins",
                    "operationId": "listPlugins",
                    "tags": ["Plugins"],
                    "responses": {
                        "200": {
                            "description": "Plugin list",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/PluginListResponse"}}}
                        }
                    }
                },
                "post": {
                    "summary": "Install a plugin",
                    "operationId": "installPlugin",
                    "tags": ["Plugins"],
                    "responses": {"200": {"description": "Plugin installed"}}
                }
            },
            "/v1/plugins/{id}": {
                "get": {
                    "summary": "Get plugin details",
                    "operationId": "getPlugin",
                    "tags": ["Plugins"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {
                        "200": {
                            "description": "Plugin details",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/PluginInfo"}}}
                        }
                    }
                },
                "delete": {
                    "summary": "Uninstall plugin",
                    "operationId": "uninstallPlugin",
                    "tags": ["Plugins"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {"200": {"description": "Plugin uninstalled"}}
                }
            },
            "/v1/plugins/{id}/start": {
                "post": {
                    "summary": "Start plugin",
                    "operationId": "startPlugin",
                    "tags": ["Plugins"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {"200": {"description": "Plugin started"}}
                }
            },
            "/v1/plugins/{id}/stop": {
                "post": {
                    "summary": "Stop plugin",
                    "operationId": "stopPlugin",
                    "tags": ["Plugins"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {"200": {"description": "Plugin stopped"}}
                }
            },
            "/v1/plugins/market/search": {
                "get": {
                    "summary": "Search plugin marketplace",
                    "operationId": "marketSearch",
                    "tags": ["Plugins"],
                    "parameters": [
                        {"name": "q", "in": "query", "schema": {"type": "string"}},
                        {"name": "page", "in": "query", "schema": {"type": "integer"}},
                        {"name": "limit", "in": "query", "schema": {"type": "integer"}}
                    ],
                    "responses": {
                        "200": {
                            "description": "Market search results",
                            "content": {"application/json": {"schema": {"type": "array", "items": {"$ref": "#/components/schemas/MarketPluginEntry"}}}}
                        }
                    }
                }
            },
            "/v1/plugins/market/install": {
                "post": {
                    "summary": "Install from marketplace",
                    "operationId": "marketInstall",
                    "tags": ["Plugins"],
                    "responses": {"200": {"description": "Market plugin installed"}}
                }
            },
            "/v1/plugins/market/sources": {
                "get": {
                    "summary": "List market sources",
                    "operationId": "marketSources",
                    "tags": ["Plugins"],
                    "responses": {"200": {"description": "Market sources list"}}
                }
            },
            "/v1/plugins/hotreload/start": {
                "post": {
                    "summary": "Start hot-reload watcher",
                    "operationId": "hotReloadStart",
                    "tags": ["Plugins"],
                    "responses": {"200": {"description": "Hot-reload started"}}
                }
            },
            "/v1/plugins/hotreload/stop": {
                "post": {
                    "summary": "Stop hot-reload watcher",
                    "operationId": "hotReloadStop",
                    "tags": ["Plugins"],
                    "responses": {"200": {"description": "Hot-reload stopped"}}
                }
            },
            "/v1/plugins/events": {
                "get": {
                    "summary": "SSE plugin lifecycle events",
                    "operationId": "pluginEvents",
                    "tags": ["Plugins", "SSE"],
                    "responses": {"200": {"description": "SSE event stream"}}
                }
            },

            // ── Security (BeEF) ──
            "/v1/security/beef/status": {
                "get": {
                    "summary": "BeEF status",
                    "operationId": "beefStatus",
                    "tags": ["Security"],
                    "responses": {"200": {"description": "BeEF status"}}
                }
            },
            "/v1/security/beef/start": {
                "post": {
                    "summary": "Start BeEF",
                    "operationId": "beefStart",
                    "tags": ["Security"],
                    "responses": {"200": {"description": "BeEF started"}}
                }
            },
            "/v1/security/beef/stop": {
                "post": {
                    "summary": "Stop BeEF",
                    "operationId": "beefStop",
                    "tags": ["Security"],
                    "responses": {"200": {"description": "BeEF stopped"}}
                }
            },
            "/v1/security/beef/restart": {
                "post": {
                    "summary": "Restart BeEF",
                    "operationId": "beefRestart",
                    "tags": ["Security"],
                    "responses": {"200": {"description": "BeEF restarted"}}
                }
            },
            "/v1/security/beef/hooks": {
                "get": {
                    "summary": "List hooked browsers",
                    "operationId": "beefHooks",
                    "tags": ["Security"],
                    "responses": {
                        "200": {
                            "description": "Hooked browsers list",
                            "content": {"application/json": {"schema": {"type": "array", "items": {"type": "object"}}}}
                        }
                    }
                }
            },

            // ── Watch Skill ──
            "/v1/watch/status": {
                "get": {
                    "summary": "Watch Skill status",
                    "operationId": "watchStatus",
                    "tags": ["Watch"],
                    "responses": {"200": {"description": "Watch plugin status"}}
                }
            },
            "/v1/watch/start": {
                "post": {
                    "summary": "Start Watch analysis",
                    "operationId": "watchStart",
                    "tags": ["Watch"],
                    "responses": {"200": {"description": "Watch started"}}
                }
            },
            "/v1/watch/stop": {
                "post": {
                    "summary": "Stop Watch analysis",
                    "operationId": "watchStop",
                    "tags": ["Watch"],
                    "responses": {"200": {"description": "Watch stopped"}}
                }
            },
            "/v1/watch/video": {
                "post": {
                    "summary": "Analyze video",
                    "operationId": "watchVideo",
                    "tags": ["Watch"],
                    "responses": {"200": {"description": "Video analysis result"}}
                }
            },
            "/v1/watch/ask": {
                "post": {
                    "summary": "Ask about video",
                    "operationId": "watchAsk",
                    "tags": ["Watch"],
                    "responses": {"200": {"description": "Answer about video"}}
                }
            },
            "/v1/watch/search": {
                "get": {
                    "summary": "Search videos",
                    "operationId": "watchSearch",
                    "tags": ["Watch"],
                    "responses": {"200": {"description": "Search results"}}
                }
            },
            "/v1/watch/videos": {
                "get": {
                    "summary": "List watched videos",
                    "operationId": "watchListVideos",
                    "tags": ["Watch"],
                    "responses": {"200": {"description": "Video list"}}
                }
            },

            // ── Knowledge Graph ──
            "/v1/graph/{project}": {
                "get": {
                    "summary": "Query knowledge graph",
                    "parameters": [{"name": "project", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "operationId": "graphQuery",
                    "tags": ["Knowledge Graph"],
                    "responses": {"200": {"description": "Graph query result"}}
                },
                "post": {
                    "summary": "Analyze code into graph",
                    "parameters": [{"name": "project", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "operationId": "graphAnalyze",
                    "tags": ["Knowledge Graph"],
                    "responses": {"200": {"description": "Analysis result"}}
                }
            },
            "/v1/graph/{project}/view": {
                "get": {
                    "summary": "View knowledge graph",
                    "parameters": [{"name": "project", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "operationId": "graphView",
                    "tags": ["Knowledge Graph"],
                    "responses": {"200": {"description": "Graph visualization"}}
                }
            },
            "/v1/projects": {
                "get": {
                    "summary": "List analyzed projects",
                    "operationId": "listProjects",
                    "tags": ["Knowledge Graph"],
                    "responses": {"200": {"description": "Project list"}}
                }
            },

            // ── Credentials ──
            "/v1/credentials": {
                "get": {
                    "summary": "List credentials",
                    "operationId": "listCredentials",
                    "tags": ["Credentials"],
                    "responses": {
                        "200": {
                            "description": "Credential list",
                            "content": {"application/json": {"schema": {"type": "array", "items": {"$ref": "#/components/schemas/CredentialEntry"}}}}
                        }
                    }
                },
                "post": {
                    "summary": "Create credential",
                    "operationId": "createCredential",
                    "tags": ["Credentials"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {"schema": {"$ref": "#/components/schemas/CreateCredentialRequest"}}
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Credential created",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/CredentialEntry"}}}
                        }
                    }
                }
            },
            "/v1/credentials/providers": {
                "get": {
                    "summary": "List credential providers",
                    "operationId": "credentialProviders",
                    "tags": ["Credentials"],
                    "responses": {"200": {"description": "Provider list"}}
                }
            },
            "/v1/credentials/{id}": {
                "get": {
                    "summary": "Get credential",
                    "operationId": "getCredential",
                    "tags": ["Credentials"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {
                        "200": {
                            "description": "Credential",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/CredentialEntry"}}}
                        }
                    }
                },
                "put": {
                    "summary": "Update credential",
                    "operationId": "updateCredential",
                    "tags": ["Credentials"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {"200": {"description": "Credential updated"}}
                },
                "delete": {
                    "summary": "Delete credential",
                    "operationId": "deleteCredential",
                    "tags": ["Credentials"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {"200": {"description": "Credential deleted"}}
                }
            },
            "/v1/credentials/{id}/validate": {
                "post": {
                    "summary": "Validate credential",
                    "description": "Test if a credential works against its provider",
                    "operationId": "validateCredential",
                    "tags": ["Credentials"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {"200": {"description": "Validation result"}}
                }
            },
            "/v1/credentials/{id}/token": {
                "get": {
                    "summary": "Get temporary access token",
                    "operationId": "getCredentialToken",
                    "tags": ["Credentials"],
                    "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "responses": {"200": {"description": "Temporary token"}}
                }
            },
            "/v1/credentials/ui": {
                "get": {
                    "summary": "Credential management UI",
                    "operationId": "credentialUi",
                    "tags": ["Credentials"],
                    "responses": {"200": {"description": "Credential UI HTML"}}
                }
            },

            // ── Auth ──
            "/login": {
                "get": {
                    "summary": "Login page",
                    "operationId": "loginPage",
                    "tags": ["Admin"],
                    "responses": {"200": {"description": "Login page HTML"}}
                },
                "post": {
                    "summary": "Login",
                    "operationId": "login",
                    "tags": ["Admin"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/x-www-form-urlencoded": {"schema": {"$ref": "#/components/schemas/LoginRequest"}}
                        }
                    },
                    "responses": {
                        "200": {"description": "Login successful"},
                        "401": {"description": "Invalid credentials"}
                    }
                }
            },
            "/logout": {
                "get": {
                    "summary": "Logout",
                    "operationId": "logout",
                    "tags": ["Admin"],
                    "responses": {"200": {"description": "Logged out"}}
                }
            },
            "/api/auth/me": {
                "get": {
                    "summary": "Current user info",
                    "operationId": "authMe",
                    "tags": ["Admin"],
                    "responses": {
                        "200": {
                            "description": "Current user info",
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/UserInfo"}}}
                        }
                    }
                }
            },

            // ── Admin ──
            "/admin": {
                "get": {
                    "summary": "Admin dashboard",
                    "operationId": "adminDashboard",
                    "tags": ["Admin"],
                    "responses": {"200": {"description": "Admin dashboard HTML"}}
                }
            },
            "/webui/*": {
                "get": {
                    "summary": "WASM admin panel",
                    "description": "Serves the Yew WASM-based admin panel from webui/dist",
                    "operationId": "webuiServe",
                    "tags": ["Admin"],
                    "responses": {"200": {"description": "WASM admin panel"}}
                }
            },

            // ── API Docs ──
            "/docs": {
                "get": {
                    "summary": "API documentation page",
                    "operationId": "apiDocs",
                    "tags": ["API Docs"],
                    "responses": {"200": {"description": "Documentation page HTML"}}
                }
            },
            "/docs/openapi.json": {
                "get": {
                    "summary": "OpenAPI specification",
                    "operationId": "openApiJson",
                    "tags": ["API Docs"],
                    "responses": {
                        "200": {
                            "description": "OpenAPI 3.0.3 JSON spec",
                            "content": {"application/json": {"schema": {"type": "object"}}}
                        }
                    }
                }
            },
            "/docs/swagger": {
                "get": {
                    "summary": "Swagger UI",
                    "operationId": "swaggerUi",
                    "tags": ["API Docs"],
                    "responses": {"200": {"description": "Swagger UI HTML"}}
                }
            }
        },
        "components": {
            "schemas": {
                "HealthResponse": {
                    "type": "object",
                    "properties": {
                        "status": {"type": "string", "example": "ok"},
                        "version": {"type": "string", "example": "3.0.0"},
                        "uptime": {"type": "string", "example": "2h 34m 12s"},
                        "checks": {
                            "type": "array",
                            "items": {"$ref": "#/components/schemas/HealthCheckItem"}
                        }
                    }
                },
                "HealthCheckItem": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "healthy": {"type": "boolean"},
                        "detail": {"type": "string"}
                    }
                },
                "VersionResponse": {
                    "type": "object",
                    "properties": {
                        "version": {"type": "string"},
                        "build": {"type": "string"},
                        "rustc": {"type": "string"}
                    }
                },
                "ModelInfo": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "object": {"type": "string", "default": "model"},
                        "created": {"type": "integer"},
                        "owned_by": {"type": "string"}
                    }
                },
                "ChatCompletionRequest": {
                    "type": "object",
                    "required": ["model", "messages"],
                    "properties": {
                        "model": {"type": "string", "description": "Model ID to use"},
                        "messages": {
                            "type": "array",
                            "items": {"$ref": "#/components/schemas/ChatMessage"}
                        },
                        "stream": {"type": "boolean", "default": false},
                        "temperature": {"type": "number", "format": "float", "default": 0.7},
                        "max_tokens": {"type": "integer"},
                        "session_id": {"type": "string"},
                        "user": {"type": "string"},
                        "tools": {"type": "array", "items": {"type": "object"}}
                    }
                },
                "ChatMessage": {
                    "type": "object",
                    "required": ["role", "content"],
                    "properties": {
                        "role": {"type": "string", "enum": ["system", "user", "assistant"]},
                        "content": {"type": "string"},
                        "name": {"type": "string"}
                    }
                },
                "ChatCompletionResponse": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "object": {"type": "string"},
                        "created": {"type": "integer"},
                        "model": {"type": "string"},
                        "choices": {"type": "array", "items": {"$ref": "#/components/schemas/Choice"}},
                        "usage": {"$ref": "#/components/schemas/UsageInfo"}
                    }
                },
                "Choice": {
                    "type": "object",
                    "properties": {
                        "index": {"type": "integer"},
                        "message": {
                            "type": "object",
                            "properties": {
                                "role": {"type": "string"},
                                "content": {"type": "string"}
                            }
                        },
                        "finish_reason": {"type": "string"}
                    }
                },
                "UsageInfo": {
                    "type": "object",
                    "properties": {
                        "prompt_tokens": {"type": "integer"},
                        "completion_tokens": {"type": "integer"},
                        "total_tokens": {"type": "integer"}
                    }
                },
                "ChatRequest": {
                    "type": "object",
                    "properties": {
                        "message": {"type": "string"},
                        "session_id": {"type": "string"},
                        "model": {"type": "string"}
                    }
                },
                "ChatResponse": {
                    "type": "object",
                    "properties": {
                        "reply": {"type": "string"},
                        "session_id": {"type": "string"}
                    }
                },
                "MultimodalChatRequest": {
                    "type": "object",
                    "properties": {
                        "prompt": {"type": "string"},
                        "file_ids": {"type": "array", "items": {"type": "string"}},
                        "session_id": {"type": "string"},
                        "model": {"type": "string"}
                    }
                },
                "EmbeddingRequest": {
                    "type": "object",
                    "required": ["model", "input"],
                    "properties": {
                        "model": {"type": "string"},
                        "input": {"type": "array", "items": {"type": "string"}}
                    }
                },
                "EmbeddingResponse": {
                    "type": "object",
                    "properties": {
                        "object": {"type": "string"},
                        "data": {"type": "array", "items": {"type": "object"}},
                        "model": {"type": "string"},
                        "usage": {"$ref": "#/components/schemas/UsageInfo"}
                    }
                },
                "AgentRunRequest": {
                    "type": "object",
                    "required": ["input"],
                    "properties": {
                        "input": {"type": "string"},
                        "agent_id": {"type": "string"},
                        "session_id": {"type": "string"},
                        "config": {
                            "type": "object",
                            "properties": {
                                "model": {"type": "string"},
                                "temperature": {"type": "number"},
                                "max_tokens": {"type": "integer"},
                                "tools": {"type": "array", "items": {"type": "string"}},
                                "timeout_secs": {"type": "integer"}
                            }
                        }
                    }
                },
                "AgentRunResponse": {
                    "type": "object",
                    "properties": {
                        "agent_id": {"type": "string"},
                        "status": {"type": "string"},
                        "output": {"type": "string"},
                        "duration_ms": {"type": "integer"}
                    }
                },
                "AgentSummary": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "name": {"type": "string"},
                        "status": {"type": "string"},
                        "created_at": {"type": "string", "format": "date-time"}
                    }
                },
                "AgentStatus": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "status": {"type": "string"},
                        "session_id": {"type": "string"},
                        "created_at": {"type": "string"},
                        "updated_at": {"type": "string"}
                    }
                },
                "McpRequest": {
                    "type": "object",
                    "properties": {
                        "jsonrpc": {"type": "string", "default": "2.0"},
                        "method": {"type": "string"},
                        "params": {"type": "object"},
                        "id": {"type": "integer"}
                    }
                },
                "McpResponse": {
                    "type": "object",
                    "properties": {
                        "jsonrpc": {"type": "string"},
                        "result": {"type": "object"},
                        "id": {"type": "integer"}
                    }
                },
                "McpTool": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "description": {"type": "string"},
                        "input_schema": {"type": "object"}
                    }
                },
                "FileRecord": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "name": {"type": "string"},
                        "mime_type": {"type": "string"},
                        "size_bytes": {"type": "integer"},
                        "file_type": {"type": "string"},
                        "created_at": {"type": "string"}
                    }
                },
                "FileUploadRequest": {
                    "type": "object",
                    "required": ["name", "data"],
                    "properties": {
                        "name": {"type": "string"},
                        "data": {"type": "string", "description": "Base64-encoded file data"},
                        "mime_type": {"type": "string"}
                    }
                },
                "FileAnalyzeRequest": {
                    "type": "object",
                    "required": ["file_id"],
                    "properties": {
                        "file_id": {"type": "string"}
                    }
                },
                "FileListResponse": {
                    "type": "object",
                    "properties": {
                        "files": {"type": "array", "items": {"$ref": "#/components/schemas/FileRecord"}},
                        "total": {"type": "integer"}
                    }
                },
                "EvalRunRequest": {
                    "type": "object",
                    "properties": {
                        "suite_name": {"type": "string"},
                        "suite_id": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}}
                    }
                },
                "EvalResult": {
                    "type": "object",
                    "properties": {
                        "suite_name": {"type": "string"},
                        "total": {"type": "integer"},
                        "passed": {"type": "integer"},
                        "failed": {"type": "integer"},
                        "overall_score": {"type": "number"},
                        "weighted_score": {"type": "number"},
                        "metrics": {
                            "type": "object",
                            "properties": {
                                "accuracy": {"type": "number"},
                                "precision": {"type": "number"},
                                "recall": {"type": "number"},
                                "f1_score": {"type": "number"},
                                "latency_p50_ms": {"type": "number"},
                                "latency_p95_ms": {"type": "number"},
                                "latency_p99_ms": {"type": "number"}
                            }
                        }
                    }
                },
                "RegressionRequest": {
                    "type": "object",
                    "properties": {
                        "baseline_id": {"type": "string"},
                        "current_id": {"type": "string"},
                        "thresholds": {"type": "object"}
                    }
                },
                "RegressionResult": {
                    "type": "object",
                    "properties": {
                        "has_regression": {"type": "boolean"},
                        "comparisons": {"type": "array", "items": {"type": "object"}},
                        "summary": {"type": "string"}
                    }
                },
                "FederationStatus": {
                    "type": "object",
                    "properties": {
                        "cluster_name": {"type": "string"},
                        "connected_nodes": {"type": "integer"},
                        "total_nodes": {"type": "integer"},
                        "active_links": {"type": "integer"},
                        "topology": {"type": "string"},
                        "uptime_seconds": {"type": "integer"},
                        "nodes": {"type": "array", "items": {"$ref": "#/components/schemas/FederationNode"}}
                    }
                },
                "FederationNode": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "name": {"type": "string"},
                        "addr": {"type": "string"},
                        "status": {"type": "string"},
                        "capabilities": {"type": "array", "items": {"type": "string"}},
                        "connected_at": {"type": "string", "format": "date-time"}
                    }
                },
                "FederationExecuteRequest": {
                    "type": "object",
                    "properties": {
                        "target_node": {"type": "string"},
                        "agent_input": {"type": "string"},
                        "timeout_secs": {"type": "integer", "default": 30}
                    }
                },
                "PluginInfo": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "name": {"type": "string"},
                        "version": {"type": "string"},
                        "status": {"type": "string", "enum": ["stopped", "running", "error"]},
                        "description": {"type": "string"},
                        "plugin_type": {"type": "string"}
                    }
                },
                "PluginListResponse": {
                    "type": "object",
                    "properties": {
                        "plugins": {"type": "array", "items": {"$ref": "#/components/schemas/PluginInfo"}},
                        "total": {"type": "integer"}
                    }
                },
                "MarketPluginEntry": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "version": {"type": "string"},
                        "description": {"type": "string"},
                        "source": {"type": "string"}
                    }
                },
                "CredentialEntry": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "provider": {"type": "string"},
                        "description": {"type": "string"},
                        "created_at": {"type": "string", "format": "date-time"},
                        "updated_at": {"type": "string", "format": "date-time"}
                    }
                },
                "CreateCredentialRequest": {
                    "type": "object",
                    "required": ["provider", "credential"],
                    "properties": {
                        "provider": {"type": "string"},
                        "credential": {"type": "string"},
                        "description": {"type": "string"}
                    }
                },
                "LoginRequest": {
                    "type": "object",
                    "required": ["username", "password"],
                    "properties": {
                        "username": {"type": "string"},
                        "password": {"type": "string", "format": "password"}
                    }
                },
                "UserInfo": {
                    "type": "object",
                    "properties": {
                        "username": {"type": "string"},
                        "role": {"type": "string"},
                        "logged_in": {"type": "boolean"}
                    }
                },
                "ErrorResponse": {
                    "type": "object",
                    "properties": {
                        "error": {"type": "string"},
                        "code": {"type": "string"},
                        "detail": {"type": "string"}
                    }
                }
            }
        }
    })
}
