# 105 · 队伍固定在 x 的两侧（`flip` 由队伍决定，不用几何猜）

状态：现行。用户口述 + replay 实证。

> **⚠️ 已证伪（T69 更正框，2026-11-27）。归档正文保留原样，删数只在下面这个框里说明。**
>
> 本节「**永远**在 `x<0`/`x>0`」「**违反 0 例外**」两句被冻结语料打破。用
> **403 个文件的冻结快照（401 份回放 + `MANIFEST.tsv` + `SHA256SUMS`）**复算：
>
> | 队伍 | `PosInfo` 样本 | `x<0` | `x==0` | x 范围 |
> |---|---:|---:|---:|---|
> | Team1 | 2681 | **2681** | 0 | [−23.989, −1.006] |
> | Team2 | 2643 | **6** | 0 | [−21.403, 23.984] |
>
> ⇒ Team1 那半句（全负）**成立且无例外**；Team2「**永远** x>0 / 0 例外」**不成立**：6 条负样本，
> 落在 **3** 份回放里 —— `1782674270`（x=−15.1006 / −6.1185）、`1782677092`（−21.4029 / −19.8887）、
> `1782812355`（−15.8127 / −12.1288）。
>
> **反例的机制（T69 追出，比"有例外"更有用）**：这 6 条都不是"出生就站错边"。以 `1782674270` 为例，
> 那两名士兵（id `181877`/`181878`）在**上一份回放 `1782673988` 里是 Team1**、终局 x 恰为
> `−15.1006`/`−6.1185`；下一局换边成 Team2 后，录制流开头把**上一局的坐标**当成初始 `PosInfo` 又发了一遍。
> ⇒ 规则应写成「**常规开局后** Team1 在 x<0、Team2 在 x>0；换边局的初始快照可能残留上一局坐标」。
> `docs/guide/01` 已按"常规开局后"表述（T61），与本框一致；**`118` §7.1 那类"永远"写法不要再用**。
>
> 复跑（本仓库根目录；口径写死在脚本头）：
> ```bash
> python3 - <<'PY'
> import glob,gzip,json,os,collections
> SNAP='.reference/replays/gw2-replays-snapshot/replays'
> files=sorted(p for p in glob.glob(SNAP+'/*') if os.path.basename(p) not in ('MANIFEST.tsv','SHA256SUMS'))
> pos=collections.Counter(); neg=collections.Counter(); ex=[]
> for p in files:
>     s2p={}; t2t={}; rows=[]
>     for ln in gzip.open(p,'rt',encoding='utf-8',errors='replace').read().splitlines():
>         try: o=json.loads(ln)
>         except Exception: continue
>         e=o.get('event') or {}
>         for nm,b in e.items():
>             if not isinstance(b,dict): continue
>             if nm=='TeamInfo': t2t[b['entity_id']]=b['team']
>             elif nm=='SoldierInfo': s2p[b['soldier_id']]=b['player_id']
>             elif nm=='PosInfo': rows.append((b['entity_id'],b['pos_x']))
>     for ent,x in rows:
>         t=t2t.get(s2p.get(ent,ent))
>         if t is None: continue
>         pos[t]+=1
>         if x<0:
>             neg[t]+=1
>             if t=='Team2': ex.append((os.path.basename(p),ent,round(x,4)))
> print(dict(pos), dict(neg)); print(ex)
> # {'Team1': 2681, 'Team2': 2643} {'Team1': 2681, 'Team2': 6}
> # 6 条反例 = 3 份回放各 2 条（见上表）
> PY
> ```
>
> **原文里那组数字（120 局 / 773 个 `PosInfo` / Team1 394 / Team2 379）是更早的语料读数**，
> 本轮无法从现存快照复现（现快照 `PosInfo` 事件 5324 条）；归档作为历史记录保留，不再当"现行结论"引用。

## 1. 规则

**我方（Team1）永远在 `x < 0` 侧，敌方（Team2）永远在 `x > 0` 侧。**
由此开火方向也固定：

| 队伍 | 所在半场 | 开火方向 | `flip` |
|---|---|---|---|
| Team1 | `x ≤ 0` | 向右（+x） | `false` |
| Team2 | `x ≥ 0` | 向左（−x） | `true` |

## 2. 证据（replay 实测）

**士兵位置**（120 局、773 个 `PosInfo` 样本）：

| 队伍 | 数量 | `x < 0` 的个数 | x 范围 |
|---|---|---|---|
| Team1 | 394 | **394** | [−23.95, 0.00] |
| Team2 | 379 | **0** | [0.00, 23.98] |

**违反 0 例外。**

**开火方向**（同批 replay 的 `FunctionPoints`）：

| 队伍 | 向右 | 向左 |
|---|---|---|
| Team1 | **108** | 0 |
| Team2 | 3 | **97** |

Team2 那 3 个"向右"是**弯曲弹道**导致 `end_x < start_x` 的假象，不是真的向右开火。

## 3. 对实现的影响

- **`flip` 必须由队伍决定**，不能用"起点 x 与终点 x 比大小"去猜。
  `tools/replay_state.py` 已改成按队伍取，仅在队伍未知时回退到几何判断。
- `docs/98` 的回合系统按玩家轮转，所以"这回合是哪个队伍"直接给出 `flip`；
  AI 不需要从坐标推断方向。
- 关卡生成里士兵的落位也应遵守这条边界（`x<0` / `x>0` 各一半）。

## 4. 尚未验证

- Team3/Team4（如果存在）落在哪个半场——**未观测到四人以上队伍**；
- 单队 0 人 / 全灭时的边界行为未单独验。
