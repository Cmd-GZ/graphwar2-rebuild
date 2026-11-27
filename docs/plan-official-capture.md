# 任务书：**连官方服务端抓包**，把 8 类"零真实帧"的协议事件从 `推断` 升到 `观测`

> **用户已明确授权（2026-11-26）**：「允许连接官服抓包」。
> 这是此前唯一被判定「物理不可得」、必须由用户决定的事，现已解锁。
> ⚠️ 本文件是**仓库内的正式任务书**（不是 `.agent-work` 里的临时规格），因为 `AGENTS.md` 规定`.agent-work/` 用完即删、不得进证据链。

## 0. 为什么非抓不可

T46（`docs/06` §96）的结论：`protocol.rs` 那 60 条自洽断言里有 **9 条换不了真值** ——
23 个补齐事件中 14 种已从冻结语料拿到真实抓包，剩下这些**在 403 份冻结回放里一条真实帧都没有**：

```
LockInfo · ChatMessage · GameHeartbeat · GameRemoved · TurnLimitInfo · ClearBans · BanConnection
以及 EndingGame（403 份里出现 0 次）
```
⇒ 它们只能停在 `推断`。抓一次真实会话即可把它们升到 `观测`，并把对应的 9 条自洽断言换成真值。

### 0.1 已经查到的开局条件（2026-11-27，`strings` 实测）

**官方客户端整套都在本机**，不用另找：

```
.reference/game/Graphwar.pck          ← 官方资源包（不是 recovered-src，是原件）
.reference/game/Graphwar.x86_64       ← 官方 Godot 可执行
.reference/game/libgraphwarrust.so    ← 官方 Rust 内核（与 .reference/steam/2026-08-28 同源）
.reference/game/start_graphwar.sh     ← 启动脚本
```

**broker 地址的来源是显式的**（`.so` 的 `.rodata` 串）：

```
https://graphwar.com/graphwar_II_ip.txt        ← 默认从这儿取 broker 地址
"Failed to get global server address."
CLI 参数：--broker-address / --broadcast-port / --logger-port / --server-port / --crash-report-address
   （"Address of the broker server to connect (e.g. 127.0.0.1:42970). If not specified, the address is
     fetched from https://graphwar.com/graphwar_II_ip.txt"）
```
⇒ 第一步就是 `curl https://graphwar.com/graphwar_II_ip.txt` 拿到真 broker 地址，再确认 TCP 可达。

## 1. 前置核查（先做，做不到就如实报"网络不可达/无法复现会话"）

1. **官方客户端能不能在这台机器上跑起来并连上服务端**：
   - `.reference/steam/2026-08-28/libgraphwarrust.so`（真二进制）+ `.reference/steam/android-2.0.60/`（arm64 版）都在；
   - `recovered-src/project.godot` / `Graphwar.gdextension` 能描述包结构；
   - **需要确认**：官方 broker 的地址/端口从哪来（`GraphwarBackend::start_client_to_local_server`、`graphwar_broker` 相关模块、`.rodata` 里的域名/端口常量）。
2. **抓包手段**（按可用性排序）：
   ① **frida hook**（`.reference/venv` 有 frida 17.26.1）—— 最干净：直接 hook Rust 侧的 `TcpStream::write`/`read` 或协议 `serialize/deserialize` 入口，拿到**结构化字节**而不是 pcap；
   ② 若只能拿 pcap：本地代理/`tcpdump` + 已知的 framing（`LEN_PREFIX_LEN=2`、`MAX_PAYLOAD=0xffff`，`graphwar_stream_buffer` 已复原）解帧。
3. **合法性与影响面**：只做**只读观测**，不注入不伪造；只记录**协议结构**（事件类型/字段/判别值/枚举取值），**不保存**任何账号、聊天正文之外的个人信息（`ChatMessage` 只留字段形状与长度分布，正文打码）。

## 2. 要拿到的三张表

1. `fixtures/protocol_live_frames.jsonl` —— 每条：`方向 | 事件名 | tag | 原始字节 hex | 解析出的字段与取值 | 时间戳`。
2. `fixtures/protocol_enum_domains.tsv` —— 这 8 类事件里出现过的**枚举取值全集**（此前 7 个线上枚举已知，缺的补上）。
3. `fixtures/protocol_zero_frame_events.tsv` —— 逐类判定：`event | 抓到次数 | 字段顺序(与 WIRE_SPEC 逐项对比) | 与文档差异 | 升档后该改什么`。

## 3. 落码/落文档

- `src/graphwar_protocol.rs`（或对应原版模块）：按真实帧校正字段顺序/类型/判别值，**每处改动标 `观测` 并附帧的 sha256**；
- `tests/`：把 T46 留下的那 9 条"换不了"的自洽断言换成真值（期望值 = 抓到的原始字节）；
- `docs/07`（协议）+ `docs/06` 追加一节：**逐事件列出"原推断 → 现观测"的差异**；
- 若某事件**抓不到**（例如只在特定模式下出现），如实登记"需要 X 条件才能触发"，不许用推断填。

## 4. 硬门槛
```
cargo test --offline 1826/0/1ignored ｜ --features gdext-bindings 1956/0/1ignored
coverage_matrix --check OK（COVERAGE_MATRIX_UPSTREAM=<MAIN>/.reference/upstream）
check_doc_refs rc=0 ｜ 三轴 fire 647 / batch 647 / batch_conv 637
boundary_audit --check mismatch 0
```
**不许**把抓到的字节当游戏资产提交（`AGENTS.md` 规矩：二进制衍生内容 gitignore），只提交**解析后的文本**。

## 5. 交付报告
① 抓到几类/各几帧 ② 与文档/WIRE_SPEC 的逐项差异 ③ 升档了哪些、哪些仍抓不到（缺什么触发条件）
④ 9 条自洽断言换了几条 ⑤ 门槛读数 ⑥ 复跑命令 ⑦ 遗留 + §我写错了
