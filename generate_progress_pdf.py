#!/usr/bin/env python3
"""Generate LingShu progress PDF report - ASCII-safe version"""

from fpdf import FPDF
import os

class PDF(FPDF):
    def header(self):
        self.set_font("Helvetica", "B", 16)
        self.set_text_color(30, 144, 255)
        self.cell(0, 10, "LingShu (ling-shu) Agent System - Progress Report", align="C", new_x="LMARGIN", new_y="NEXT")
        self.set_font("Helvetica", "", 9)
        self.set_text_color(100, 100, 100)
        self.cell(0, 6, "Generated: 2026-07-14 23:50 CST  |  Status: RC (Release Candidate)", align="C", new_x="LMARGIN", new_y="NEXT")
        self.line(10, self.get_y(), 200, self.get_y())
        self.ln(4)

    def footer(self):
        self.set_y(-15)
        self.set_font("Helvetica", "I", 8)
        self.set_text_color(128, 128, 128)
        self.cell(0, 10, f"Page {self.page_no()}/{{nb}}")

    def section_hdr(self, title, r=30, g=144, b=255):
        self.set_font("Helvetica", "B", 13)
        self.set_text_color(r, g, b)
        self.cell(0, 8, title, new_x="LMARGIN", new_y="NEXT")
        self.ln(3)

pdf = PDF(orientation="P", unit="mm", format="A4")
pdf.alias_nb_pages()
pdf.set_auto_page_break(auto=True, margin=20)
pdf.add_page()

# 1. Overview
pdf.section_hdr("1. Project Overview")
pdf.set_font("Helvetica", "", 10)
data = [
    ("Project", "LingShu - Modular Agent Framework in Rust"),
    ("Version", "v5.1.0 (2026-07-13)"),
    ("Language", "Rust (44+ workspace crates, ~33,000+ lines)"),
    ("Repo", "github.com/malaxiya2019/ling-shu"),
    ("Status", "RC (Release Candidate) - all core features complete"),
]
for k, v in data:
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(25, 6, k + ":")
    pdf.set_font("Helvetica", "", 10)
    pdf.cell(0, 6, v, new_x="LMARGIN", new_y="NEXT")
pdf.ln(4)

# 2. Milestones
pdf.section_hdr("2. Completed Milestones")
ms = [
    "[DONE] v1.x Core Infrastructure: core/traits/runtime/eventbus/security...",
    "[DONE] v2.x Real-time + Memory + MCP + Platform + Multimodal",
    "[DONE] v2.6 Evaluator Framework + v2.7 Federation + WebUI",
    "[DONE] v3.x Enterprise: Multi-Tenant/Audit Dashboard/Vault/TEE",
    "[DONE] v3.4-3.6 Ecosystem: OpenHands/AutoAgents/Discord/Qdrant",
    "[DONE] v4.2-4.5 Runtime Metrics/RBAC/OmniVoice/Hot Reload",
    "[DONE] v5.0 AgentSwarm(62 tests) + Distributed Scheduler(32) + Autonomy(18)",
    "[DONE] v5.1 Engineering Quality: CI fixed, Clippy zero-warnings",
    "[IN-PROGRESS] v4.5 Audit SQLite persistence + Yew WebUI sync (TODAY)",
]
for m in ms:
    pdf.set_font("Helvetica", "", 9)
    c = (0, 180, 80) if m.startswith("[DONE]") else (255, 165, 0)
    pdf.set_text_color(*c)
    pdf.cell(0, 5, m, new_x="LMARGIN", new_y="NEXT")
pdf.set_text_color(60, 60, 60)
pdf.ln(3)

# 3. Today's Work
pdf.section_hdr("3. Work Completed Today (2026-07-14)", 0, 180, 80)
pdf.set_font("Courier", "", 8)
lines = [
    "Task 1: AuditLogStore SQLite Persistence",
    "  File:    audit/src/sqlite.rs (NEW - 250 lines)",
    "  Feature: auto-create table, WAL mode, full query support",
    "  Tests:   20/20 passed (append,query,count,pagination,persist)",
    "",
    "Task 2: Yew WebUI Audit Page Enhancement",
    "  File:    webui/src/pages/audit.rs (REWRITTEN)",
    "  File:    webui/src/api/client.rs (audit API types added)",
    "  Features added:",
    "    - Stats cards (total + per-event-type distribution)",
    "    - Filter panel (actor input, event_type/result selects)",
    "    - Detail modal (click row to view full entry)",
    "    - Export buttons (JSON / CSV download)",
    "    - Pagination (prev/next with page info)",
]
for l in lines:
    pdf.set_text_color(60, 60, 60)
    pdf.cell(0, 4, l, new_x="LMARGIN", new_y="NEXT")
pdf.ln(4)

# 4. Remaining Tasks
pdf.section_hdr("4. Remaining Tasks", 255, 100, 50)
pdf.set_font("Helvetica", "", 9)
pdf.set_font("Helvetica", "B", 9)
pdf.cell(12, 6, "Pri")
pdf.cell(55, 6, "Task")
pdf.cell(0, 6, "Detail", new_x="LMARGIN", new_y="NEXT")
pdf.line(10, pdf.get_y(), 200, pdf.get_y())
pdf.ln(2)
rem = [
    ("P0", "Route SqliteAuditLog into app/", "Replace in-memory store in build_router"),
    ("P1", "Yew WASM compile verify", "Install wasm32 target + trunk, build webui"),
    ("P2", "Fix cargo doc failures", "webui/llm-router/scheduler/multimodal"),
    ("P3", "E2E tests for swarm/autonomy", "Integration test coverage"),
    ("P3", "cargo bench baseline", "Establish performance baseline"),
]
for pri, t, d in rem:
    pc = {"P0": (220, 50, 50), "P1": (220, 150, 50), "P2": (180, 180, 50), "P3": (100, 150, 200)}
    pdf.set_font("Helvetica", "B", 9)
    pdf.set_text_color(*pc[pri])
    pdf.cell(12, 5, pri)
    pdf.set_font("Helvetica", "", 9)
    pdf.set_text_color(60, 60, 60)
    pdf.cell(55, 5, t[:50])
    pdf.set_font("Helvetica", "", 8)
    pdf.set_text_color(100, 100, 100)
    pdf.multi_cell(0, 5, d[:70])
    pdf.ln(1)
pdf.ln(3)

# 5. Test Stats
pdf.section_hdr("5. Test Statistics")
pdf.set_font("Helvetica", "B", 9)
pdf.set_fill_color(30, 60, 90)
pdf.set_text_color(200, 200, 200)
pdf.cell(85, 7, "  Suite", fill=True)
pdf.cell(30, 7, "Tests", align="C", fill=True)
pdf.cell(30, 7, "Pass", align="C", fill=True)
pdf.ln()
pdf.set_text_color(60, 60, 60)
tests = [
    ("lingshu-swarm (AgentSwarm)", "62", "100%"),
    ("lingshu-distributed (Scheduler)", "32", "100%"),
    ("lingshu-autonomy (Self-evolve)", "18", "100%"),
    ("lingshu-security (RBAC+JWT)", "31", "100%"),
    ("lingshu-traits", "89", "100%"),
    ("lingshu-federation", "20", "100%"),
    ("lingshu-evaluator", "14", "100%"),
    ("lingshu-audit (incl. SQLite)", "20", "100%"),
    ("lingshu-eventbus", "9", "100%"),
    ("lingshu-core", "7", "100%"),
]
for n, c, r in tests:
    pdf.set_font("Helvetica", "", 8.5)
    pdf.cell(85, 5, f"  {n}")
    pdf.cell(30, 5, c, align="C")
    pdf.set_text_color(0, 180, 80)
    pdf.cell(30, 5, r, align="C")
    pdf.set_text_color(60, 60, 60)
    pdf.ln()
pdf.ln(3)

# 6. Release Timeline
pdf.section_hdr("6. Release Timeline")
rels = [
    ("v5.1.0", "2026-07-13", "Engineering quality + CI fix"),
    ("v5.0.0", "2026-07-12", "AgentSwarm + Distributed + Autonomy"),
    ("v4.2.7", "2026-06", "LTS stable release"),
]
pdf.set_font("Helvetica", "", 9)
for v, d, desc in rels:
    pdf.set_font("Helvetica", "B", 9)
    pdf.cell(22, 5, v)
    pdf.set_font("Helvetica", "", 9)
    pdf.cell(28, 5, d)
    pdf.cell(0, 5, desc, new_x="LMARGIN", new_y="NEXT")
pdf.ln(3)

# 7. Git Status
pdf.section_hdr("7. Git HEAD")
pdf.set_font("Courier", "", 8)
pdf.set_text_color(80, 80, 80)
for line in [
    "Branch: main",
    "HEAD:   a1e0173 - fix(ci): let lint job auto-run cargo fmt --all",
    "Tag:    v5.1.0-21-ga1e0173",
    "Status: 3 untracked files (fix_patch.py, ci scripts)",
]:
    pdf.cell(0, 4.5, f"  {line}", new_x="LMARGIN", new_y="NEXT")

# Save
out = "/data/data/com.termux/files/home/storage/downloads/Lingshu_Progress_Report.pdf"
try:
    pdf.output(out)
    print(f"OK: {out} ({os.path.getsize(out)} bytes)")
except Exception as e:
    out2 = "/data/data/com.termux/files/home/ling-shu/Lingshu_Progress_Report.pdf"
    pdf.output(out2)
    print(f"OK (fallback): {out2}")
    os.system(f"cp {out2} /data/data/com.termux/files/home/storage/downloads/")
    print(f"Also copied to downloads")
