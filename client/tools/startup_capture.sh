#!/usr/bin/env bash
# 抓 Godot **真窗口**的启动序列，逐帧报告主导色 —— 用来证明"没有启动画面闪烁"。
#
#   bash client/tools/startup_capture.sh [秒数] [输出目录]
#
# 为什么要它：`boot_splash/bg_color` / `default_clear_color` 这类东西在 `--headless`
# 下**完全看不见**（没有窗口）。只有 Xvfb + 录屏逐帧，才能区分
#   * Godot logo（`boot_splash/show_image=true` 时约 4% 像素被覆盖）
#   * Godot 默认启动底色 #242424（没写 `boot_splash/bg_color`）
#   * 我们自己的清屏色
# 见 docs/121 §2。
#
# 依赖：Xvfb、ffmpeg。产物（mp4 + ppm 序列 + 报告）落在输出目录，**不入库**。
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
client="$(dirname "$here")"
ws="$(cd "$client/../.." && pwd)"
secs="${1:-8}"
out="${2:-$ws/.agent-work/client/startup}"
godot="${GODOT:-$ws/tools/Godot_v4.7.2-stable_linux.x86_64}"

export XDG_DATA_HOME="${XDG_DATA_HOME:-$ws/.agent-work/client/data}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-$ws/.agent-work/client/cache}"
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$ws/.agent-work/client/config}"

rm -rf "$out"; mkdir -p "$out"
display=:$((70 + RANDOM % 25))
Xvfb "$display" -screen 0 1280x720x24 -nolisten tcp >"$out/xvfb.log" 2>&1 &
xvfb_pid=$!
sleep 1.0
DISPLAY=$display ffmpeg -loglevel error -f x11grab -video_size 1280x720 -framerate 60 \
    -i "$display" -t "$secs" -y "$out/cap.mp4" >"$out/ffmpeg.log" 2>&1 &
ff_pid=$!
sleep 0.6
DISPLAY=$display "$godot" --path "$client" --rendering-driver opengl3 \
    --resolution 1280x720 --position 0,0 >"$out/godot.log" 2>&1 &
g_pid=$!
wait $ff_pid
kill $g_pid 2>/dev/null; sleep 0.3; kill $xvfb_pid 2>/dev/null; wait 2>/dev/null
ffmpeg -loglevel error -i "$out/cap.mp4" -fps_mode passthrough "$out/f%04d.ppm" -y >/dev/null 2>&1

python3 - "$out" <<'PY'
import glob, os, sys
from collections import Counter

def read_ppm(p):
    data = open(p, 'rb').read(); i = 0; f = []
    while len(f) < 4:
        while data[i:i+1].isspace(): i += 1
        if data[i:i+1] == b'#':
            while data[i:i+1] not in (b'\n', b''): i += 1
            continue
        s = i
        while not data[i:i+1].isspace(): i += 1
        f.append(data[s:i])
    i += 1
    w, h = int(f[1]), int(f[2])
    return w, h, data[i:i + w * h * 3]

d = sys.argv[1]
files = sorted(glob.glob(os.path.join(d, 'f*.ppm')))
runs, prev = [], None
for p in files:
    w, h, px = read_ppm(p)
    cnt = Counter()
    for k in range(0, w * h, 16):
        o = k * 3
        cnt[(px[o], px[o+1], px[o+2])] += 1
    c, n = cnt.most_common(1)[0]
    sig = (c, round(n / sum(cnt.values()), 2))
    if runs and runs[-1][0] == sig:
        runs[-1][2] = os.path.basename(p)
    else:
        runs.append([sig, os.path.basename(p), os.path.basename(p)])
print("frames: %d  (dir: %s)" % (len(files), d))
for s, a, b in runs:
    print("  %s..%s  dominant=#%02x%02x%02x  %.0f%%" % (a, b, s[0][0], s[0][1], s[0][2], s[1] * 100))
print("\n判据（docs/121 §2）：不应出现 #242424（Godot 默认启动底色），"
      "也不应出现两段不同的底色相撞；修复后除黑屏外应只有一个底色。")
PY
