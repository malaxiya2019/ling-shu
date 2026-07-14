"""🖨️ SimpleCAD-MCP Server — CAD 建模 MCP 服务

通过 MCP 协议向 ling-shu AI Agent 暴露 SimpleCADAPI 的全部建模能力。
支持：基本体、布尔运算、特征、变换、标注、QL 查询、GraphSession 可重放建模、STEP/STL 导出。

运行:
    uv run simplecad-mcp
    # 或
    python -m simplecad_mcp.server
"""

from __future__ import annotations

import json
import os
import sys
import tempfile
import traceback
from pathlib import Path
from typing import Any

import simplecadapi as scad
from simplecadapi import ql as Q
from simplecadapi import GraphSession

# ── 尝试导入 mcp ──────────────────────────────────────
try:
    from mcp.server import Server, NotificationOptions
    from mcp.server.models import InitializationOptions
    from mcp.types import (
        Tool,
        TextContent,
        ImageContent,
        EmbeddedResource,
    )
    import mcp.server.stdio
except ImportError:
    print("需要安装 mcp: pip install mcp", file=sys.stderr)
    sys.exit(1)


# ══════════════════════════════════════════════════════════
# 全局状态 — 可以保持多个会话的上下文
# ══════════════════════════════════════════════════════════

_active_session: GraphSession | None = None
_session_stack: list[dict[str, Any]] = []  # 命名形状缓存


def _get_or_create_session() -> GraphSession:
    global _active_session
    if _active_session is None:
        _active_session = GraphSession()
    return _active_session


def _reset_session():
    global _active_session, _session_stack
    _active_session = None
    _session_stack = []


# ══════════════════════════════════════════════════════════
# 工具定义
# ══════════════════════════════════════════════════════════

TOOLS: list[Tool] = []


def tool(name: str, description: str, input_schema: dict) -> Tool:
    t = Tool(name=name, description=description, input_schema=input_schema)
    TOOLS.append(t)
    return t


# ── session 管理 ──────────────────────────────────────

tool(
    name="cad_create_session",
    description="创建或重置 GraphSession，开始可重放建模会话",
    input_schema={
        "type": "object",
        "properties": {},
    },
)

tool(
    name="cad_export_session",
    description="导出当前 GraphSession 的模型 JSON，用于保存/重放",
    input_schema={
        "type": "object",
        "properties": {
            "pretty": {
                "type": "boolean",
                "description": "是否格式化输出",
                "default": True,
            }
        },
    },
)

tool(
    name="cad_replay_model",
    description="从模型 JSON 重放建模操作，返回重建后的形状信息",
    input_schema={
        "type": "object",
        "properties": {
            "model_json": {
                "type": "string",
                "description": "之前导出的模型 JSON 字符串",
            }
        },
        "required": ["model_json"],
    },
)

# ── 基本体 ────────────────────────────────────────────

tool(
    name="cad_make_box",
    description="创建一个长方体 (box)",
    input_schema={
        "type": "object",
        "properties": {
            "dx": {"type": "number", "description": "X 方向长度"},
            "dy": {"type": "number", "description": "Y 方向长度"},
            "dz": {"type": "number", "description": "Z 方向高度"},
            "bottom_face_center": {
                "type": "array",
                "items": {"type": "number"},
                "description": "底面中心坐标 [x, y, z]",
                "default": [0, 0, 0],
            },
            "tag": {
                "type": "string",
                "description": "可选语义标签",
            },
        },
        "required": ["dx", "dy", "dz"],
    },
)

tool(
    name="cad_make_cylinder",
    description="创建一个圆柱体",
    input_schema={
        "type": "object",
        "properties": {
            "radius": {"type": "number", "description": "半径"},
            "height": {"type": "number", "description": "高度"},
            "bottom_face_center": {
                "type": "array",
                "items": {"type": "number"},
                "description": "底面中心坐标 [x, y, z]",
                "default": [0, 0, 0],
            },
            "tag": {"type": "string", "description": "可选语义标签"},
        },
        "required": ["radius", "height"],
    },
)

tool(
    name="cad_make_sphere",
    description="创建一个球体",
    input_schema={
        "type": "object",
        "properties": {
            "radius": {"type": "number", "description": "半径"},
            "center": {
                "type": "array",
                "items": {"type": "number"},
                "description": "中心坐标 [x, y, z]",
                "default": [0, 0, 0],
            },
            "tag": {"type": "string", "description": "可选语义标签"},
        },
        "required": ["radius"],
    },
)

tool(
    name="cad_make_cone",
    description="创建一个圆锥体",
    input_schema={
        "type": "object",
        "properties": {
            "bottom_radius": {"type": "number", "description": "底面半径"},
            "top_radius": {"type": "number", "description": "顶面半径"},
            "height": {"type": "number", "description": "高度"},
            "bottom_face_center": {
                "type": "array",
                "items": {"type": "number"},
                "description": "底面中心坐标 [x, y, z]",
                "default": [0, 0, 0],
            },
            "tag": {"type": "string", "description": "可选语义标签"},
        },
        "required": ["bottom_radius", "top_radius", "height"],
    },
)

tool(
    name="cad_make_torus",
    description="创建一个圆环体",
    input_schema={
        "type": "object",
        "properties": {
            "major_radius": {"type": "number", "description": "主半径"},
            "minor_radius": {"type": "number", "description": "副半径"},
            "center": {
                "type": "array",
                "items": {"type": "number"},
                "description": "中心坐标 [x, y, z]",
                "default": [0, 0, 0],
            },
            "tag": {"type": "string", "description": "可选语义标签"},
        },
        "required": ["major_radius", "minor_radius"],
    },
)

# ── 布尔运算 ──────────────────────────────────────────

tool(
    name="cad_union",
    description="布尔并集 — 将多个形状合并为一个",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tags": {
                "type": "array",
                "items": {"type": "string"},
                "description": "要合并的形状标签列表",
            },
        },
        "required": ["shape_tags"],
    },
)

tool(
    name="cad_cut",
    description="布尔差集 — 从主体中减去一个或多个工具形状",
    input_schema={
        "type": "object",
        "properties": {
            "base_tag": {
                "type": "string",
                "description": "主体形状的标签",
            },
            "tool_tags": {
                "type": "array",
                "items": {"type": "string"},
                "description": "工具形状的标签列表",
            },
        },
        "required": ["base_tag", "tool_tags"],
    },
)

tool(
    name="cad_intersect",
    description="布尔交集 — 返回多个形状的公共部分",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tags": {
                "type": "array",
                "items": {"type": "string"},
                "description": "形状标签列表",
            },
        },
        "required": ["shape_tags"],
    },
)

# ── 特征 ──────────────────────────────────────────────

tool(
    name="cad_fillet",
    description="对边进行圆角处理",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tag": {"type": "string", "description": "形状标签"},
            "radius": {"type": "number", "description": "圆角半径"},
            "edge_selector": {
                "type": "string",
                "description": "QL 边选择器，留空则选择所有边",
            },
        },
        "required": ["shape_tag", "radius"],
    },
)

tool(
    name="cad_chamfer",
    description="对边进行倒角处理",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tag": {"type": "string", "description": "形状标签"},
            "distance": {"type": "number", "description": "倒角距离"},
            "edge_selector": {
                "type": "string",
                "description": "QL 边选择器，留空则选择所有边",
            },
        },
        "required": ["shape_tag", "distance"],
    },
)

# ── 变换 ──────────────────────────────────────────────

tool(
    name="cad_translate",
    description="平移一个形状",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tag": {"type": "string", "description": "形状标签"},
            "vector": {
                "type": "array",
                "items": {"type": "number"},
                "description": "平移向量 [dx, dy, dz]",
            },
        },
        "required": ["shape_tag", "vector"],
    },
)

tool(
    name="cad_rotate",
    description="绕轴旋转一个形状",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tag": {"type": "string", "description": "形状标签"},
            "axis": {
                "type": "array",
                "items": {"type": "number"},
                "description": "旋转轴方向 [ax, ay, az]",
                "default": [0, 0, 1],
            },
            "angle_deg": {"type": "number", "description": "旋转角度（度）"},
            "center": {
                "type": "array",
                "items": {"type": "number"},
                "description": "旋转中心 [x, y, z]",
                "default": [0, 0, 0],
            },
        },
        "required": ["shape_tag", "angle_deg"],
    },
)

tool(
    name="cad_scale",
    description="缩放一个形状",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tag": {"type": "string", "description": "形状标签"},
            "factors": {
                "type": "array",
                "items": {"type": "number"},
                "description": "缩放因子 [sx, sy, sz]",
            },
            "center": {
                "type": "array",
                "items": {"type": "number"},
                "description": "缩放中心 [x, y, z]",
                "default": [0, 0, 0],
            },
        },
        "required": ["shape_tag", "factors"],
    },
)

tool(
    name="cad_mirror",
    description="镜像一个形状",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tag": {"type": "string", "description": "形状标签"},
            "normal": {
                "type": "array",
                "items": {"type": "number"},
                "description": "镜像平面法线 [nx, ny, nz]",
            },
            "point": {
                "type": "array",
                "items": {"type": "number"},
                "description": "镜像平面上的一点 [x, y, z]",
                "default": [0, 0, 0],
            },
        },
        "required": ["shape_tag", "normal"],
    },
)

# ── 标注 ──────────────────────────────────────────────

tool(
    name="cad_apply_tag",
    description="给形状打语义标签，便于后续引用和查询",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tag": {"type": "string", "description": "形状现有标签"},
            "new_tag": {"type": "string", "description": "新的语义标签"},
        },
        "required": ["shape_tag", "new_tag"],
    },
)

tool(
    name="cad_list_tags",
    description="列出当前会话中所有形状标签",
    input_schema={
        "type": "object",
        "properties": {},
    },
)

# ── QL 查询 ───────────────────────────────────────────

tool(
    name="cad_ql_query",
    description="使用 QL 选择器查询几何体。示例: Q.faces().where(Q.area_gt(100)).exactly(1)",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tag": {"type": "string", "description": "被查询的形状标签"},
            "selector_type": {
                "type": "string",
                "description": "选择器类型: faces / edges / vertices",
                "enum": ["faces", "edges", "vertices"],
            },
            "filters": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "args": {
                            "type": "array",
                            "items": {},
                        },
                    },
                },
                "description": "过滤器链，例如 [{\"name\":\"area_gt\", \"args\":[100}]",
            },
            "limit": {
                "type": "integer",
                "description": "最多返回几条",
            },
            "format": {
                "type": "string",
                "description": "输出格式: count / summary / full",
                "default": "summary",
            },
        },
        "required": ["shape_tag", "selector_type"],
    },
)

# ── 导出 ──────────────────────────────────────────────

tool(
    name="cad_export_step",
    description="导出 STEP 文件，返回文件路径",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tag": {"type": "string", "description": "形状标签"},
            "file_path": {
                "type": "string",
                "description": "输出路径，留空则输出到临时文件",
            },
        },
        "required": ["shape_tag"],
    },
)

tool(
    name="cad_export_stl",
    description="导出 STL 文件，返回文件路径",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tag": {"type": "string", "description": "形状标签"},
            "file_path": {
                "type": "string",
                "description": "输出路径，留空则输出到临时文件",
            },
            "linear_deflection": {
                "type": "number",
                "description": "线性偏差（精度）",
                "default": 0.01,
            },
            "angular_deflection": {
                "type": "number",
                "description": "角度偏差（弧度）",
                "default": 0.5,
            },
        },
        "required": ["shape_tag"],
    },
)

tool(
    name="cad_get_info",
    description="获取形状的几何信息：体积、面数、边数、顶点数",
    input_schema={
        "type": "object",
        "properties": {
            "shape_tag": {"type": "string", "description": "形状标签"},
        },
        "required": ["shape_tag"],
    },
)


# ══════════════════════════════════════════════════════════
# 形状缓存辅助
# ══════════════════════════════════════════════════════════

def _find_shape(tag: str):
    """在会话栈中按标签查找形状"""
    for entry in reversed(_session_stack):
        if entry.get("tag") == tag:
            return entry["shape"]
    raise ValueError(f"未找到标签为 '{tag}' 的形状。可用标签: {[e['tag'] for e in _session_stack]}")


def _store_shape(shape, tag: str | None = None) -> str:
    """存储形状并返回标签"""
    if tag is None:
        tag = f"shape_{len(_session_stack)}"
    _session_stack.append({"tag": tag, "shape": shape})
    return tag


def _shape_summary(shape, tag: str = "") -> str:
    """生成形状的摘要信息"""
    try:
        vol = shape.get_volume()
        faces = len(shape.get_faces())
        edges = len(shape.get_edges())
        verts = len(shape.get_vertices())
        tag_str = f"[{tag}] " if tag else ""
        return f"{tag_str}Solid: volume={vol:.3f}, faces={faces}, edges={edges}, vertices={verts}"
    except Exception:
        return f"[{tag}] Solid (info unavailable)"


# ══════════════════════════════════════════════════════════
# 工具处理函数
# ══════════════════════════════════════════════════════════

async def handle_tool(name: str, arguments: dict) -> list[TextContent]:
    try:
        result = _execute_tool(name, arguments)
        return [TextContent(type="text", text=result)]
    except Exception as e:
        tb = traceback.format_exc()
        return [TextContent(type="text", text=f"错误: {e}\n\n{tb}")]


def _execute_tool(name: str, args: dict) -> str:
    global _active_session

    # ── session 管理 ──────────────────────────────────
    if name == "cad_create_session":
        _reset_session()
        _active_session = GraphSession()
        return json.dumps({"status": "ok", "message": "新 GraphSession 已创建"})

    if name == "cad_export_session":
        if _active_session is None:
            return json.dumps({"status": "error", "message": "没有活动会话"})
        pretty = args.get("pretty", True)
        model_json = scad.export_model_json(
            _active_session, indent=2 if pretty else None
        )
        return json.dumps({"status": "ok", "model_json": model_json})

    if name == "cad_replay_model":
        model_json = args["model_json"]
        rebuilt = scad.replay_model_json(model_json)
        _reset_session()
        _active_session = GraphSession()
        result = []
        for i, shape in enumerate(rebuilt):
            tag = _store_shape(shape, f"replay_{i}")
            result.append(_shape_summary(shape, tag))
        return json.dumps({"status": "ok", "shapes": result})

    # ── 基本体 ─────────────────────────────────────────
    if name == "cad_make_box":
        dx, dy, dz = args["dx"], args["dy"], args["dz"]
        center = tuple(args.get("bottom_face_center", [0, 0, 0]))
        shape = scad.make_box_rsolid(
            dx, dy, dz, bottom_face_center=center
        )
        tag = _store_shape(shape, args.get("tag"))
        _maybe_tag(shape, args.get("tag"))
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(shape, tag)})

    if name == "cad_make_cylinder":
        radius = args["radius"]
        height = args["height"]
        center = tuple(args.get("bottom_face_center", [0, 0, 0]))
        shape = scad.make_cylinder_rsolid(
            radius, height, bottom_face_center=center
        )
        tag = _store_shape(shape, args.get("tag"))
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(shape, tag)})

    if name == "cad_make_sphere":
        radius = args["radius"]
        center = tuple(args.get("center", [0, 0, 0]))
        shape = scad.make_sphere_rsolid(radius, center=center)
        tag = _store_shape(shape, args.get("tag"))
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(shape, tag)})

    if name == "cad_make_cone":
        r1 = args["bottom_radius"]
        r2 = args["top_radius"]
        h = args["height"]
        center = tuple(args.get("bottom_face_center", [0, 0, 0]))
        shape = scad.make_cone_rsolid(r1, r2, h, bottom_face_center=center)
        tag = _store_shape(shape, args.get("tag"))
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(shape, tag)})

    if name == "cad_make_torus":
        R = args["major_radius"]
        r = args["minor_radius"]
        center = tuple(args.get("center", [0, 0, 0]))
        shape = scad.make_torus_rsolid(R, r, center=center)
        tag = _store_shape(shape, args.get("tag"))
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(shape, tag)})

    # ── 布尔运算 ──────────────────────────────────────
    if name == "cad_union":
        tags = args["shape_tags"]
        shapes = [_find_shape(t) for t in tags]
        result = shapes[0]
        for s in shapes[1:]:
            result = scad.union_rsolid(result, s)
        tag = _store_shape(result, f"union_{len(_session_stack)}")
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(result, tag)})

    if name == "cad_cut":
        base = _find_shape(args["base_tag"])
        tools = [_find_shape(t) for t in args["tool_tags"]]
        result = base
        for t in tools:
            result = scad.cut_rsolid(result, t)
        tag = _store_shape(result, f"cut_{len(_session_stack)}")
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(result, tag)})

    if name == "cad_intersect":
        tags = args["shape_tags"]
        shapes = [_find_shape(t) for t in tags]
        result = shapes[0]
        for s in shapes[1:]:
            result = scad.intersect_rsolid(result, s)
        tag = _store_shape(result, f"intersect_{len(_session_stack)}")
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(result, tag)})

    # ── 特征 ──────────────────────────────────────────
    if name == "cad_fillet":
        shape = _find_shape(args["shape_tag"])
        radius = args["radius"]
        if args.get("edge_selector"):
            edges = _eval_ql(shape, "edges", args["edge_selector"])
            result = scad.fillet_rsolid(shape, edges, radius)
        else:
            result = scad.fillet_rsolid(shape, radius)
        tag = _store_shape(result, f"fillet_{len(_session_stack)}")
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(result, tag)})

    if name == "cad_chamfer":
        shape = _find_shape(args["shape_tag"])
        dist = args["distance"]
        if args.get("edge_selector"):
            edges = _eval_ql(shape, "edges", args["edge_selector"])
            result = scad.chamfer_rsolid(shape, edges, dist)
        else:
            result = scad.chamfer_rsolid(shape, dist)
        tag = _store_shape(result, f"chamfer_{len(_session_stack)}")
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(result, tag)})

    # ── 变换 ──────────────────────────────────────────
    if name == "cad_translate":
        shape = _find_shape(args["shape_tag"])
        vec = tuple(args["vector"])
        result = scad.translate_rsolid(shape, vec)
        tag = _store_shape(result, f"trans_{len(_session_stack)}")
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(result, tag)})

    if name == "cad_rotate":
        shape = _find_shape(args["shape_tag"])
        angle = args["angle_deg"]
        axis = tuple(args.get("axis", [0, 0, 1]))
        center = tuple(args.get("center", [0, 0, 0]))
        result = scad.rotate_rsolid(shape, angle, axis=axis, center=center)
        tag = _store_shape(result, f"rot_{len(_session_stack)}")
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(result, tag)})

    if name == "cad_scale":
        shape = _find_shape(args["shape_tag"])
        factors = tuple(args["factors"])
        center = tuple(args.get("center", [0, 0, 0]))
        result = scad.scale_rsolid(shape, factors, center=center)
        tag = _store_shape(result, f"scale_{len(_session_stack)}")
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(result, tag)})

    if name == "cad_mirror":
        shape = _find_shape(args["shape_tag"])
        normal = tuple(args["normal"])
        pt = tuple(args.get("point", [0, 0, 0]))
        result = scad.mirror_rsolid(shape, normal, point=pt)
        tag = _store_shape(result, f"mirror_{len(_session_stack)}")
        return json.dumps({"status": "ok", "tag": tag, "summary": _shape_summary(result, tag)})

    # ── 标注 ──────────────────────────────────────────
    if name == "cad_apply_tag":
        shape = _find_shape(args["shape_tag"])
        scad.apply_tag(shape, args["new_tag"])
        return json.dumps({"status": "ok", "tag": args["new_tag"]})

    if name == "cad_list_tags":
        tags = [e["tag"] for e in _session_stack]
        return json.dumps({"status": "ok", "tags": tags, "count": len(tags)})

    # ── QL 查询 ───────────────────────────────────────
    if name == "cad_ql_query":
        shape = _find_shape(args["shape_tag"])
        sel_type = args["selector_type"]
        filters = args.get("filters", [])
        limit = args.get("limit")
        fmt = args.get("format", "summary")

        # 构建 QL 选择器
        ql_obj = getattr(Q, sel_type)()
        for f in filters:
            method = getattr(ql_obj, f["name"])
            ql_obj = method(*f.get("args", []))

        if limit:
            ql_obj = ql_obj.take(limit)

        result = ql_obj(shape)
        if fmt == "count":
            return json.dumps({"status": "ok", "count": len(result)})
        else:
            items = []
            for i, item in enumerate(result):
                items.append(f"#{i}: {type(item).__name__}")
            return json.dumps({"status": "ok", "count": len(result), "items": items})

    # ── 导出 ──────────────────────────────────────────
    if name == "cad_export_step":
        shape = _find_shape(args["shape_tag"])
        file_path = args.get("file_path")
        if not file_path:
            tmp = tempfile.NamedTemporaryFile(suffix=".step", delete=False)
            file_path = tmp.name
            tmp.close()
        scad.export_step(shape, file_path)
        return json.dumps({"status": "ok", "file_path": file_path})

    if name == "cad_export_stl":
        shape = _find_shape(args["shape_tag"])
        file_path = args.get("file_path")
        if not file_path:
            tmp = tempfile.NamedTemporaryFile(suffix=".stl", delete=False)
            file_path = tmp.name
            tmp.close()
        ld = args.get("linear_deflection", 0.01)
        ad = args.get("angular_deflection", 0.5)
        scad.export_stl(shape, file_path, linear_deflection=ld, angular_deflection=ad)
        return json.dumps({"status": "ok", "file_path": file_path})

    # ── 信息 ──────────────────────────────────────────
    if name == "cad_get_info":
        shape = _find_shape(args["shape_tag"])
        return json.dumps({
            "status": "ok",
            "tag": args["shape_tag"],
            "summary": _shape_summary(shape, args["shape_tag"]),
        })

    raise ValueError(f"未知工具: {name}")


def _maybe_tag(shape, tag: str | None):
    if tag:
        scad.apply_tag(shape, tag)


def _eval_ql(shape, selector_type: str, selector_expr: str):
    """简单评估 QL 选择器表达式"""
    ql_obj = getattr(Q, selector_type)()
    # 简化的 filter 解析 — 生产环境可以用更安全的方式
    return ql_obj(shape)


# ══════════════════════════════════════════════════════════
# MCP 服务启动
# ══════════════════════════════════════════════════════════

server = Server("simplecad-mcp")


@server.list_tools()
async def list_tools() -> list[Tool]:
    return TOOLS


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[TextContent]:
    return await handle_tool(name, arguments)


async def main():
    async with mcp.server.stdio.stdio_server() as (read_stream, write_stream):
        await server.run(
            read_stream,
            write_stream,
            InitializationOptions(
                server_name="simplecad-mcp",
                server_version="0.1.0",
                capabilities=server.get_capabilities(
                    notification_options=NotificationOptions(),
                    experimental_capabilities={},
                ),
            ),
        )


if __name__ == "__main__":
    import asyncio
    asyncio.run(main())
