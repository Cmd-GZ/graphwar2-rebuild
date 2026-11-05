# 49 · 验收清单（每条都能照着跑）

本文件的每条命令都在本轮**实际执行过**，输出摘录在下面。
没跑过的条目不会出现在这里。

## 前置

```bash
cd gw2-rebuild
export CARGO_HOME=$PWD/../.cargo-home CARGO_TARGET_DIR=$PWD/../.cargo-home/target
```

需要游戏二进制的条目还要：

```bash
ln -sf ../libgraphwarrust.so .          # 或把仓库根目录的 .so 复制进来
python3 -m venv ../.venv && ../.venv/bin/pip install -r requirements.txt
```

## 1. 单元 + 集成测试

```bash
cargo test --offline
```

实测：**通过 106，失败 0**。

## 2. 构建出可加载的二进制

```bash
cargo build --release --offline
nm -D --defined-only $CARGO_TARGET_DIR/release/libgw2_rebuild.so
```

实测导出 12 个符号：
`gdext_rust_init`、`gw2_correction`、`gw2_is_occupied`、`gw2_parse_len`、
`gw2_rasterize`、`gw2_rebuild_*`（4 个自检）、`gw2_shot_points`、`gw2_simulate`。

## 3. 报文逐字节一致

```bash
OURS=$(cargo run --quiet --offline --example emit_broadcast -- "[::1]:43717")
CAP=$(grep -m1 '"address":"\[::1\]:43717"' fixtures/protocol_match.jsonl)
[ "$OURS" = "$CAP" ] && echo 一致 || echo 不一致
```

实测：**一致**。

## 4. 真实数据集成测试

```bash
cargo test --offline --test live_hits
```

实测：**12 passed**。覆盖：
真实关卡回放（4 发 / 6 发）、两次致死、事件序列、按回合配对的精确回放。

## 5. 与真二进制对拍（需要 `.so` + unicorn）

```bash
../.venv/bin/python tools/capi_check.py       # 曲线 API
../.venv/bin/python tools/mask_check.py       # 光栅化逐字节
../.venv/bin/python tools/mask_api_check.py   # 点测试随机对拍
../.venv/bin/python tools/simulate_check.py   # 开火模拟装配
```

实测：四条全部 **RESULT: PASS**。

## 6. 服务端握手（真实 TCP）

```bash
cargo run --quiet --offline --example serve_handshake -- 127.0.0.1:39991 &
```

另开一个客户端发一帧 `NewConnectionRequest`，应收到 7 帧、324 字节。
实测输出：

```
服务端: ACCEPTED 127.0.0.1:59000
        GOT {"NewConnectionRequest":{"major_version":"2.0","minor_version":"60"}}
        SENT 7 frames, 324 bytes
```

## 7. 真引擎加载（需要 Godot 4.7.2 与 `godot-probe/`）

```bash
export XDG_DATA_HOME=$PWD/../.godot-data XDG_CACHE_HOME=$PWD/../.godot-cache \
       XDG_CONFIG_HOME=$PWD/../.godot-config
../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path ../godot-probe
```

实测：`GW2_HAS_CLASS=true`、`GW2_INSTANCE=ok`、`GW2_HAS_PING=true`，
并打印 `[gw2-rebuild] ping() called from GDScript`。

## 8. 实时对局探针（需要 frida + `godot-live/`）

```bash
node tools/hook_live_game.js        # 玩法函数调用计数
node tools/capture_net_raw.js 20000 60   # 抓真实报文
```

## 覆盖范围提醒

以上验证的范围是**非随机**的部分。三行随机驱动的模块已证不可复现
（`docs/40`、`docs/47`、`docs/48`），不在本清单的覆盖范围内。
