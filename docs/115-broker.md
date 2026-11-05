# 115 · Broker（房间列表服务器）：协议 + 我们自己的实现

> 目标：搞清楚原版客户端 **怎么连 broker、发什么、收什么**，然后自己实现一台能顶替它的
> broker，并且**全程只在本机**（`127.0.0.1` + 一次性的网络命名空间）。
>
> 结论一句话：**帧格式与对局连接完全相同（`u16` 大端长度 + JSON）；一次会话是
> 「客户端发 `VersionInfo` → broker 逐条推 `GameInfo`」；房间表项就是那 9 个字段的
> `GameInfo` 结构，键是它的 `address`。**
>
> 我们的实现：[`src/broker.rs`](../src/broker.rs)（+ [`tests/broker.rs`](../tests/broker.rs)），
> `cargo test` **217 条全绿**（205 → 217）。

## 0. 证据等级约定

| 档 | 含义 |
|---|---|
| **逐位验证** | `cargo test` 里有对拍：抓包原文 → 解析 → 重新序列化，字节相等 |
| **观测** | 能指到具体地址/字节/运行输出（objdump、夹具、netns 实测） |
| **推断** | 有间接证据但没直接观测；**下面每条都写明缺什么** |
| **未知** | 没有任何证据 |

## 1. 传输层：和客户端↔服务器**同一套**帧

**逐位 + 观测**：

* 对局连接是 `u16` **大端**长度 + JSON 负载（`docs/24` §1，实测帧头 `0x0031` = 49 字节负载）。
* broker 连接用的**是同一个写入函数** `0x3d89e0`：`graphwar_server_broker_handler` 的
  连接函数 `0x437ce0` 在 `0x437ef8` 直接 `call 0x3d89e0`。
* `0x3d89e0` 本体里能看到这两条决定性指令：
  * `0x3d8ab0: cmp $0xffff,%rbx` —— 长度必须塞得进 `u16`（否则走 `Message too large`）；
  * `0x3d8b7b: rol $0x8,%cx` —— 16 位长度做字节翻转，即**大端**。
* 同一函数的错误串：`Failed to serialize event.` / `Failed to write to tcp_stream. Message too long.`

⇒ broker 会话 = `[u16 BE len][JSON payload] [u16 BE len][JSON payload] …`，**没有换行、没有魔数**。

## 2. 消息：全部是单键对象 `{"TypeName":{字段}}`

对局协议里已经确立的形状（`docs/24` §2、`docs/07`）在这里同样成立，且 broker 段的
三个消息**都能在序列化代码里逐字节读出来**：

### 2.1 `VersionInfo` —— 会话第一帧（客户端 → broker）

```
{"VersionInfo":{"version":"2.0.60","system":"linux"}}
```

* **逐位验证**：真实报文（`fixtures/protocol_match.jsonl` 第 83 行）。
* **观测（静态）**：`0x437ce0` 在连上 broker 之后构造事件并调用发送函数，事件里那条
  5 字节字符串是**在栈上直接写出来的** `"linux"`（`0x437e97`：`movl $0x756e696c` = `linu`，
  `0x437ea6`：`movb $0x78` = `x`）。`system` 取的是编译期平台常量，`version` 是 `2.0` + `60`
  拼出来的（二进制里没有 `"2.0.60"` 这个字面量）。

### 2.2 `GameInfo` —— broker 推给客户端的房间行

```
{"GameInfo":{"game_info":{"room_name":"US Room 2","game_state":"ExecutingResults",
 "current_player_count":2,"address":"69.164.213.144:35981","axis_mode":"EveryUnit",
 "function_mode":"NormalFunction","turn_mode":"SequentialTurns","time_mode":"Timer1m",
 "locked":false}}}
```

* **逐位验证**：`fixtures/protocol_match.jsonl` 第 84–86 行是**三条真实报文**；
  `tests/broker.rs` 与 `src/broker.rs` 的单测把它们解析后重新序列化，断言**字节相等**。
* **观测（静态）**：外壳由序列化函数 `0x50ff30` 逐字节写出——
  `'{'`（`movb $0x7b`）→ `"GameInfo"`（`0x19f398`，长度 8）→ `':'` → `'{'` →
  `"game_info"`（`0x1c5410`，长度 9）→ 内层结构体 → `'}'` → `'}'`。
* **观测（静态）**：内层结构体的序列化是 `0x5060d0`，9 个键名按
  `room_name, game_state, current_player_count, address, axis_mode, function_mode,
  turn_mode, time_mode, locked` 的顺序写（键名地址 `0x1c5875`/`0x1c587e`/…），最后补 `'}'`。
  它唯一的**直接**调用者是 LAN 广播 `0x457790`；事件路径经 trait 间接调用所以看不见直接调用边
  —— 这解释了"为什么只有一个直接调用者，但事件里确实带着同一个对象"。
* `tests/broker.rs` 里还有一条**运行期**证据（见 §5.3）：原版客户端接受一个**裸的** 9 字段
  `GameInfo` 对象并把房间行画了出来，说明这 9 个字段的**取值写法**（枚举是字符串、
  `locked` 是布尔、`current_player_count` 是整数）是对的。

### 2.3 `GameRemoved` —— 房间消失

```
{"GameRemoved":{"game_address":"1.2.3.4:35981"}}
```

* **推断**（**从未抓到过真实报文**）。依据：序列化函数 `0x511de0` 写出
  `'{' "GameRemoved"(10 字节, 0x1c5703) ':' '{' "game_address"(12 字节, 0x1c585c) ':' 值 '}' '}'`。
  字段数 N=1 与 `.rodata` 里 `struct GameRemovedEvent with 1 element` 一致（`docs/96` §5.1）。
* **缺什么**：没有一次真实流量证明 broker 真的会在房间消失时发这条、也没有证明字段类型是字符串。

### 2.4 `Subscribe` —— 从未在 broker 段出现过

```
{"Subscribe":{"subscriber_id":<整数>,"subscribed_event_type":"<事件名>"}}
```

* **观测（静态）**：序列化函数 `0x5135d0` 写出上述形状（键名 `0x1c57d4`/`0x1c57e1`），N=2。
* **未知**：谁发、发给谁。抓到的 broker 会话里**没有**它，所以我们的 broker 只在收到它时
  重发一次房间表（把它当"刷新"），不依赖它。

## 3. broker 手里的"房间表"是什么结构

| 结论 | 等级 |
|---|---|
| 表项 = `GameInfo` 结构（9 字段） | **逐位验证**（真实报文 + 序列化代码） |
| 表项的**键** = `GameInfo.address`（`"host:port"` 字符串） | **推断**——依据是 `GameRemoved` 只带一个 `game_address`，且它的取值集合必然来自 `GameInfo.address`；没有抓到"更新同一房间"的流量来直接证明 |
| 同一 `address` 再次出现 = **更新**（人数/状态/锁变化），不是新行 | **推断**——真实抓包里 3 条 `GameInfo` 的 `address` 各不相同，没抓到过重复地址 |
| `game_address` 与 `address` 是**同一个值** | **推断**（字段名不同、语义相同）；`GameRemoved` 没有别的字段可以表示"哪个房间" |
| 房间列表的**顺序** | **未知**（我们的实现按地址排序，只为可复现，不作为复原结论） |

`GameInfo` 的 9 个字段（键序即序列化顺序）：

| 字段 | 类型 | 实测取值 |
|---|---|---|
| `room_name` | 字符串 | `"Graphwar Game"`、`"US Room 1/2/3"` |
| `game_state` | 字符串（枚举） | `"Setup"`、`"ExecutingResults"`（`docs/96` 另有 `WaitingForFunctions`/`EndingGame`） |
| `current_player_count` | 整数 | `0`、`2` |
| `address` | 字符串 | `"69.164.213.144:35981"`、`"[::1]:35245"` |
| `axis_mode` | 字符串（枚举） | `"EveryUnit"` |
| `function_mode` | 字符串（枚举） | `"NormalFunction"` |
| `turn_mode` | 字符串（枚举） | `"SequentialTurns"` |
| `time_mode` | 字符串（枚举） | `"Timer1m"`（**不是** `Timers1m`，`docs/24` §1.1 已更正） |
| `locked` | 布尔 | `false` |

## 4. 房主怎么注册：长连接，不是心跳式上报

**推断（较强）**，依据都是静态的：

1. broker 侧有**专门的线程**：`graphwar_server_broker_handler.rs`
   （`0x4588d0` 里引用了该文件的 `:94` 日志：`failed to spawn thread`、
   `Failed to join connection thread.`）。
2. 这个线程**建连**（`0x437ce0`：解析地址 → `connect` → 发首帧；失败记
   `Failed to connect to broker at {}`，位置 `0x437d7d` 引用 `0x1a96a2`）。
3. 它**还读**这条连接：`0x4588d0` 调用 `0x3e5a80`，而 `0x3e5a80`
   （`src/server_broker_request_connection.rs:100`）是一个
   `Failed to read tcp_stream.` + `read_frame`（`0x3d86a0`）的循环。
   ⇒ broker **也会往房主那侧推事件**（推什么**未知**）。
4. 地址只有两个来源（`0x4581d0`）：命令行的 `--broker_address`，否则
   `GET https://graphwar.com/graphwar_II_ip.txt`（失败记 `:83`
   `Failed to get global server address.`，解析失败记 `:78`）。

**"房间在 = 连接在"** 是我们的实现口径；真实超时值**未知**
（`GameHeartbeat` 事件存在但**从未抓到过**，`docs/96` §3 也把它列为"字段数都未知"）。

## 5. 本地实验：做了什么、结果如何、边界怎么守的

### 5.1 边界（先说结论）

* 全程 **只在本机**：所有客户端运行都在
  `unshare -rn`（只有 `lo` 的**新网络命名空间**）里，broker 地址写死 `127.0.0.1`。
  命名空间里没有默认路由，**任何**外发连接都不可能成功。
* 一次都没有连 `graphwar.com`。任务允许的那**一次** `curl https://graphwar.com/graphwar_II_ip.txt`
  **没有使用**——协议已经从仓库里已有的真实抓包定死了，不需要它。
* 没有任何规避检测/伪装/限流绕过的实现。

### 5.2 想让原版客户端连我们的 broker —— **没做成**，原因是硬的

跑了 6 组受控实验（`--broker_address` 的各种写法），**客户端一次都没连到我们的 listener**：

| # | 传参 | 结果 |
|---|---|---|
| 1 | `--broker_address 127.0.0.1:42970` | 无连接；日志 `src/graphwar_server_broker_handler.rs:83 - Failed to get global server address.` |
| 2 | `--broker_address zzz`（故意非法） | **没有任何 clap 报错** ⇒ 参数根本没被解析 |
| 3 | `--broker-address 127.0.0.1:42970`（kebab） | 无连接（Godot 把未知参数拼成**一个**字符串） |
| 4 | `--broker-address=127.0.0.1:42970`（单 token） | 无连接 |
| 5 | 上面加 `--broadcast-port/--logger-port/--server-port` | **端口不变**（仍是 UDP 42968 / TCP 46868）⇒ CLI 完全无效 |
| 6 | **干净 argv**（只带游戏参数、`cwd`=工程目录、Xvfb 下窗口模式跑） | 仍走 `:83` 分支 |

结论（**推断，但证据收敛**）：这个构建里 **命令行参数是死的**。Rust 的
`std::env::args()` 从 `lang_start` 填充的静态量取 argv，而 GDExtension 是被 Godot
`dlopen` 的 cdylib，`lang_start` 从不运行 ⇒ `args()` 为空；二进制里也没有任何
`OS.get_cmdline_args` / `get_cmdline_user_args` 的方法名串（`grep cmdline` = 0 命中）。

于是**唯一**实际生效的地址来源是那次 HTTPS 请求。要把它改到本机，等价于要一张
`graphwar.com` 的证书：二进制的 TLS 根证书是**编进 rodata 的**（有 `rustls-webpki`、
`UnknownIssuer` 等串；**没有** `rustls-native-certs` / `platform-verifier` /
`SSL_CERT_FILE`），所以注入本地 CA 不可行。**这就是我们停下的原因**——再往前要么连
graphwar.com（越界），要么改二进制绕过证书校验（不做）。

### 5.3 做成了什么：用原版客户端验证了**同一个房间表项**的线格式

broker 走不通，但"房间列表"这套东西还有第二个入口：**LAN 发现**
（`docs/24` §1.1，UDP、明文 JSON、无长度前缀）。在同一个隔离命名空间里：

1. `ss -ltnup` 实测客户端在 LanLobby 绑 **UDP `0.0.0.0:42968`**（和 `127.0.0.0/8` 里的
   房间服务器 TCP 46868 不是一回事）。
2. 往 `127.0.0.1:42968` / `[::1]:42968` / `ff02::1%lo:42968` 灌候选报文：
   5 个变体各带一个独特 `room_name`，看哪一个出现在 UI 里。
3. 结果：**只有变体 C（裸的 9 字段 `GameInfo` + 结尾换行 `"\n"`）被接受**。
   `GraphwarLanVBox` 里出现一行 `game_room_entry.tscn`：
   `RoomName = "VAR-C-nl"`、`PlayerCount = "2"`。
   不带换行的裸对象（A）、事件包裹形式（B）、`{"game_info":…}`（D）、
   `"Timers1m"`（E）**全部被丢弃**。

⇒ 两条**运行期**结论：

* 房间表项的 9 个字段、键名与取值写法（枚举当字符串、`locked` 布尔、人数整数）是**对的**；
* LAN 通道的分帧是**换行**，而 broker 通道的分帧是 **`u16` 大端长度**（§1）——两条通道
  **不是**同一个分帧，别混用。

### 5.4 仓库里本来就有一次真实的 broker 会话

`fixtures/protocol_match.jsonl` 第 83–86 行就是抓到的 broker 段（浏览方视角），
本次静态还原的三条形状（`VersionInfo` / `GameInfo` 外壳 / `GameInfo` 内层 9 字段）
**与它逐字节一致**。这构成了本文件最硬的一条对拍：不是"我推断它应该长这样"，
而是"代码读出来的形状 = 真实流量的字节"。

## 6. 我们实现的 broker

### 6.1 为什么放在 `src/broker.rs`（而不是 `tools/broker.rs`）

因为它要**进 `cargo test`**：复用 `src/protocol.rs` 里已被抓包钉死的
`Frame`（u16 大端帧）/ `Broadcast`（9 字段房间行）/ `VersionInfo`，并且能用两条普通
TCP 连接做端到端断言。放 `tools/` 只能靠人工跑；放 `src/` 才能被验收入口覆盖。
（`tools/` 在本仓库是"分析脚本"，不是产品代码。）

### 6.2 行为表

| 触发 | broker 的行为 | 依据 |
|---|---|---|
| 新连接 | 记下写端，等第一帧 | — |
| 收到**首帧**（真实会话是 `VersionInfo`） | 打日志，然后把当前房间表**逐条**用 `GameInfo` 推回去 | **逐位**（真实会话：`VersionInfo` 之后紧跟 3 条 `GameInfo`） |
| 收到 `GameInfo` | 以 `address` 为键 upsert；广播给**其它**连接 | **推断**（房主方向未抓到；没有别的通道能到达 broker） |
| 收到 `GameRemoved` | 删键；广播给其它连接 | **推断**（消息本身从未抓到） |
| 收到 `GameHeartbeat` | 只刷新该连接名下房间的时间戳 | **推断**（`GameHeartbeat` 字段数都未知，不解析字段） |
| 收到 `Subscribe` | 把当前表再推一次 | **未知方向**，只做兼容 |
| 其它消息 | 忽略 | — |
| 连接断开 | 删掉该连接登记的所有房间，并广播 `GameRemoved` | **推断**（长连接注册） |
| 超过 `room_ttl`（默认 120 s）没更新 | 删房并广播 `GameRemoved` | **推断**（真实 TTL 未知） |

### 6.3 用法

```bash
# 起一台（默认 0.0.0.0:42970，帮助文本里的示例端口）
cargo run --offline --example serve_broker -- 127.0.0.1:42970
```

### 6.4 测试（12 条，全部不需要真游戏）

`src/broker.rs` 内 8 条 + `tests/broker.rs` 4 条：

* **逐位**：真实抓包的 4 条 broker 报文 → 解析 → 重新序列化 → 字节相等；
  `VersionInfo` 的 `2.0.60/linux` 字段；`GameInfo` 外壳字面量；
* 帧：`u16` 大端前缀、半包不整帧、`Message too large` 的边界；
* 房间表：upsert 覆盖、`remove_owner`、`expire`、快照排序；
* **端到端（真 TCP）**：房主连上登记 → 浏览客连上就收到这条房间；更新人数/状态后
  浏览客收到新记录；`GameRemoved` 到达；**房主掉线后浏览客收到 `GameRemoved`**。

## 7. 与原版的差异（必须写清）

| 项 | 原版 | 我们的 |
|---|---|---|
| 地址来源 | 命令行 `--broker_address`，否则 HTTPS 取 `graphwar_II_ip.txt` | 只看命令行参数（`serve_broker`），没有任何拉取逻辑 |
| 房主登记 | **未观测** | 房主在同一条连接上发 `GameInfo`（推断） |
| 心跳/超时 | **未知**（`GameHeartbeat` 从未抓到） | 连接断开即删房 + 120 s 空闲兜底（推断值） |
| 房间顺序 | **未知** | 按 `address` 排序（只求可复现） |
| 重复连接 | **未知** | 不限制（相同 `address` 后到者覆盖，且归属换到新连接） |
| 鉴权/版本校验 | **未知**（`VersionInfo` 收到了，但没证据表明它会拒绝旧版本） | 只打日志，不拒绝 |
| `Subscribe` | **未知** | 收到就重发快照 |
| 错误处理 | `Failed to connect to broker at {addr}` 等 | 静默丢弃坏帧/坏连接，日志走 stderr |

## 8. 接入官方 global room 的风险与限制（只陈述事实）

1. **协议层面**：帧格式（`u16` 大端 + JSON）与 `VersionInfo`/`GameInfo`/`GameRemoved`
   的形状我们有**逐位**证据，理论上能收发兼容报文；**但**"官方 broker 需要哪些消息、
   什么顺序、什么频率、有没有鉴权"我们**只观测到一条浏览方会话**（版本握手 + 三条房间），
   其余**未知**。⇒ 能不能被官方 broker 接受，**没有证据**。
2. **地址层面**：客户端取 broker 地址的**唯一**实际生效路径是
   `https://graphwar.com/graphwar_II_ip.txt`（另一个 `--broker_address` 在这个构建里
   不生效，见 §5.2）。要让原版客户端连到非官方 broker，就得控制该 URL 的响应或该域名
   —— 这需要官方基础设施的控制权，不是本地配置项。二进制里**没有**任何"改用本地 broker"
   的官方开关（除那个不生效的参数）。
3. **TLS**：该请求是 HTTPS，根证书编进二进制（§5.2），因此**不可能**通过本地 CA
   让客户端把该请求解析到本机；要做到只能改二进制绕过证书校验。
4. **服务条款/账号**：游戏**没有账号系统**（抓包与代码里都只有 `connection_id`/
   `player_id` 这类每次连接现分配的编号），所以"用自己账号"这个问题不存在；
   但**我们从未读过 EULA/服务条款原文**，因此"把官方 global room 当我们的房间列表
   后端"或"让官方客户端连非官方 broker"是否被允许——**未知，不做断言**。
5. **我们没做、也不做**的事：伪造官方身份、绕过证书校验、绕过限流、伪装客户端指纹、
   把官方 broker 的数据当自己的后端。本文件里的所有实验都在隔离网络命名空间内完成。
6. **一条已知事实**：抓包里出现过的 `69.164.213.144` 是**房间（房主）地址**，
   不是 broker 的地址；broker 的实际端口**未观测**（帮助文本的示例值是 `42970`）。

## 9. 仍未确定

| 问题 | 状态 | 缺什么 |
|---|---|---|
| 客户端（浏览方）侧的 broker 连接代码在哪 | **未知** | 抓到的那次会话是浏览方发出的 `VersionInfo`，而发 `VersionInfo` 的函数 `0x437ce0` 在 `graphwar_server_broker_handler.rs` 里、且**没有直接调用者**（线程/函数指针入口）⇒ 我们**推断**两端共用这段连接代码，但没能定位浏览方的调用点 |
| 房主如何登记/刷新房间 | **未知** | 需要一次房主视角的真实抓包 |
| broker 会不会回推事件给房主（`0x3e5a80` 在读） | **推断会读**，内容**未知** | 同上 |
| 房间的存活/超时策略 | **未知** | `GameHeartbeat` 从未抓到 |
| `GameRemoved` 真实报文 | **推断**（只有序列化代码） | 一次房间消失的抓包 |
| 房间列表顺序、分页、上限 | **未知** | 房间数 >3 的抓包 |
| 官方 broker 端口 | **未知** | 只能从 `graphwar_II_ip.txt` 得知，未读取 |

## 10. 复现

```bash
# 1) 测试（217 条，含 broker 的 12 条）
export CARGO_HOME=<ws>/.cargo-home CARGO_TARGET_DIR=<ws>/.cargo-home/target
cargo test --offline

# 2) 起一台 broker 并手工试
cargo run --offline --example serve_broker -- 127.0.0.1:42970

# 3) 隔离环境下的原版客户端实验（只在本机；脚本在 .agent-work/broker/，未入库）
#    unshare -rn 里：ip link set lo up → 起 listener → 跑 godot(harness) --broker-address=...
#    见 §5.2 的 6 组参数；LAN 通道实验见 §5.3。
```
