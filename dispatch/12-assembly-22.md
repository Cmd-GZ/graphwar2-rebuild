# 12 · `agent/assembly-22`：把装配从 3 个 manager 扩到 22 个

**优先级 P1。通向"能跑的原版形状"。** 现状：只有 3 个 manager 进容器，节点树下 25 个空 `Node` 仍是占位。

## 已知（`docs/02` §22.4，指令级）

```text
GraphwarGameClient::new    → 22 条 add_module
                             x86 0x517690 / arm64 0x409c70，调用序逐位相同
容器 = GraphwarModuleContainer { modules: HashMap<TypeId, Rc<dyn Any>> }  // 1 字段 / 48 字节
```

⚠️ **只观测到"调用序相同"，没有逐型证据**——哪个 `add_module::<T>()` 对应哪个类型，
需要你补证据（arm64 的单态化符号 + 尺寸比闸 0.61–1.19），**能定性几个算几个，不许猜**。

## 要做什么

1. 把 22 条 `add_module` 的**顺序 + 类型**逐条对上（能全定最好，定不了就列出"未知"和缺什么证据）；
2. 在 `root_wiring.rs` 里按该顺序装配（**已经装配的 3 个不要动**）；
3. 每加一个 manager，在 headless 脚本里断言"容器能按类型查到它"；
4. 不要试图实现那 19 个 manager 的**行为**——只装配（构造 + 注册）。

## 允许改的文件

- `src/_scaffold/root_wiring.rs`、`src/_scaffold/graphwar_root_node_class.rs`
- `client/tools/assemble_check.gd`（扩展断言）
- `docs/16-client.md` 追加一节

⚠️ **不许改** `src/lib.rs`、`src/graphwar_*manager.rs` 的纯逻辑、别人的 `*_class.rs`、`client/src/**`。

## 验收

```text
容器里能查到的 manager 数：3 → ？（给出确切数与逐型清单）
每个 manager 的证据档（逐位/观测/推断）必须标出来
assemble_check.gd 断言全过
cargo test --offline 全绿（1512）；coverage --check OK
```

## 汇报（≤8 行）

分支+sha、**22 条里定死了几条**、装配进去几个、每个的证据档、
哪些只能标推断/未知（缺什么证据）、`cargo` 条数。
