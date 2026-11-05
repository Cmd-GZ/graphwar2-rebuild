#!/usr/bin/env bash
# 客户端的小启动器：负责"资产提取 + 第一次先 import"这两件事，然后按参数跑一种模式。
#
#   bash client/run.sh assets    # 从你的 Graphwar.pck 解出图标/字体到 client/assets/（首次必做）
#   bash client/run.sh           # 正常启动（需要显示器 / DISPLAY）
#   bash client/run.sh selftest  # 无显示器自检（93 条断言，退出码 0/1）
#   bash client/run.sh dump      # 打印节点树
#   bash client/run.sh shots     # 逐界面截图（需要 Xvfb）
#
# Godot 路径默认取仓库根的 tools/Godot_v4.7.2-stable_linux.x86_64，可用 GODOT=... 覆盖。
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ws="$(cd "$here/../.." && pwd)"
godot="${GODOT:-$ws/tools/Godot_v4.7.2-stable_linux.x86_64}"
extract="$here/../tools/extract_assets.py"
assets="$here/assets"

# Godot 会往 XDG 目录写东西；指到工作区里，别污染 ~（本机家目录受限）
export XDG_DATA_HOME="${XDG_DATA_HOME:-$ws/.agent-work/client/data}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-$ws/.agent-work/client/cache}"
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$ws/.agent-work/client/config}"

missing_assets_hint() {
    cat >&2 <<EOF
[run.sh] client/assets/ 不完整 —— 本仓库**不存放任何媒体文件**（图标/字体是原版资产）。
         先在你自己的 Graphwar.pck 上跑一次：

             python3 $extract                 # 默认用 $ws/Graphwar.pck
             python3 $extract --pck /path/Graphwar.pck

         产出目录 client/assets/ 已 gitignore；提完不用重新 --import。
EOF
}

# 资产就绪 = 有清单且清单里每个文件都在（缺一个都算没就绪）
assets_ready() {
    [ -f "$assets/MANIFEST.tsv" ] || return 1
    python3 "$extract" --check --out "$assets" >/dev/null 2>&1
}

if [ ! -d "$here/.godot" ]; then
    echo "[run.sh] 首次运行，先让 Godot 扫描资源（生成 .godot/ 与 class 缓存）"
    "$godot" --headless --path "$here" --import >/dev/null 2>&1 || true
fi

mode="${1:-gui}"
case "$mode" in
    assets)
        exec python3 "$extract" "${@:2}"
        ;;
    gui)
        if ! assets_ready; then missing_assets_hint; exit 4; fi
        exec "$godot" --path "$here"
        ;;
    shots)
        if ! assets_ready; then missing_assets_hint; exit 4; fi
        exec bash "$here/tools/shots.sh" "${2:-$ws/.agent-work/client/shots}"
        ;;
    selftest|dump)
        # 自检在缺资产时**照样跑**，但会显式跳过并报告图标/字体断言（docs/121 §6.3）。
        if ! assets_ready; then
            echo "[run.sh] 注意：没检测到完整的 client/assets/，图标/字体断言会被显式跳过" >&2
        fi
        if [ "$mode" = "selftest" ]; then
            exec "$godot" --headless --path "$here" -- --selftest
        fi
        exec "$godot" --headless --path "$here" -- --dump-tree
        ;;
    *)  echo "unknown mode: $mode (assets|gui|selftest|dump|shots)" >&2; exit 2 ;;
esac
