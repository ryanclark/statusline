#!/bin/bash
# Generates screenshots of the statusline with different segment configurations.
# Opens Claude Code in iTerm2 for each config so the statusline renders natively.
#
# Usage:
#   ./scripts/screenshot.sh              # generate all screenshots
#   ./scripts/screenshot.sh --dir shots  # save to shots/ instead of screenshots/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
OUT_DIR="$REPO_DIR/screenshots"
WIDTH=780
HEIGHT=320

ONLY=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir)    OUT_DIR="$REPO_DIR/$2"; shift 2 ;;
    --width)  WIDTH="$2"; shift 2 ;;
    --height) HEIGHT="$2"; shift 2 ;;
    --only)   ONLY="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

mkdir -p "$OUT_DIR"

SETTINGS_DIR="$HOME/.statusline"
SETTINGS_FILE="$SETTINGS_DIR/settings.json"
SETTINGS_BACKUP=""
CLAUDE_SETTINGS="$HOME/.claude/settings.json"
CLAUDE_SETTINGS_BACKUP=""

# Back up existing settings
if [[ -f "$SETTINGS_FILE" ]]; then
  SETTINGS_BACKUP="$SETTINGS_FILE.screenshot-backup"
  cp "$SETTINGS_FILE" "$SETTINGS_BACKUP"
fi

# Back up Claude Code settings and point statusLine at local build
if [[ -f "$CLAUDE_SETTINGS" ]]; then
  CLAUDE_SETTINGS_BACKUP="$CLAUDE_SETTINGS.screenshot-backup"
  cp "$CLAUDE_SETTINGS" "$CLAUDE_SETTINGS_BACKUP"
  python3 -c "
import json
with open('$CLAUDE_SETTINGS') as f: d = json.load(f)
d['statusLine'] = {'type': 'command', 'command': '$REPO_DIR/target/release/statusline'}
with open('$CLAUDE_SETTINGS', 'w') as f: json.dump(d, f, indent=2)
"
fi

MOCK_DIR="$HOME/statusline"

cleanup() {
  if [[ -n "$SETTINGS_BACKUP" && -f "$SETTINGS_BACKUP" ]]; then
    mv "$SETTINGS_BACKUP" "$SETTINGS_FILE"
  elif [[ -z "$SETTINGS_BACKUP" && -f "$SETTINGS_FILE" ]]; then
    rm "$SETTINGS_FILE"
  fi
  if [[ -n "$CLAUDE_SETTINGS_BACKUP" && -f "$CLAUDE_SETTINGS_BACKUP" ]]; then
    mv "$CLAUDE_SETTINGS_BACKUP" "$CLAUDE_SETTINGS"
  fi
  rm -rf "$MOCK_DIR"
}
trap cleanup EXIT

# Create ~/statusline as a git repo with dirty state, ahead/behind, and a stash
if [[ -e "$MOCK_DIR" ]]; then
  echo "Error: $MOCK_DIR already exists. Remove it first." >&2
  exit 1
fi
mkdir -p "$MOCK_DIR"

(
  cd "$MOCK_DIR"
  git init -q
  git checkout -q -b main

  echo "hello" > README.md
  git add README.md
  git commit -q -m "initial commit"

  git clone -q --bare . upstream.git
  git remote add origin "$MOCK_DIR/upstream.git"
  git fetch -q origin
  git branch -q --set-upstream-to=origin/main main

  echo "change 1" >> README.md
  git add README.md
  git commit -q -m "local change 1"
  echo "change 2" >> README.md
  git add README.md
  git commit -q -m "local change 2"

  echo "wip" > wip.txt
  git add wip.txt
  git stash -q

  # Add CLAUDE.md so Claude Code skips the splash/tips panel
  echo "" > CLAUDE.md
  git add CLAUDE.md
  git commit -q -m "add CLAUDE.md"

  # Make the working tree dirty (uncommitted change)
  echo "dirty" >> README.md
)

# Read org_id from existing settings
ORG_ID="org-example"
if [[ -n "$SETTINGS_BACKUP" ]]; then
  ORG_ID=$(python3 -c "import json; print(json.load(open('$SETTINGS_BACKUP'))['org_id'])" 2>/dev/null || echo "org-example")
fi

# Define screenshot configs as parallel arrays
CONFIG_NAMES=(
  default
  context-window
  rate-limits
  cost-performance
  git-info
  environment
  kitchen-sink
)

CONFIG_SEGMENTS=(
  'null'

  '["context_percentage", "input_tokens", "output_tokens", "divider", "cache_read_tokens", "cache_hit_ratio", "divider", "context_remaining", "context_window_size"]'

  '["context_percentage", "input_tokens", "output_tokens", "divider", "five_hour", "seven_day"]'

  '["context_percentage", "divider", "cost", "cost_rate", "tokens_per_second", "divider", "duration", "api_duration", "divider", "lines_added", "lines_removed"]'

  '["context_percentage", "input_tokens", "output_tokens", "divider", "cwd", "divider", {"type": "git_branch", "dirty": true}, "git_ahead_behind", "git_stash"]'

  '["context_percentage", "divider", "cwd", "divider", "model", "divider", "version", "divider", "five_hour", "seven_day"]'

  '["context_percentage", "input_tokens", "output_tokens", "divider", "cwd", "divider", {"type": "git_branch", "dirty": true}, "divider", "model", "divider", "five_hour", "seven_day", "divider", "cost", "tokens_per_second", "lines_added", "lines_removed"]'
)

write_settings() {
  local segments="$1"
  if [[ "$segments" == "null" ]]; then
    cat > "$SETTINGS_FILE" <<SETTINGS
{
  "org_id": "$ORG_ID",
  "five_hour_reset_threshold": 70,
  "seven_day_reset_threshold": 100
}
SETTINGS
  else
    cat > "$SETTINGS_FILE" <<SETTINGS
{
  "org_id": "$ORG_ID",
  "five_hour_reset_threshold": 40,
  "seven_day_reset_threshold": 50,
  "segments": $segments
}
SETTINGS
  fi
}

capture_screenshot() {
  local name="$1"
  local segments="$2"
  local output="$OUT_DIR/$name.png"

  # Clear clipboard so Claude Code doesn't show "Image in clipboard" indicator
  echo -n " " | pbcopy

  write_settings "$segments"
  echo "  Settings: $(cat "$SETTINGS_FILE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('segments','default'))" 2>/dev/null)"

  # Open iTerm2 window and launch claude interactively
  osascript <<EOF
tell application "iTerm"
  activate
  set newWindow to (create window with default profile)
  tell newWindow
    set bounds of newWindow to {100, 200, $((100 + WIDTH)), $((200 + HEIGHT))}
    tell current session of current tab
      write text "cd ~/statusline && clear && CLAUDE_CODE_HIDE_ACCOUNT_INFO=1 claude"
    end tell
  end tell
end tell
EOF

  echo "  Waiting for Claude Code to start ($name)..."
  sleep 3

  # Type "hello" and press Enter via iTerm2
  osascript <<EOF
tell application "iTerm"
  tell current session of front window
    write text "hello"
  end tell
end tell
EOF

  echo "  Waiting for response..."
  sleep 12

  # Get the iTerm2 window ID and screenshot
  local wid
  wid=$(osascript -e 'tell application "iTerm" to return id of front window')
  screencapture -l"$wid" "$output"

  # Close the window
  osascript -e 'tell application "iTerm" to close front window' 2>/dev/null || true
  sleep 1

  if [[ -f "$output" ]]; then
    echo "  OK $output"
  else
    echo "  FAIL $name"
  fi
}

echo "Generating statusline screenshots..."
echo "Each config opens Claude Code, waits for a response, then screenshots."
echo ""

for i in "${!CONFIG_NAMES[@]}"; do
  if [[ -n "$ONLY" && "${CONFIG_NAMES[$i]}" != "$ONLY" ]]; then
    continue
  fi
  capture_screenshot "${CONFIG_NAMES[$i]}" "${CONFIG_SEGMENTS[$i]}"
done

echo ""
echo "Done. Screenshots saved to $OUT_DIR/"
