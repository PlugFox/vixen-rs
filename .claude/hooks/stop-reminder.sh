#!/usr/bin/env bash
# Stop hook: reminds about required checks when server/, website/, migrations/, or i18n/ files were edited.
# Reads the Stop-hook JSON event on stdin, extracts the transcript path,
# and scans tool_use entries for Edit/Write/NotebookEdit targets.

set -euo pipefail

# stop_hook_active=true means we're being re-entered from our own decision —
# bail out to avoid infinite loops (per Claude Code hook spec).
event=$(cat)
if printf '%s' "$event" | jq -e '.stop_hook_active == true' >/dev/null 2>&1; then
    exit 0
fi

transcript=$(printf '%s' "$event" | jq -r '.transcript_path // empty')
if [[ -z "$transcript" || ! -f "$transcript" ]]; then
    exit 0
fi

# Collect Edit/Write targets. Transcript is JSONL: one message per line.
paths=$(jq -r '
    select(.type == "assistant")
    | .message.content[]?
    | select(.type == "tool_use" and (.name == "Edit" or .name == "Write" or .name == "NotebookEdit"))
    | .input.file_path // empty
' "$transcript" 2>/dev/null || true)

server_touched=false
website_touched=false
migrations_touched=false
i18n_touched=false

while IFS= read -r p; do
    [[ -z "$p" ]] && continue
    case "$p" in
        */server/migrations/*.sql) migrations_touched=true; server_touched=true ;;
        */server/*.rs|*/server/*.toml) server_touched=true ;;
        */website/i18n/*.yaml|*/website/i18n/*.yml) i18n_touched=true; website_touched=true ;;
        */website/src/*.ts|*/website/src/*.tsx|*/website/src/*.css|*/website/*.json) website_touched=true ;;
    esac
done <<< "$paths"

if ! $server_touched && ! $website_touched; then
    exit 0
fi

msg="Reminder before reporting task as done:"
if $server_touched; then
    msg+=$'\n  server/ changed -> cd server && cargo fmt && cargo clippy -- -D warnings && cargo test'
fi
if $migrations_touched; then
    msg+=$'\n  migrations/ changed -> regenerate .sqlx (run /db-migrate or '"'"'cd server && cargo sqlx prepare -- --all-targets'"'"') and commit .sqlx/'
fi
if $website_touched; then
    msg+=$'\n  website/ changed -> cd website && bun run check && bun run typecheck && bun run build'
fi
if $i18n_touched; then
    msg+=$'\n  i18n/ changed -> verify EVERY locale file has the same keys'
fi

# additionalContext feeds the reminder back into the model without blocking.
jq -n --arg c "$msg" '{hookSpecificOutput: {hookEventName: "Stop", additionalContext: $c}}'
