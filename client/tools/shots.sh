#!/usr/bin/env bash
# 逐界面截图（需要 Xvfb + OpenGL；headless 拿不到画面）。
#
#   bash client/tools/shots.sh [输出目录]
#
# 产物是我们自己界面的截图，不含任何游戏资产。
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
client="$(dirname "$here")"
ws="$(cd "$client/../../../.." && pwd)"
out="${1:-$ws/.agent-work/client/shots}"
godot="${GODOT:-$ws/tools/Godot_v4.7.2-stable_linux.x86_64}"

export XDG_DATA_HOME="${XDG_DATA_HOME:-$ws/.agent-work/client/data}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-$ws/.agent-work/client/cache}"
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$ws/.agent-work/client/config}"

mkdir -p "$out"
xvfb-run -a "$godot" --path "$client" --rendering-driver opengl3 --resolution 1280x720 \
    -- --shots="$(cd "$out" && pwd)" 2>&1 | grep -v "^$" | tail -20
echo "screenshots in $out"
