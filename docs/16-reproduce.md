# 16 · 如何验证这份复原

给接手的人一页可操作说明：**怎么确认这些代码是对的**，以及每个真值从哪来。

## 1. 两个仓库的分工

| 仓库 | 内容 | 远端 |
|---|---|---|
| `gw2`（上游） | 曲线算法的论证（`REPORT.md`）+ 对拍工具链（`analysis/`）+ 已验证的 Python 参照实现 | `Cmd-GZ/graphwar2-analysis` |
| `gw2-rebuild`（本仓库） | 复原实现（Rust）+ 观测量与推断记录（`docs/`） | `Cmd-GZ/graphwar2-rebuild` |

本仓库**不包含**二进制、反编译产物或夹具以外的原始数据。上游也从没收录过本仓库的任何文件
（可用 `git -C .. ls-files | grep gw2-rebuild` 验证，应为 0 行）。

## 2. 跑测试（不需要二进制）

```bash
cd gw2-rebuild
export CARGO_HOME=$PWD/../.cargo-home CARGO_TARGET_DIR=$PWD/../.cargo-home/target
cargo test --offline
```

期望：**21 条通过**。其中

| 组 | 条数 | 依赖 |
|---|---|---|
| 常量锚定 | 6 | 无（断言写的是 `.rodata` 里的比特） |
| 金标准 | 4 | `fixtures/*.tsv`（已入库） |
| 坐标系 + 掩膜 | 6 | 无 |
| 形状（事件/生命） | 5 | 无 |

`fixtures/` 已提交，所以**只跑测试不需要游戏二进制**——这是刻意的。

## 3. 真值从哪来

| 夹具 | 用例 | 真值来源 |
|---|---|---|
| `parse.tsv` | 37 | 上游已验证的 Python 参照实现 |
| `eval.tsv` | 70 | **真二进制**：unicorn 调用 `0x4eb9d0` |
| `path.tsv` | 224 | **真二进制**：unicorn 调用 `0x3c2530` |
| `module_map*.tsv` | — | 静态索引（非真值，是归属统计） |

重新生成（需要二进制与 `.venv`）：

```bash
../.venv/bin/python tools/gen_fixtures.py
```

## 4. 用真二进制复核全链路

真正的"标准答案"在上游那套对拍里，本仓库**不自我证明**：

```bash
cd .. && .venv/bin/python analysis/difftest.py         # 768 例（原 704 + 64 条早停回归用例）
.venv/bin/python analysis/difftest_parser.py           # 90 例
.venv/bin/python analysis/final_verify.py              # 648 + 40
.venv/bin/python analysis/fuzz.py 300 7                # 随机语法树
```

若本仓库与它们冲突，**错的一定是本仓库**。

## 5. 复现逆向环境（可选）

需要重新反编译时才用得上：

```bash
# 1) Ghidra 12.1.4（REA 只接受 12.1.x；12.2 会被拒）
export GHIDRA_INSTALL_DIR=$PWD/tools/ghidra_12.1.4_PUBLIC
export JAVA_HOME=/usr/lib/jvm/java-25-openjdk

# 2) 首次导入约 21 分钟，之后同会话内是秒级
# 3) REA 的启动预算要在每个进程里重设（运行时设置不跨重启）
#    update_analysis_settings: ghidra.startup_timeout_ms = 2400000
```

地址换算：**REA/Ghidra 地址 = ELF 虚地址 + 0x100000**。
`funcstarts_ghidra_va.pkl` 已经做过减法，是虚地址空间。

## 6. 环境坑（都踩过）

| 现象 | 原因 | 处理 |
|---|---|---|
| `npm EROFS` | `~/.npm` 只读 | 用工作区内的 `npm_config_cache` |
| `/tmp` 文件消失 | `/tmp` 是按命令独立的 tmpfs | 中间产物写进工作区 |
| `cargo` 找不到 registry | `~/.cargo` 只读 | 设 `CARGO_HOME` 到工作区 |
| git push 报 ssh 配置错误 | `/etc/ssh/ssh_config.d/` 权限问题 | `GIT_SSH_COMMAND="ssh -F /dev/null"` |
| REA 工具调用 60 秒超时 | 客户端默认值 | 注册里加 `toolCallTimeoutMs: 1800000` |
| 反编译里 `_DAT_xxxx` 读不对 | 那是 Ghidra 地址 | 读原始字节要减 `0x100000` |

## 7. 改动本仓库的规矩

1. **不得**把推断写成结论——文档里每个结论都要能指到"观测"或"推断"。
2. 改动 `src/` 的行为后，`cargo test` 必须仍全绿；若夹具需要更新，
   说明真值变了，必须回到 §4 用真二进制复核。
3. 新发现的观测先进 `docs/`，再考虑写代码。顺序反了容易把猜测固化成实现。
4. 上游 `REPORT.md` 若与本仓库冲突，先判断是不是上游错；是则修上游（本目标允许），
   不是则改本仓库。
