# 90 · `agent/lan-lobby-flake`：修**第二条** selftest flake（LAN 大厅成对红）

**优先级 P1 —— 护栏最后一块洞。修好之后"红 = 真红"这个前提才成立。**

## 0. 现状（84 号 A/B 证明是**既有** flake，不是本轮引入）

```
现象：连跑 selftest 时，LAN 大厅相关用例**成对**变红（两个一起红）
归因（84 号的 A/B）：**selftest 自己改了窗口尺寸** + `queue_free` 的延迟释放
      ⇒ 上一个用例的节点还在，下一个用例就绪判断被污染
另有一条我这边的观测（`docs/18` §10.3.3）：在"release 构建 + --import + 连跑 14 个 headless"
      之后，头两次 selftest 会掉到 **631 / 7 failed**，第三次回到 716/0
      ⇒ 与这条很可能是同一个根因（**用例间共享状态没清干净**）
```

## 1. 要做什么

1. **先做出确定性复现**：写一个循环跑 `bash client/run.sh selftest` N=10，
   记录每次的 `passed/failed`、失败项名字、以及**它们是否成对**；
2. **定位到具体共享状态**（候选，逐个排除并给证据）：
   ```
   · 窗口尺寸被上一个用例改过（`DisplayServer.window_set_size` / `get_window().size`）
   · `queue_free` 延迟：节点在下一用例开始时仍存活（用 `is_instance_valid` 打点）
   · `user://` 文件（`local_ip.config` / `serve_match_logs/`）跨用例残留
   · 端口/连接未释放（上一条用例的 client 还连着）
   ```
3. **修成确定性的**：优先"显式 teardown + 每用例唯一前缀 + 等就绪信号"，**不要加大 sleep**；
4. **验收**：连续跑 **10 次全绿**（贴 10 行）；且在"先连跑 14 个 headless 再跑 selftest"这个
   **我这边复现过的场景**下，**第一次就必须全绿**（这是本轮的关键判据）。

## 2. 硬约束

- **不许把失败改成跳过**；
- 不改断言判据；`cargo test --offline` **1753**；三轴 `647 / 647 / 637`；headless **1923**；
- headless 只跑 `extends SceneTree` 的脚本（拿 helper 喂 `--script` 会弹 ALERT + 留实例）；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/lan-lobby-flake`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 相关：`client/tools/selftest.gd`（LAN/大厅那几组）、`client/run.sh`、`examples/serve_match.rs`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/lan-lobby-flake`

## 4. 验收

```text
① 复现率（10 次里几次红、是否成对）
② 根因（共享状态是哪一项，给证据）
③ 修法（清什么、等什么、怎么隔离）
④ 连续 10 次全绿的输出
⑤ **"14 个 headless 之后第一次跑"也全绿**的证据（关键）
⑥ cargo/三轴/headless 是否退
```

## 5. 汇报（≤8 行）

分支+sha、复现率、根因、修法、10 次连跑结果、headless 后首跑结果、三轴是否退。
