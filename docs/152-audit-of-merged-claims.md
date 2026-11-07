# 152 · 已合并结论的对抗性审计（独立复算）

状态：**审计已完成**（master `8755db9e` 上复算）。目的不是再次确认，而是**找出虚报**。
凡是复算通过的就写"一致"，不通过的写"不一致"并就地更正原文；复算不了的写"无法判定（缺什么）"。

## 0 口径：不用被审对象的工具

全部结论只用**本轮自建**的三样东西复算，不调用仓库里 `tools/*.py` 的判定（只把它当"待验声称"）：

| 工具 | 作用 |
|---|---|
| 自写 ELF64 读取器 | 段/节、`.rela.dyn`（24,669 条：RELATIVE 24,501 / GLOB_DAT 100 / JUMP_SLOT 55 / IRELATIVE 13） |
| capstone 5.0.7 线性扫描 `.text` | 1,520,816 条指令；`call/jmp rel32`、`call/jmp [rip+X]`、`lea/mov rip`、`movabs imm64` 全存索引 |
| `.eh_frame_hdr` 解析 | **13,430 个 FDE**（函数起点 + 长度），当函数边界裁判；与 `analysis/funcstarts_ghidra_va.pkl`（13,263 条）在本次涉及的全部地址上一致 |

地址口径（本轮**测过**，不是假定）：

* `analysis/funcstarts_ghidra_va.pkl` 里的值是 **Python `int`**，最小值 `0x37b940` = `.text` 的 `sh_addr`，
  最大值 `0x94eb70` < `.text` 末（`0x94eb84`）⇒ 它已经是 **ELF 虚地址**空间，名字里的 "ghidra_va" 是历史名。
  本轮所有地址按 **ELF vaddr** 读（Ghidra/REA = +`0x100000`，只加一次）。
* `.eh_frame_hdr` 的表项编码是 **`0x3b`（datarel sdata4）**，基址 = `.eh_frame_hdr` 自己的地址。
  若当成 `pcrel`（像 `funcstarts.pkl` 那样）解，会得到 13,430 个**错地址**——
  这大概就是 `docs/02` §12 "与 Ghidra 只交 2 个" 的来源（用 `datarel` 解，交集是 **11,881**）。

## 1 不一致（先看这几条）

| # | 文档 | 原值（声称） | 实测 | 影响 |
|---|---|---|---|---|
| A | `docs/10-campaign.md` §13.3 | `0x3c1b40` 出口是 `cmp` / **`setb`** | **`setg`**（`0x3c1c1c cmp %r8d,%eax`、`0x3c1c1f setg %al`） | **结论不变**（人数是小非负整数，`setg`≡`setb`；平手仍进 Team1）。只是条件码记错 |
| B | `docs/06-verification.md` §25.4 | arm64 `execute_results` 体 = `0x4a8558–0x4a8e50` | `.symtab` 尺寸 **3712 B** ⇒ `0x4a8558..0x4a93d8`；`0x4a8e50` 只是**热路径的 `ret`**，其后还有 `0x588` 字节 panic/冷块 | 起点对、**终点错**（少算 31%）。`pos_y` 那几条指令不受影响 |
| C | `docs/06-verification.md` §25.2 | 格式批原文对 **"13 种"**，随后只列了 **12 种**（15 批） | 实测 **13 种 / 16 批**，漏掉的是 `0.0000000000000000007606517635480697` vs `7.606517635480697e-19` | 数字对、**清单少一项**；154 批那条不受影响 |
| D | `docs/151-addr-open-items.md` §2.4 | "`0x9baa10` 的使用者**全在** `0x3axxxx` 的纯字符串比较族里" | 该槽有 **267 条 `call *` + 56 条 `mov`**，跨 **153 个函数**，遍布整个 `.text`（`0x3a/0x3d/0x44/0x4f/0x83/0x84/…`）。原文列的 `0x3abb06` 只是排序后的前几条 | 第 22 条的**结论（= `bcmp`）成立**；这句旁证是"只看前几条就推广" |

四处已就地更正（见 §4）。

## 2 逐项复算

### 2.1 项 1 —— `graphwar_module_container` 68 = 25 + 23 + 20，`0x4694f0` / `0x46a490` / `0x46c5a0`

| 声称 | 实测 | 判定 |
|---|---|---|
| arm64 `.symtab`：`add_module` 25 / `get_module` 23 / `get_module_wk` 20 | **25 / 23 / 20**（另有 1 个 `drop_in_place<Weak<RefCell<GraphwarModuleContainer>>>`，不计） | 一致 |
| x86 三块首 `0x4694f0` / `0x46a490` / `0x46c5a0`，步长 160 / 368 / 544，**零空隙** | 25 个地址全在 FDE 起点表里，排序后相邻差**恒为 160**（`0x4694f0..0x46a3f0`）；23 个恒 **368**（`0x46a490..0x46c430`）；20 个恒 **544**（`0x46c5a0..0x46ee00`）。`0x46a3f0+160 = 0x46a490`、`0x46c430+368 = 0x46c5a0` ⇒ 首尾相接成立 | 一致 |
| 容器 = `HashMap<TypeId, Rc<dyn Any>>` | `add_module` 体：`incq (%rsi)`（Rc::clone）→ `movups <16 B TypeId>,[rip]` → `lea <dyn Any vtable>,%rcx` → `lea rsp+0x10,%rsi` → `call 0x443bc0`（HashMap::insert）。**25 个 `add_module` 的 `rdi` 在 `call` 前一次都没被写**（`self` 原样传入） | 一致 |
| 容器 48 字节、一个字段 | `get_module`（`0x46a490`）只碰 `self+0x0`（ctrl）/`+0x8`（bucket_mask）/`+0x18`（items，空表快路径）/`+0x20`（16 B `RandomState`，`call 0x47e490` = hash_one）；`get_module_wk` 同一组偏移（`RcBox+0x30`=items、`RcBox+0x38`=RandomState ⇒ 容器 `+0x18`/`+0x20`）。最大触及偏移 `+0x28`（hasher 的第二字）⇒ 结构 = `0x30` | 一致（偏移式推断，非独立尺寸常量） |
| 25 个 manager **没有** `+0xNN` 偏移 | `add_module` 从不位移 `self`；取模块靠 `TypeId` 键 + vtable 比对 | 一致（否定结论成立） |
| 25 个类型的 `size` 列 | 逐 `add` 读 vtable 第 2 字（`= size_of::<RefCell<M>>`）− 8，**25/25 与文档的 `size` 列逐行相同**（64/120/72/128/32/72/168/112/96/32/72/48/56/72/32/72/72/72/32/24/72/296/72/72/80，按地址序） | 一致 |
| 装配点 22 / 20 / 1+1，且 `22+20−18+1 = 25` | `0x517690`：**22** 条；`0x4500c0`：**20** 条；`0x515590`/`0x5159a0`：各 **1** 条（`GameRecorder`）。交集 **18**，并集 24，+`GameRecorder` = 25 | 一致 |
| §22.2(c) 的尺寸序列 `80,120,72,…`（vtable 字） | 实测（减 8 后）`72,112,64,72,72,24,296,32,72,32,72,80,32,72,32,168,72,72,48,72,120,72`，逐位等于文档序列 | 一致 |

```bash
X=.reference/steam/2026-08-28/libgraphwarrust.so; A=.reference/steam/android-2.0.60/libgraphwarrust.so
readelf -sW $A | grep graphwar_module_container | grep FUNC \
  | grep -oE '(10add_module|10get_module|13get_module_wk)' | sort | uniq -c     # 25/23/20
objdump -d --no-show-raw-insn --start-address=0x4694f0 --stop-address=0x469590 $X   # add_module 全景
objdump -d --no-show-raw-insn --start-address=0x46a490 --stop-address=0x46a500 $X   # +0x18/+0x20 偏移
# vtable 尺寸：读 0x958398+8（第 k 个 add 的 lea 目标见 §22.3 表）
```

### 2.2 项 2 —— `weak_caller` 40 = 39 单态化 + 1 内联调用者；39 = 23 + 16

| 声称 | 实测 | 判定 |
|---|---|---|
| x86 `call` 23 个，`0x3d10e0..0x3d2520` | **23**：22 个能按"`cmp rdi,-1` + `movq $-1,0x10(..)` 借用闸"直接认出，第 23 个是 `0x3d22a0`（闭包内联、无间接调用）。它的归属另有一条硬证据（见下） | 一致 |
| x86 `call_0` 16 个，`0x3d2610..0x3d3330` | **16**，区间端点逐位相同 | 一致 |
| 40 = 39 + 1（内联调用者 `0x3d4fa0`，704 B） | `0x3d4fa0` 跨度 **704 B**，体内同时有 `graphwar_game_recorder.rs:107/108` 与 **`weak_caller.rs:17`** 三条锚 ⇒ 确实是"内联了 `call` 的 recorder 方法"，不是单态化 | 一致 |
| 23 / 16 的**分界** | 两条 `panic::Location` 各自的使用者集合**恰好分开**：`weak_caller.rs:17` → 23 个 `call`（含 `0x3d22a0`）+ `0x3d4fa0`；`weak_caller.rs:11` → 16 个 `call_0`。**零交叉** | 一致 |
| 39 个 `RcBox` 尺寸 = 24 + `size_of(T)` | 逐函数读 `mov esi,<n>` / `mov edx,8` 的 dealloc 参数：**39/39** 等于 `24 + 文档的 size_of(T)` | 一致 |
| 13 双有 / 10 只有 `call` / 3 只有 `call_0`，共 26 型 | 由 26/23/16 代数强制（13+10=23、13+3=16）。**独立校核**：按尺寸分组，`size=72` 的型 10 个（其中 7 个双有）⇒ 应出现 `7×2+3 = 17` 个 dealloc=96 的函数 —— 实测**正好 17**；`size=32` 的 4 型 ⇒ `1+1+1+2 = 5` 个 dealloc=56 —— 实测**正好 5**；其余 12 型各 1 或 2 个，14 个不同尺寸的计数**逐项吻合**（48×2/56×5/72×1/88×1/96×17/104×1/120×1/136×2/144×2/152×2/168×1/192×2/240×1/320×1） | 一致（结构闭环） |
| arm64 35 = 21 + 14 | `.symtab` 里 `WeakCaller4call`（21）+ `WeakCaller6call_0`（14）= **35**，地址与文档 arm64 两列逐位相同 | 一致 |

> **无法独立复核的残余**：具体 `T` 的**名字**。x86 侧被 strip，名字只能来自 `f` 实参归属或 arm64 侧推断；
> 本轮只验到"数量 / 尺寸 / 两方法的分组 / 锚点分组"全部闭环，**没有**独立重做 26 个类型名的指派。

```bash
X=.reference/steam/2026-08-28/libgraphwarrust.so
objdump -d --no-show-raw-insn --start-address=0x3d10e0 --stop-address=0x3d11d0 $X  # call 型体（含 dealloc 尺寸 0x38）
objdump -d --no-show-raw-insn --start-address=0x3d2610 --stop-address=0x3d2700 $X  # call_0 型体
objdump -d --no-show-raw-insn --start-address=0x3d22a0 --stop-address=0x3d2340 $X  # 内联例外（movb $0x1,0xa3(%rdi)）
# 39 个的整体枚举：见 §3 的自建扫描器（形状签名 cmp rdi,-1 + 借用闸 + dealloc(24+size_of(T))），
# 以及真正分开 23/16 的那条证据：weak_caller.rs:17 / :11 两条 Location 的引用者集合（零交叉）
```

### 2.3 项 3 —— `pick_team` `0x3c1b40` 与整房 10 人 `0x4739c0`

| 声称 | 实测 | 判定 |
|---|---|---|
| 谓词 = `Σteam < Σ(team^1)` ⇒ 进人少的队 | `0x3c1bdb add %r9d,%r8d`（`r8d`=Σteam）、`0x3c1bde xor $0x1,%r9d` + `0x3c1be2 add %r9d,%eax`（`eax`=Σ(team^1)）；`0x3c1c1c cmp %r8d,%eax` + **`0x3c1c1f setg %al`** ⇒ al = `Σ(team^1) > Σteam` | 一致（**但条件码名字见 §1-A**） |
| 平手进 Team1 | 平手时 `>` 为假 ⇒ al=0；空表分支 `0x3c1b90 test %rdx,%rdx; je` 直接把 `eax` 置 0 ⇒ 第一个玩家拿 0 | 一致 |
| 返回值直接当 team 传下去 | `0x473ae0 call 0x3c1b40` → `0x473b05 movzbl %al,%ebp` → `0x473b0a mov %ebp,%edx` → `0x473b0c call 0x3c17d0`（`add_to_team`，`esi`=`%ebx`=实体号） | 一致 |
| 整房上限 10（`0x4739c0`） | `0x473a1f mov 0x48(%r15),%rcx`（玩家表 len）→ `0x473a34 cmp $0xa,%rcx` → `0x473a38 jae` ⇒ `len >= 10` 就不再收 | 一致 |
| `game_state[0x120] == 1` 才分队 | `0x4739ce cmpb $0x1,0x120(%rdi)` → `jne` | 一致 |

```bash
objdump -d --no-show-raw-insn --start-address=0x3c1b40 --stop-address=0x3c1c40 $X
objdump -d --no-show-raw-insn --start-address=0x4739c0 --stop-address=0x473b20 $X
```

### 2.4 项 4 —— `computer_player_manager` 的 4/11/26/101/251、10..50、5..200

调用点 `0x3e2830`（12,512 B 的事件分派器，`case 8` = 判别值 10）里的一条链，
四个参数在 `0x3e45cd call 0x478fc0` 前全部就位（`esi`=连接号、`edx`=level、`ecx`=精英数、`r8d`=5、`r9d`=limit）：

```text
3e3d95  mov $0xa,%ecx ; mov $0x5,%r9d          ; 档0: 10 / 5
3e3da0  cmp $0x4,%r14d  ; jb  -> 用上面那组
3e3daa  mov $0x14,%ecx                         ; 档1: 20 / 5
3e3daf  cmp $0xb,%r14d  ; jb  -> 用上面那组
3e3db9  cmp $0x1a,%r14d ; jae 3e456d
3e3dc3  mov $0x1e,%ecx ; mov $0xa,%r9d         ; 档2: 30 / 10
3e456d  cmp $0x65,%r14d ; jae 3e458d
3e4573  mov $0x28,%ecx ; mov $0xf,%r9d         ; 档3: 40 / 15
3e458d  cmp $0xfb,%r14d ; mov $0x19,%eax ; mov $0xc8,%r9d ; cmovb %eax,%r9d
3e45a3  mov $0x32,%ecx                         ; 档4/5: 50 / (25 或 200)
3e45c7  mov $0x5,%r8d  ; 3e45cd call 0x478fc0
```

| 声称 | 实测 | 判定 |
|---|---|---|
| 阈值 4/11/26/101/251 → 档 0..5 | `jb 4` / `jb 0xb` / `jae 0x1a` / `jae 0x65` / `jae 0xfb`（`0x65=101`、`0xfb=251`） | 一致 |
| 精英数 10/20/30/40/50/50 | `ecx` = `0xa/0x14/0x1e/0x28/0x32` = 10/20/30/40/50/50 | 一致 |
| `limit` 5/5/10/15/25/200 | `r9d` = `5/5/0xa/0xf/(cmovb: 25 否则 200)` | 一致 |
| 第 5 个实参是常数 5 | `0x3e45c7 mov $0x5,%r8d` | 一致 |

> 只**逐条读出了立即数与寄存器指派**（`ecx`=第 4 实参、`r9d`=第 6 实参）；"哪个叫精英数、哪个叫 limit"
> 仍依赖文档对 §1 的语义对齐，本轮没有独立追进 `0x478fc0` 的字段用途。

```bash
objdump -d --no-show-raw-insn --start-address=0x3e3d90 --stop-address=0x3e45d2 $X
```

### 2.5 项 5 —— `0x467530` 通视裁定：17 个调用者 + 零函数指针

| 声称 | 实测 | 判定 |
|---|---|---|
| 17 个 `call rel32`，落在 17 个不同函数 | **17 条**，逐条与 §5.7 表的调用地址**逐位相同**（`0x466207`…`0x4aceaa`），落在 **17** 个不同 FDE 起点 | 一致 |
| `jmp rel32`（尾调用）0 | **0** | 一致 |
| RELATIVE 槽 / `lea` / `imm64` 三形态全 0 | 槽 **0**、`lea/mov rip` **0**、`movabs` **0** | 一致 |
| 调用者全是关卡构造 | 自建 `panic::Location` 解析（2,042 条记录 → 按 rip 引用投票）：`graphwar_level_manager.rs` **16**、`graphwar_level_generator.rs` **1**、其它 0 | 一致 |
| 正对照：采样器 `0x3c2530` → 槽 `0x9bac98`，使用者恰好 2 处 | 槽表：addend=`0x3c2530` 的 RELATIVE **只有 1 个** = `0x9bac98`；`call *0x9bac98` **恰好 2 条**：`0x3c30c1`（在 `0x3c2bc0`）、`0x3e2d35`（在 `0x3e2830`） | 一致 |
| 正对照：`0x3c2bc0` → `0x9bacb0`、`generate_level 0x4661a0` → `0x957fc8`、`0x47b4b0`（32 B thunk）→ `0x957fc0` | 三个槽 addend 逐位相同；`0x47b4b0` 体 = 32 B 且第 5 条就是 `call 0x4661a0` | 一致 |

```bash
readelf -rW $X | grep 9bac98 ; readelf -rW $X | grep 9bacb0 ; readelf -rW $X | grep 957fc8
objdump -d --no-show-raw-insn --start-address=0x47b4b0 --stop-address=0x47b4d0 $X
# 17 个调用点：capstone 全 .text 扫 call rel32 == 0x467530
```

### 2.6 项 6 —— `fx-y-pre`：170 = 16 + 154 与 ulp 分布；arm64 `0x4a8558`

**重跑了整条链**（自己编译的两个 example + 401 份定格快照），并且**用自己写的分桶规则**从
`conv.jsonl` 复算，不调用 `tools/bit_exact_attrib.py`：

| 声称 | 实测（自建规则） | 判定 |
|---|---|---|
| `Y` 桶 170 批 | **170**（判据：`kind_seq_ok` ∧ 无 `corrected_function`/`end_x`/`pos_x` 差 ∧ `max|Δpos_y| ≤ 1e-9`） | 一致 |
| 16 批纯序列化格式 + 154 批真末位差 | **16 / 154**；16 批的 `ours` 与 `observed` 解析成 f64 后**逐位相等**（16/16） | 一致 |
| ulp 分布 1:117 / 2:23 / 3:6 / 4:2 / 8·33·59·64·803·1797 各 1 | **完全一致**：`{1:117, 2:23, 3:6, 4:2, 8:1, 33:1, 59:1, 64:1, 803:1, 1797:1}`；≤4 ulp = 148/154 | 一致 |
| 模式分布 DiffEq 114 / Normal 40（数值）、10 / 6（格式） | `{DiffEq:114, Normal:40}`、`{DiffEq:10, Normal:6}` | 一致 |
| 136/154 批的停点 = 我方点序列最后一点 | **136**（另 18 批 `blocked`） | 一致 |
| 格式批原文对 "13 种" | 13 种 /**16 批** ✓，但**文档清单只列了 12 种**（见 §1-C） | 不一致（清单） |
| arm64 `execute_results` = `0x4a8558` | `.symtab`：`GraphwarGameStateManager::execute_results17hfcf…` @ `0x4a8558`，**3712 B** | 一致（起点） |
| 体 = `0x4a8558–0x4a8e50` | `0x4a8e50` 是热路径 `ret`；函数真实终点 **`0x4a93d8`**（其后紧接 `get_alive_teams`） | **不一致（终点）** |
| `0x4a891c ldp d9,d8,[x8,#-0x10]` = `points[len-1]`；`pos_y` 实参 = 该点第二字，中间无求值器 | `0x4a88e8 orr x22,…`（拼 ptr）、`0x4a88ec ldr x20,[x8,#0x10]`（len）、`0x4a8918 add x8,x22,x20,lsl #4`、**`0x4a891c ldp d9,d8,[x8,#-0x10]`**、`0x4a89b4 stp d9,d8,[x29,#-0x60]`、`0x4a89b8 sturb wzr,[x29,#-0x4c]`（type=0）、`0x4a89e8 mov x8,#0x2c` / **`0x4a89ec movk x8,#0x8000,lsl#48`**、`0x4a8a08 bl 0x3d7a40`。`0x4a891c`→`0x4a89b4` 之间没有任何 f64 运算 | 一致 |

```bash
export CARGO_HOME=<ws>/.cargo-home CARGO_TARGET_DIR=<私有 target>
cargo run --release --offline --example replay_attrib -- --dir <401 份定格快照> --drive conv --policy strict --json conv.jsonl
cargo run --release --offline --example posy_probe  -- --dir <401 份定格快照> --json posy.jsonl
python3 tools/posy_attrib.py --conv conv.jsonl --probe posy.jsonl      # 与被审对象同口径
readelf -sW $A | grep execute_results                                  # 3712 B @ 0x4a8558
llvm-objdump -d --start-address=0x4a8910 --stop-address=0x4a8920 $A     # ldp d9,d8,[x8,#-0x10]
```

> 快照与两份 `jsonl` 都是**可重建**的中间产物（`.agent-work/` 会回收）：401 份定格快照的
> 生成方式见 `docs/16-reproduce.md`；本轮 401 份跑完 1,080 个结算批、511 批失败，
> 与 §24/§25 表里的分母逐个相同。

### 2.7 项 7 —— `0x418820` 谓词 = `i <= n`（`setbe`）

| 声称 | 实测 | 判定 |
|---|---|---|
| `cmp ebp,[rsp+0x34]` + **`setbe`**（无符号 `<=`） | `0x418a10 cmp ebp,dword ptr [rsp+0x34]`、`0x418a14 setbe sil` | 一致 |
| 循环上界钳在 20 | `0x41886c cmp ebp,0x14` + `0x41886f cmove r13d,ebp` | 一致 |
| 只有两个调用点；存档那处实参是 `n+1` | 调用点**恰好 2 条**：`0x4186d9`（在 `0x4183b0`）、`0x3a3eda`（在 `0x3a2e10`）；前者前一条是 `0x4186d2 inc %edx` → `0x4186d7 mov %edx,%esi` | 一致 |

```bash
objdump -d --no-show-raw-insn --start-address=0x418a00 --stop-address=0x418a20 $X
objdump -d --no-show-raw-insn --start-address=0x4186cd --stop-address=0x4186e0 $X
```

### 2.8 项 8 —— `docs/151` 悬案清单抽查 6 条（#14 / #21 / #22 / #24 / #27 / #28）

| 条 | 声称 | 实测 | 判定 |
|---|---|---|---|
| #24 | `0x554730` / `0x554750` **不是**函数起点（所属 FDE `0x5546c0..0x554762`）；`0x554770` 是起点，`..0x554826` | `0x554730`/`0x554750` 都不在 FDE 起点表，所属 FDE = `0x5546c0`，长 **0xa2**；`0x554770` 是起点，长 **0xb6** | 一致 |
| #27 | `0x51a750` = `0xcf0` B、5 条锚全指 `graphwar_function_evolutionator.rs`；`0x51a610` = `0xd6` B、1 条指 `local_player_manager.rs`；`0x3d76f0` = `0x254` B、零锚 | `0xcf0` / 5 票全票；`0xd6` / 1 票；`0x254` / **0** | 一致 |
| #28 | `0x3c7330` 只有 `0x2f`（47）字节、零锚；`0x3c7cb0` 是 `0x8d9` B、零锚 | **47 B** / 零锚；**0x8d9** / 零锚 | 一致 |
| #14 | `0x47a300` 零 `Location`，`0x212` B | **0x212 B** / 零锚 | 一致 |
| #21 / §2.4 | `0x9bb030` → `0x514b40`、`0x9ba978` → `0x8ab200`、`0x9bac88` → `0x3d38f0` | 三个 addend 逐位相同 | 一致 |
| #22 | `0x9baa10` = `bcmp@GLIBC_2.2.5`（`R_X86_64_GLOB_DAT`） | `readelf -r`：`00000000009baa10 0000000500000006 R_X86_64_GLOB_DAT bcmp@GLIBC_2.2.5 + 0`（`sym 5` = `bcmp`） | 一致（**但 §2.4 的旁证句错，见 §1-D**） |

```bash
readelf -rW $X | grep -E '9baa10|9bb030|9ba978|9bac88'
readelf --dyn-syms -W $X | sed -n '5,8p'
.venv/bin/python -c "..."   # 见 §3：FDE 起点表 + Location 记录 → 逐函数投票
```

## 3 复算方法（可重跑）

1. **ELF**：自写 `struct` 解析（无第三方），`readelf -SW` 只用来对照。
2. **指令**：capstone 5.0.7 线性扫 `.text`（`0x37b940+0x5d3244`）与 `.plt`，
   记 `call/jmp rel32`、`call/jmp [rip+X]`、`lea/mov rip`、`movabs imm64`。
   ⚠️ **`call *槽` 不属于 `lea` 形态**——`0x9bac98` 的两条使用者在 capstone 里是
   `call qword ptr [rip+…]`，只扫 `lea` 会漏（本轮第一次就踩了）。
3. **函数边界**：解 `.eh_frame_hdr`（版本 1、`eh_frame_ptr` 编码 `0x1b`、计数 `0x03`、表项 **`0x3b` datarel**），
   得到 13,430 个 FDE；再用 `.eh_frame` 的 `initial_location`/`address_range` 得长度。
   `funcstarts_ghidra_va.pkl`（13,263）当第二裁判；两者在本次所有地址上一致。
   **不要**把 `0x3b` 当 `pcrel` 解（`funcstarts.pkl` 的毛病）。
4. **`panic::Location`**：`.rodata` 里 509 个 `src/*.rs` 字符串 → `R_X86_64_RELATIVE` addend
   指向它们的槽 = Location 记录基址（`{ptr,len,line,col}`）→ 找 `rip` 引用者 → 按函数区间投票。
   本轮用它复现了 `docs/08` §5.6 的 9 个锚（`45:108…105:71`，全在 `0x467530` 的 `+0x1138` 冷块）
   与 `docs/151` 的模块归属投票。

## 4 本轮就地改到的文档

| 文档 | 改法 |
|---|---|
| `docs/10-campaign.md` §13.3 | `setb` → **`setg`**，并注明"审计发现原值 `setb`，实测 `setg`（结论不变）" |
| `docs/06-verification.md` §25.4 | `0x4a8558–0x4a8e50` → 补 `0x4a93d8`（`.symtab` 3712 B；`0x4a8e50` 只是热路径 `ret`） |
| `docs/06-verification.md` §25.2 | 13 种原文对的清单补上漏掉的 `7.606517635480697e-19` 那一条 |
| `docs/151-addr-open-items.md` §2.4 | "使用者全在 `0x3axxxx`" → 改成实测分布（267 条 `call *` / 56 条 `mov`，153 个函数，遍布 `.text`） |

## 5 没做的 / 无法判定

| 项 | 缺什么 |
|---|---|
| `weak_caller` 的 26 个 `T` **名字** | x86 被 strip，名字要逐调用点把 `f` 的符号追出来（arm64 侧只能给"哪个方法"，不给 `T`） |
| 项 4 里 `ecx` / `r9d` 的**语义标签**（精英数 vs limit） | 需要读进 `0x478fc0` 与进化器的字段用途；本轮只确认了立即数与寄存器位置 |
| 容器"48 字节"的**尺寸常量** | `Rc<RefCell<Container>>` 的 alloc/dealloc 站点没定位；本轮是偏移式推断 |
| `0x467530` 的**行为语义**（通视 vs 连通） | 本轮只复算身份与可达性，没读完全部 1,229 条指令（`docs/08` §5.6 自己也是这么标的） |
