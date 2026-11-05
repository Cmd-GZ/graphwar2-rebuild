#!/usr/bin/env bash
# 客户端的小启动器：负责"第一次进来先 import"这件事，然后按参数跑一种模式。
#
#   bash client/run.sh             # 正常启动（需要显示器 / DISPLAY）
#   bash client/run.sh selftest    # 无显示器自检（93 条断言，退出码 0/1）
#   bash client/run.sh dump        # 打印节点树
#   bash client/run.sh shots       # 逐界面截图（需要 Xvfb）
#
# Godot 路径默认取仓库根的 tools/Godot_v4.7.2-stable_linux.x86_64，可用 GODOT=... 覆盖。
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ws="$(cd "$here/../.." && pwd)"
godot="${GODOT:-$ws/tools/Godot_v4.7.2-stable_linux.x86_64}"

# Godot 会往 XDG 目录写东西；指到工作区里，别污染 ~（本机家目录受限）
export XDG_DATA_HOME="${XDG_DATA_HOME:-$ws/.agent-work/client/data}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-$ws/.agent-work/client/cache}"
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$ws/.agent-work/client/config}"

if [ ! -d "$here/.godot" ]; then
    echo "[run.sh] 首次运行，先让 Godot 扫描资源（生成 .godot/ 与 class 缓存）"
    "$godot" --headless --path "$here" --import >/dev/null 2>&1 || true
fi

mode="${1:-gui}"
case "$mode" in
    gui)      exec "$godot" --path "$here" ;;
    selftest) exec "$godot" --headless --path "$here" -- --selftest ;;
    dump)     exec "$godot" --headless --path "$here" -- --dump-tree ;;
    shots)    exec bash "$here/tools/shots.sh" "${2:-$ws/.agent-work/client/shots}" ;;
    *)        echo "unknown mode: $mode (gui|selftest|dump|shots)" >&2; exit 2 ;;
esac
