# 69 · `agent/lan-ai-match`：造 **LAN + AI 局** —— 1 ulp 唯一还开着的入口

**优先级 P0。66 号把别的路都堵死了，只剩这条（它自己写的"下一步唯一活口"）。**

## 0. 为什么必须是"LAN + AI"

```
语料里那些"分叉批"（`pos_x ≠ end_x`、末位差）**全部来自 AI-vs-AI 局**
66 号三样同抓失败：EffectInfo 的载荷就是 pts[n-1] 的 16 字节拷贝
   ⇒"常数末位差"与"数组被改过"**双双被否**，60 号那个"常数 +1 ulp"旋钮**被推翻**
⇒ 线索断在"**AI 开的那一发**"上：AI 走的是与人类不同的那条路径
⇒ 要复现，就得让 **AI 在 LAN 局里开火**（人类在局域网里当对手，AI 补位/托管）
```

## 1. 要做什么

1. **把局面搭起来**：LAN 局 + 至少一个 AI（`serve_match` 或游戏内建 Bot 都行），
   让 AI 真的开火（不是人类代打）；确认 AI 开火那条路径与语料里那些批同源；
2. **在同一发上抓三样**（沿用 66 号的探针）：`0x3c2530` 入口 token 的 16 字节 /
   `0x472865` 的 `(rdx,rcx,n)` + 16 字节 / `EffectInfo` 的 JSON；
3. **顺带给 `0x3c5f70` 找调用者**（偏移函数走**指针派发**，`call rel32` 为 0）：
   扫 `.data.rel.ro` / GOT 槽，定位谁把 `&Point` 传进去 —— 那是 `x0` 的生产者；
4. 判据：
   ```
   · 若 AI 那一发的 token 常数与"回放文本 parse 出来的"**逐字节相同** ⇒ 1 ulp 在更下游
   · 若不同（多/少 1 ulp）⇒ 找到正主
   · 若抓不到 AI 开火 ⇒ 如实报"卡在哪"（这套局本身怎么搭不起来）
   ```

## 2. 纪律

- 样本入库（文本 JSON，去路径用户名）；
- 抓不到就说抓不到，**不许用静态推测代替运行时结论**；
- 提交信息用 `-F 文件`。

## 3. 环境

```
<WS>/.reference/game/  三件套（sha 9a538115…）
frida 17.26.1          <WS>/.venv
Xvfb（TCP 监听）        docs/06 §35.1 的三个坑
LAN                    `examples/serve_match`（已入库）；或游戏内 LAN 房间 + 补 AI
```

- 分支 `agent/lan-ai-match`
- **私有** `CARGO_TARGET_DIR`
- 现成：`tools/frida_point_array.*`（66 号已扩到抓 EffectInfo 载荷与 JSON）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/lan-ai-match`

## 4. 可以改的文件

- `tools/`、`examples/`（只加不改）、`fixtures/`（样本）
- `docs/06-verification.md` 追加一节（标 `观测`）
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① LAN+AI 局怎么搭的（步骤/命令），AI 开火抓到几发
② 三样是否同一次、常数 token 与回放文本逐字节比较结果
③ `0x3c5f70` 的调用者找到了吗（地址 / 或"未找到 + 怎么扫的"）
④ 判决：1 ulp 的正主（token 上游 / 下游 / 仍未定位）
⑤ cargo test 1693；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、LAN+AI 怎么搭、抓到几发、常数 token 对比结论、`0x3c5f70` 调用者、`cargo` 结果。
