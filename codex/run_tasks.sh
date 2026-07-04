#!/usr/bin/env bash
set -euo pipefail

TASK_FILE="codex/task.json"
STATE_FILE="codex/state.json"

echo "🚀 LingShu Codex Task Runner"
echo "=============================="

# Init state file
if [ ! -f "$STATE_FILE" ]; then
  echo '{"completed":[],"current":null,"failed":[]}' > "$STATE_FILE"
fi

get_status() {
  jq -r ".tasks[] | select(.id == \"$1\") | .status" "$TASK_FILE"
}

set_status() {
  local id="$1"
  local status="$2"
  local tmp=$(mktemp)
  jq "(..|select(.id?==\"$id\")).status = \"$status\"" "$TASK_FILE" > "$tmp" && mv "$tmp" "$TASK_FILE"
}

list_pending() {
  echo "📋 Pending tasks:"
  jq -r '.tasks[] | select(.status == "pending" or .status == null) | "  \(.id) [\(.phase)] \(.name) — \(.description)"' "$TASK_FILE"
}

list_completed() {
  echo "✅ Completed tasks:"
  jq -r '.tasks[] | select(.status == "completed") | "  \(.id) [\(.phase)] \(.name)"' "$TASK_FILE"
}

list_all() {
  echo "📋 All tasks:"
  jq -r '.tasks[] | "  [\(.status // "todo")] \(.id) [\(.phase)] \(.name)"' "$TASK_FILE"
}

run_task() {
  local id="$1"
  local status=$(get_status "$id")
  
  if [ "$status" = "completed" ]; then
    echo "⏭ Task $id already completed, skipping."
    return 0
  fi

  local name=$(jq -r ".tasks[] | select(.id == \"$id\") | .name" "$TASK_FILE")
  local desc=$(jq -r ".tasks[] | select(.id == \"$id\") | .description" "$TASK_FILE")
  local input=$(jq -r ".tasks[] | select(.id == \"$id\") | .input" "$TASK_FILE")
  local deps=$(jq -r ".tasks[] | select(.id == \"$id\") | .depends_on[]" "$TASK_FILE" 2>/dev/null || echo "")

  # Check dependencies
  for dep in $deps; do
    local dep_status=$(get_status "$dep")
    if [ "$dep_status" != "completed" ]; then
      echo "❌ Dependency $dep not completed. Run it first."
      return 1
    fi
  done

  echo ""
  echo "=============================="
  echo "🧠 Task $id: $name"
  echo "   $desc"
  echo "=============================="
  
  set_status "$id" "running"
  
  # Update state
  local tmp=$(mktemp)
  jq ".current = \"$id\"" "$STATE_FILE" > "$tmp" && mv "$tmp" "$STATE_FILE"

  echo "📥 Input: $input"
  echo "🔧 Running..."
  
  # Codex will execute this task — mark it as placeholder
  echo "⚠ Task $id is delegated to Codex CLI"

  # After completion (this will be called by Codex after doing the work)
  echo ""
  echo "✅ Task $id done — run 'codex done $id' to mark complete"
}

mark_done() {
  local id="$1"
  set_status "$id" "completed"
  local tmp=$(mktemp)
  jq ".completed += [\"$id\"] | .current = null" "$STATE_FILE" > "$tmp" && mv "$tmp" "$STATE_FILE"
  echo "✅ Task $id marked completed"
}

mark_failed() {
  local id="$1"
  set_status "$id" "failed"
  local tmp=$(mktemp)
  jq ".failed += [\"$id\"] | .current = null" "$STATE_FILE" > "$tmp" && mv "$tmp" "$STATE_FILE"
  echo "❌ Task $id marked failed"
}

# CLI
cmd="${1:-help}"
shift || true

case "$cmd" in
  list)        list_all ;;
  pending)     list_pending ;;
  completed)   list_completed ;;
  run)         run_task "$@" ;;
  done)        mark_done "$@" ;;
  fail)        mark_failed "$@" ;;
  status)      cat "$STATE_FILE" | jq '.' ;;
  help|*)
    echo "Usage:"
    echo "  $0 list              — Show all tasks"
    echo "  $0 pending           — Show pending tasks"
    echo "  $0 completed         — Show completed tasks"
    echo "  $0 run <task_id>     — Run a task"
    echo "  $0 done <task_id>    — Mark task completed"
    echo "  $0 fail <task_id>    — Mark task failed"
    echo "  $0 status            — Show current state"
    ;;
esac
