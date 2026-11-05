# AGENTS.md —— gw2-rebuild 仓库约定

本仓库是 Graphwar II 的源码复原。**先读 [`docs/00-status.md`](docs/00-status.md) 和
[`docs/HANDOVER.md`](docs/HANDOVER.md)**，再动代码。

## 三条硬规矩

1. **不把推断写成结论。** 每个结论必须能标成四档之一：
   **逐位验证**（`cargo test` 覆盖）/ **观测**（能指到地址或字节）/ **推断**（写明未证实）/
   **未知**。文档里出现"应该是""大概是"这类措辞时，必须同时写清缺什么证据。
2. **代码不自我证明。** 行为正确性只由真二进制决定。改动 `src/` 后：
   `cargo test` 必须全绿；若夹具要改，说明真值变了，必须回到上游 `analysis/` 用
   unicorn 对拍复核（见 `docs/16-reproduce.md` §4）。
3. **不提交二进制衍生内容。** `rsc/`（游戏资源）、`*.pck`、反编译产物一律 gitignore。
   上游 `AGENTS.md` 有同样的规则，理由也适用于此处（体积 + 版权）。

## 目录

```
src/        Rust 复原实现（function/path/shot/plane/events/life）
tests/       与真二进制对齐的验收（21 条）
fixtures/   真值夹具（已入库；重新生成见 tools/gen_fixtures.py）
tools/       分析脚本（callers/ptr_slots/const_xrefs/pck_extract/gen_fixtures/module_map/propagate）
docs/        观测、推断、决策与操作说明（17 篇，索引在 README）
rsc/         ← gitignore，用 tools/pck_extract.py 生成
```

## 常用命令

```bash
export CARGO_HOME=$PWD/../.cargo-home CARGO_TARGET_DIR=$PWD/../.cargo-home/target
cargo test --offline                      # 21 条，无需游戏二进制

../.venv/bin/python tools/callers.py --module graphwar_turn_manager.rs
../.venv/bin/python tools/ptr_slots.py 0x3c2bc0      # 函数指针派发
../.venv/bin/python tools/const_xrefs.py 50.0 30.0   # 常量反查
../.venv/bin/python tools/pck_extract.py --verify    # 还原 rsc/
```

## 两个反复踩过的坑

- **地址空间**：反编译里的 `_DAT_xxxx` 是 **Ghidra 地址**，读原始字节要减 `0x100000`；
  但 `Caller`/`Callee` 类工具返回的也是 Ghidra 地址，别拿虚地址去查。
- **调用图会骗人**：Rust 大量用函数指针派发，直接调用为零**不代表**没被使用。
  写"未观测到调用"，并用 `tools/ptr_slots.py` 补查。

## 提交

不要直接 `git push`（本机 ssh 配置有权限问题）：

```bash
GIT_SSH_COMMAND="ssh -F /dev/null" git push origin main
```

上游 `Cmd-GZ/graphwar2-analysis` 与本仓库互不包含；本仓库的内容**绝不**提交到上游。
