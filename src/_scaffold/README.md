# `src/_scaffold/` —— **这些不是原版的东西**

> ⚠️ **一句话**：`src/_scaffold/` 里的每个文件都是**我们**加的桥接层、工具层或替代实现，
> 原版 `libgraphwarrust.so` 里**没有**对应模块。**不要**把它们当成复原成果，也不要给它们起
> `graphwar_*.rs` 这种原版名（`AGENTS.md` 铁律 3 / §0 第 3 条）。

原版 `src/` 只有 **58 个 `graphwar_*.rs` + 12 个非 `graphwar_` 前缀的游戏模块**
（名单在 [`docs/coverage-matrix.tsv`](../../docs/coverage-matrix.tsv)）。
`src/` 根目录下的其它文件都能指到其中某个模块（对照表见
[`docs/02-module-map.md` §17](../../docs/02-module-map.md)）；
指不到的，全部收在本目录。

## 逐文件

| 文件 | 是什么 | 为什么不在复原层 | 证据 / 说明 |
|---|---|---|---|
| `gdext.rs` | GDExtension 入口（`gdext_rust_init`，唯一的动态导出符号） | 原版的入口在 Godot 模块侧，不在这 58+12 个文件里 | 导出符号名是从 `.so` 里读出来的（`nm -D`），但**入口代码**本身无法归属到某个 `src/*.rs` |
| `gdext_class.rs` | `classdb_register_extension_class4` 等 ABI 封装 | 同上：注册动作不是游戏逻辑 | `docs/116-kernel-api.md` §3 |
| `capi.rs` | 对外 C ABI，供 Python `ctypes` 对拍 | 纯为验证而加，原版没有 | — |
| `api.rs` | 内核 → GDScript / JSON 的对外 API（`GW2Rebuild.path_calculate` …） | 原版把"给脚本用"这件事直接交给 Godot 模块反射，没有这一层 | `docs/108` §2.3、`docs/116`、`docs/138`（ODE 口径） |
| `protocol.rs` | 报文编解码装配层（`Frame` / `parse_message` / `emit_*`） | 原版没有同名模块：它的事件编解码分散在 `graphwar_event.rs` 与各 handler 里（我方还没按那些边界拆） | 线格式来自真实抓包：`docs/07`、`docs/33`、`fixtures/protocol*.jsonl` |
| `broker.rs` | **我们自己的一台** broker（房间列表）服务端 | 原版只有**连** broker 的客户端，没有服务端 | `docs/115` |
| `rng.rs` | ChaCha12 的重写 | 原版用的是 `rand_chacha` crate（第三方依赖），不是游戏模块 | 状态布局 `docs/53` |
| `replay_inflate.rs` | 手写 DEFLATE / gzip（RFC 1951/1952） | 原版用 `flate2`；本 crate 零依赖，所以自己写 | `docs/133` §6 |
| `replaydiff.rs` | 回放**消息级**对拍器（工具） | 原版没有这种工具模块 | `docs/136` |
| `diffconv.rs` | 上一条的收敛层（模式感知预测 + 分歧归因） | 同上 | `docs/142` |
| `ai_explain.rs` | AI 决策链解释器（把 evolutionator 的零件按原版顺序跑一遍并留痕） | 同上：它**不引入新行为**，只是把已对过地址的零件串起来读 | `docs/147` |

## 与"混装"的区别

本目录的文件**每一个**整体都是我方自造；而 `src/events.rs` / `game.rs` / `plane.rs` /
`server.rs` / `turnstate.rs` 是**复原文件里混了多个原版模块**，它们留在 `src/` 根目录，
登记在 [`docs/02-module-map.md` §17.4](../../docs/02-module-map.md) 的"待拆分"表里。
两者不要混为一谈：前者是"原版没有"，后者是"原版有、我们还没拆干净"。

## 依赖方向

复原层**不**依赖本目录，本目录**可以**依赖复原层。唯一的例外需要单独说明：

* `graphwar_game_server.rs` / `game.rs` 等复原文件用 `crate::_scaffold::protocol` 产出报文字节
  —— 因为"把结构体写成线上 JSON"这一层的原版模块边界还没解，先用装配层顶着。
  这是一处**已知的桥接**，等 `graphwar_event.rs` 的 serde 边界拆出来就该去掉。
