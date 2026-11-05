#!/bin/bash
# 启动真游戏并在端口就绪后立刻尝试加入（判据 3 的实验脚手架）
set -u
ROOT=/home/user/MyProject/gw2
cd "$ROOT"
rm -f godot-live/project.binary
XDG_DATA_HOME=$ROOT/.godot-data XDG_CACHE_HOME=$ROOT/.godot-cache \
XDG_CONFIG_HOME=$ROOT/.godot-config GW2_START_FRAME=2400 \
  timeout 120 ./tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path ./godot-live > $ROOT/.rea-work/live_join.log 2>&1 &
GAME=$!
for i in $(seq 1 60); do
  PORT=$(ss -tlnp 2>/dev/null | grep -i godot | grep -oE '[0-9]+ +\*:\*|0\.0\.0\.0:[0-9]+|\*:[0-9]+' | grep -oE '[0-9]{4,5}' | head -1)
  if [ -n "${PORT:-}" ]; then echo "port=$PORT (waited ${i}x0.5s)"; ALLPORTS=$(ss -tlnp 2>/dev/null | grep -i godot | grep -oE ":[0-9]{4,5}" | tr -d ":" | sort -u | tr "\n" " "); echo "候选端口: $ALLPORTS"; break; fi
  sleep 0.5
done
if [ -z "${PORT:-}" ]; then echo "NO_PORT"; kill $GAME 2>/dev/null; exit 1; fi
sleep 3
for P in $ALLPORTS; do
  echo "--- 试端口 $P ---"
  timeout 15 python3 gw2-rebuild/tools/join_real_server.py "$P" 2>&1 | head -6
done
kill $GAME 2>/dev/null
