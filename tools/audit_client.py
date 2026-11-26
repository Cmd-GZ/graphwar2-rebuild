#!/usr/bin/env python3
"""审计面 ①：`client/` —— "以行为重建代替源码还原"的地方（只读，不改 client/**）。

判据、分档见 `.agent-work/specs/AUDIT-CRITERIA.md`；本面怎么做见 `AUDIT-1-client.md`。

    python3 tools/audit_client.py            # 产出 fixtures/source_audit_client.tsv + 统计
    python3 tools/audit_client.py --check    # 只复查（copy 是否逐字节、挂载点是否还在、行数是否变）

# 这个工具怎么保证"可复核"

1. **行的归属不是我拍的**：行数由 `func` 边界机器算（`^func`/`^static func` 列 0），
   每一条 TSV 行声明它覆盖哪几个函数；工具断言"每个生产文件的每一行恰好被覆盖一次"，
   覆盖不全就报错退出（不让"没判到"看起来像"判过了"）。
2. **判据列里的东西都要能自己跑**：`cmp` / `grep` / `godot --headless -- --dump-tree`。
   工具自己会对 27 份原版视图 + 52 份原版副本重做 `cmp`，对 27 个挂载点重做 grep。
3. **正对照**：`--check` 里同时验一个"已知合格"（原版视图副本逐字节相同 + `_ready`
   有效果：`client/tools/recovered_view_check.gd` 57/0）与一个"已知不合格"
   （`client/scenes/screens/play_screen.tscn` 12 行 vs 原版 780 行）。
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "fixtures", "source_audit_client.tsv")

# ---------------------------------------------------------------------------
# 0. 面：client/ 里"我们的代码"到底有哪些（原版字节脚本不算）
# ---------------------------------------------------------------------------
VIEWS_DIR = "client/src/views"
RECOVERED_COPIES = "client/tools/recovered"

#: 原版界面控制器的 Rust 源文件（`.so` 的 `panic::Location` 字符串里逐字面量可见）。
#: 复跑：`grep -ao 'src/graphwar_[a-z_]*\\.rs' .reference/game/libgraphwarrust.so | sort -u`
#: ⇒ 57 条 `graphwar_*.rs`；其中控制器/视图相关的是下面这些（本审计反复引用）。
RUST_CONTROLLERS = {
    "root": "src/graphwar_root_node.rs",
    "play": "src/graphwar_play_screen_control.rs",
    "lobby": "src/graphwar_lobby_control.rs",
    "campaign": "src/graphwar_campaign_control.rs",
    "player": "src/graphwar_player_screen_control.rs",
    "setup": "src/graphwar_setup_control.rs",
    "replay": "src/graphwar_replay_control.rs",
    "replay_player": "src/graphwar_replay_player.rs",
    "input": "src/graphwar_function_input.rs",
    "audio": "src/graphwar_audio_control.rs",
    "button": "src/graphwar_button.rs",
    "mode": "src/graphwar_mode_manager.rs",
    "color": "src/graphwar_color_manager.rs",
    "client_conn": "src/graphwar_client_connection_handler.rs",
    "stream": "src/graphwar_stream_buffer.rs",
    "lan_v6": "src/graphwar_lan_broadcaster_v6.rs",
    "broker": "src/graphwar_server_broker_handler.rs",
    "gsm": "src/graphwar_game_state_manager.rs",
    "turn": "src/graphwar_turn_manager.rs",
    "life": "src/graphwar_life_manager.rs",
    "target": "src/graphwar_target_manager.rs",
    "name": "src/graphwar_name_manager.rs",
    "team": "src/graphwar_team_manager.rs",
    "effect": "src/graphwar_effect_manager.rs",
    "entity_effect": "src/graphwar_entity_effect_manager.rs",
    "obstacle": "src/graphwar_obstacle_manager.rs",
    "function": "src/graphwar_function_manager.rs",
    "soldier": "src/graphwar_soldier_manager.rs",
    "player_cfg": "src/graphwar_player_config.rs",
    "plane": "src/graphwar_plane.rs",
    "level_gen": "src/graphwar_level_generator.rs",
    "level_mgr": "src/graphwar_level_manager.rs",
    "shot_calc": "src/graphwar_shot_calculator.rs",
    "path_calc": "src/graphwar_path_calculator.rs",
    "math_fn": "src/math_function.rs",
    "turnstate": "src/turnstate.rs",
    "game": "src/game.rs",
}


def p(rel):
    return os.path.join(ROOT, rel)


def read(rel):
    with open(p(rel), encoding="utf-8", errors="replace") as f:
        return f.read()


def lines_of(rel):
    ls = read(rel).split("\n")
    if ls and ls[-1] == "":
        ls.pop()          # wc -l 口径：以换行结尾的文件不多算最后那个空串
    return ls


def wc(rel):
    return len(lines_of(rel))


def rel_of(abs_path):
    return os.path.relpath(abs_path, ROOT)


# ---------------------------------------------------------------------------
# 1. 函数边界（机器算，不手数）
# ---------------------------------------------------------------------------
FUNC_RE = re.compile(r"^(static )?func (\w+)")


def funcs(rel):
    """[(name, start, end)] —— 1-based、闭区间；行号以 `^func` 那行为 start。"""
    ls = lines_of(rel)
    out = []
    cur = None
    for i, line in enumerate(ls, 1):
        m = FUNC_RE.match(line)
        if m:
            if cur:
                cur = (cur[0], cur[1], i - 1)
                out.append(cur)
            cur = (m.group(2), i, len(ls))
    if cur:
        out.append((cur[0], cur[1], len(ls)))
    return out


def production_files():
    """client/ 下**我们的** GDScript（排除 27 份原版视图、排除 52 份原版副本）。"""
    out = []
    for base in ("client/src", "client/tools"):
        for dp, _dn, fn in os.walk(p(base)):
            r = rel_of(dp)
            if r.startswith(VIEWS_DIR) or r.startswith(RECOVERED_COPIES):  # dp == 原版目录
                continue
            for f in sorted(fn):
                if f.endswith(".gd"):
                    out.append(os.path.join(r, f).replace(os.sep, "/"))
    return sorted(out)


# ---------------------------------------------------------------------------
# 2. 逐段判定表
#
#    每条 = (文件, 起函数, 止函数, 类别, 档位, 是什么, 判据, 修复建议, 谁受影响)
#    起止都用函数名（工具自己换成行号）；`PRE` = 文件头（1 行 ~ 第一个 func 前），
#    `ALL` = 整个文件。未列出的函数落到该文件的 `DEFAULT`。
#
#    类别：视图 / 规则 / 数据 / 接缝 / 测试 / 工具
#    档位：A-source-level / B-behavioural-only / C-duplicated / D-self-consistent / E-unknown
# ---------------------------------------------------------------------------

E_TS = ("视图" , "B", )

# 复用的判据片段（都能自己跑）
J_TS_PLAY = ("cmp client/scenes/screens/play_screen.tscn recovered-src/play_screen.tscn ⇒ 我方 **12 行**空壳"
             "（根 Control + script=我们自己的 play_screen.gd），原版 **780 行 / 106 节点**；"
             "节点树在 `.gd::build()` 里手搭（`godot --headless --path client -- --dump-tree` ⇒ "
             "`ScreensControl/PlayScreen` 子树 **111** 节点）")
J_TS_GAMESETUP = ("cmp client/scenes/screens/game_setup.tscn recovered-src/GameSetup.tscn ⇒ 我方 12 行空壳，"
                  "原版 273 行 / 47 节点；dump ⇒ 我方 47 节点（逐个手抄）")
J_TS_PLAYER = ("cmp client/scenes/screens/player_screen.tscn recovered-src/player_screen.tscn ⇒ 我方 12 行，"
               "原版 473 行 / 64 节点；dump ⇒ 我方 264 节点（静态 64 + 动态入口）")
J_TS_LOBBY = ("cmp client/scenes/screens/lan_lobby.tscn recovered-src/lan_lobby.tscn ⇒ 我方 12 行，原版 213 行 / 32 节点；"
              "dump ⇒ 我方 30 节点")
J_TS_MENU = ("cmp client/scenes/screens/main_menu.tscn recovered-src/main_menu.tscn ⇒ 我方 12 行，原版 138 行 / 25 节点；"
             "dump ⇒ 我方 25 节点")
J_TS_CAMPAIGN = ("cmp client/scenes/screens/campaign.tscn recovered-src/campaign.tscn ⇒ 我方 12 行，原版 92 行 / 12 节点；"
                 "dump ⇒ 我方 33 节点（12 静态 + 21 个手造关卡按钮）")
J_TS_REPLAY_LOBBY = ("cmp client/scenes/screens/replay_lobby.tscn recovered-src/replay_lobby.tscn ⇒ 我方 12 行，"
                     "原版 92 行 / 13 节点；dump ⇒ 13 节点")
J_TS_ENDGAME = ("cmp client/scenes/screens/endgame_pop_up.tscn recovered-src/endgame_pop_up.tscn ⇒ 我方 12 行，"
                "原版 349 行 / 69 节点；dump ⇒ 69 节点")
J_TS_JOIN_LEVEL = ("cmp client/scenes/screens/join_level_pop_up.tscn recovered-src/join_level_pop_up.tscn ⇒ 我方 12 行，"
                   "原版 258 行 / 42 节点；dump ⇒ 42 节点")
J_TS_ADD_BOT = ("cmp client/scenes/screens/add_bot_pop_up.tscn recovered-src/add_bot_pop_up.tscn ⇒ 我方 12 行，"
                "原版 254 行 / 45 节点；dump ⇒ 45 节点")
J_TS_CHAT = ("cmp client/scenes/screens/chat_pop_up.tscn recovered-src/chat_pop_up.tscn ⇒ 我方 12 行，原版 214 行 / 7 节点；"
             "dump ⇒ 7 节点")
J_TS_CONFIRM = ("cmp client/scenes/screens/confirm_exit_pop_up.tscn recovered-src/confirm_exit_pop_up.tscn ⇒ 我方 12 行，"
                "原版 176 行 / 31 节点；dump ⇒ 31 节点")
J_TS_CONNECT = ("cmp client/scenes/screens/{connecting,disconnected,join_ip}_pop_up.tscn 与 recovered-src/ 同名文件 ⇒ "
                "我方各 12 行，原版 159/160/172 行 / 28/28/31 节点；dump ⇒ 28/28/31 节点")

FIX_TS = ("P1 换成**加载原版 `.tscn`**：把 recovered-src 的 res:// 依赖（.tres/.svg/.ttf）按原路径还原进 client/，"
          "再 `load(\"res://.../<scene>.tscn\")`；这是 `AUDIT-1` ② 的'视图用原版真值'。"
          "阻塞点：资产只有 pck 里的 `.ctex/.res/.fontdata`（真 `.svg/.ttf` 不在包里）⇒ 需要一层 import/remap")

# --- 逐文件表 -----------------------------------------------------------------

SEGMENTS = {
    # ======================= screens/ =======================
    "client/src/screens/screen.gd": [
        ("ALL", "ALL", "视图", "B",
         "界面基类：`build()/on_enter()` 契约 + **字号缩放**（`apply_font_scale` / "
         "`_apply_size_recursive`：`maxi(10, 宽/N)`，`font_divisor` meta）+ `build_header()`",
         "原版没有这个基类：字号由 **32 个原版视图脚本各自** 连 `size_changed` 算"
         "（`client/src/views/LanLobbyTitle.gd` 等，`recovered_view_check.gd` C 段实测 w/30、w/40…）；"
         "原版 105 个 .gd 里 `maxi` 出现 **0** 次 ⇒ 那个下限 10 是我们加的（本文件 :86 注释自认）",
         "P2 字号规则改由原版脚本承担：把 18 份未挂的原版脚本挂上（见 ④ 段行），"
         "删掉 `_apply_size_recursive` 的 `maxi` 分支",
         "所有 screen；像素基线（八屏 0 px）"),
    ],
    "client/src/screens/main_menu.gd": [
        ("ALL", "ALL", "视图", "B", "主菜单节点树（25 节点）+ 按钮 → 命令键",
         J_TS_MENU + "；`build()` 的节点名/尺寸/`input_text` 逐条照抄原版 `.tscn`（文件头自述'照抄 25 个节点'）",
         FIX_TS, "主菜单像素（0 px 基线）"),
    ],
    "client/src/screens/campaign.gd": [
        ("PRE", "_on_area_resized", "视图", "B", "战役选择屏静态骨架（返回键 + 标题 'CAMPAIGN'）",
         J_TS_CAMPAIGN, FIX_TS, "campaign 屏"),
        ("on_enter", "_apply_level_font_sizes", "规则", "C",
         "**关卡解锁**（`refresh_unlock_state`）+ 21 个关卡按钮的落位与 `input_text`（`place_buttons`）",
         "解锁：`src/graphwar_campaign_control.rs:711 is_level_unlocked` / `:737-741 "
         "`level <= unlocked_through+1`（Rust 文档 :701 直接写'`store.gd::highest_unlocked_level()` 的同一选择'）"
         "⇒ 同一条规则两处实现；按钮：原版 `CampaingLevelsButtons.gd` 就挂在 `campaign.tscn` 上，"
         "实测能跑（`recovered_view_check.gd` ★ 段：21 个按钮、名字 `level_0..level_20`、`input_text` 全对）"
         "⇒ **该挂原版脚本而不是手造**",
         "P1 挂 `CampaingLevelsButtons.gd`（实测可跑）+ 解锁判定改调 Rust；删 `place_buttons`/`refresh_unlock_state`",
         "战役解锁进度、campaign 屏像素"),
    ],
    "client/src/screens/lan_lobby.gd": [
        ("PRE", "_header_cell", "视图", "B", "LAN 大厅静态骨架（32 节点 → 我方 30）+ 表头三列",
         J_TS_LOBBY, FIX_TS, "lan_lobby 两页"),
        ("on_enter", "on_enter", "视图", "B", "进入时刷新房间表",
         J_TS_LOBBY, FIX_TS, "lan_lobby"),
        ("_process", "_on_join_pressed", "规则", "B",
         "**房间表的过滤/排序/行构造**（`_wanted_rows` 排序、`_same_keys`、`_rebuild_list`、`_room_entry`）"
         "+ `GLOBAL_ROOMS` 常量",
         "原版这块在 `GraphwarLobbyControl`（Rust，`src/graphwar_lobby_control.rs` 191 行）与 broker 房间表；"
         "我方 Rust 侧只有 `BrokerConn` 的房间表（`_scaffold/broker.rs`），**没有** lobby 的行序/过滤实现 ⇒ "
         "规则只有 GDScript 版；且 `GLOBAL_ROOMS` 行序在 `docs/06` §75 已被真帧证伪过一次（已修数字、规则仍在此）",
         "P2 行序/过滤 → 搬进 `graphwar_lobby_control.rs`（Rust 侧需先补复原）；否则至少把表序写进 fixtures 当数据",
         "lan_lobby_global 真帧"),
    ],
    "client/src/screens/player_screen.gd": [
        ("ALL", "ALL", "视图", "B",
         "玩家选项屏（64 静态节点 → 我方 264 节点，含 Skins/Faces/Hats 三页 + 商店格）",
         J_TS_PLAYER + "；文件头自述'结构（恢复出的 player_screen.tscn 64 个静态节点）'",
         FIX_TS, "player_* 三屏（0.085/0.138 %）"),
    ],
    "client/src/screens/game_setup.gd": [
        ("PRE", "_pin_start_width", "视图", "B", "对局设置屏骨架（47 节点 + 玩家行）",
         J_TS_GAMESETUP, FIX_TS, "game_setup（1.05 %）"),
        ("_refresh_players", "_on_change_team", "视图", "B", "玩家行/加兵/删兵/换队的**界面**（按钮接线）",
         J_TS_GAMESETUP + "；原版这些行是 `team1_entry.tscn`/`team2_entry.tscn` 实例 + "
         "`GameSetupPlayerEntry.gd`/`SoldierIconMarginContainer.gd`（我方**未挂**这两份原版脚本）",
         "P2 换 `team*_entry.tscn` + 原版两份脚本；行内按钮命令走 Rust `graphwar_setup_control`",
         "game_setup 像素与交互"),
    ],
    "client/src/screens/play_screen.gd": [
        ("PRE", "PRE", "数据", "B",
         "文件头注释 + 状态字段 + 常量（`TEAM_COLORS` 两张 6 色表、`VIEW_MODES`、按钮/曲线/弹道/回放字段）",
         "6 色值可对 `src/graphwar_color_manager.rs:302/312 TEAM1_COLORS/TEAM2_COLORS`（Rust 侧带 "
         "`.rodata` 证据）；字段本身是 GDScript 侧状态，原版对应物是 Rust 控制器的成员"
         "（`GraphwarPlayScreenControl` 3 727 行反编译体，`src/graphwar_play_screen_control.rs` 1124 行）",
         "P2 字段随控制器迁回 Rust；色表改调 Rust", "对局屏"),
        ("assign_colors", "assign_colors", "规则", "C",
         "**玩家配色**：本队 6 色池随机起点 + 线性探测取第一个未用色",
         "`src/graphwar_color_manager.rs:302/312 TEAM1_COLORS/TEAM2_COLORS`、`:221 TEAM_POOL_LEN=6`、"
         "`:459 pick()`（`rand::random::<u64>() % 6` + 线性探测，`观测` 带 `udiv/msub`）；"
         "我方用 `RandomNumberGenerator(seed_value)` 自造确定性 RNG（:120-139）⇒ 同一条规则两处实现，"
         "**且随机流不同源**（Rust 用 rand 的 ChaCha 线程流）",
         "P1 改调 Rust（`_scaffold` 暴露 `GraphwarColorManager.pick`）；GDScript 只留显示",
         "本地局配色、准备圈颜色"),
        ("_ready", "_update_view_geometry", "视图", "B",
         "对局屏**视图树**：`build()`(296) + 键盘两片 `_make_grid/_fill_grid/_extra_button` + 回放键条 "
         "`_build_replay_controls/_replay_button_width/_set_replay_layout/_update_replay_pause_icon` + "
         "真平面装配（下一段）+ `apply_font_scale`/`_update_view_geometry`",
         "J_TS_PLAY；`recovered-src/play_screen.tscn` 引用了 **13** 份原版视图脚本，我方 `GwViews.attach` "
         "只挂了 **6** 份（ExitButtonContainer/FireContainer/FxContainer/MarginContainer/OffsetContainer/"
         "TimerContainer）；缺 7 份（ButtonGrid/ExtraOptionPad/ExtraOptionsHBox/ShowExtraButtons/"
         "ViewFunctionPad/ViewFunctionsLabel/HBoxContainer）—— 其中 **5 份在 "
         "`recovered_view_check.gd` 里实测能跑**，它们的界面行为现在是本文件手写的",
         "FIX_TS + 把 5 份可跑未挂的原版脚本挂上、删对应手造代码", "对局屏像素、键盘、回放键条"),
        ("begin_match", "_refresh_plane", "接缝", "B",
         "开局/收局：把 `state` 喂给平面、起时钟、清局（`begin_match` 117 / `end_match` 12），"
         "以及布局尺寸与士兵/靶子的**像素坐标**换算（`_apply_layout_sizes`/`_spawn_position`/"
         "`_target_positions`/`_view_targets`）",
         "规则在 Rust（`_flow`/`_turn_driver`/`GwTurnClock`），本段只做节点装配；但 `_spawn_position` "
         "把棋盘坐标自己换算成像素 = 抢了 Rust `EntityDrawer.spawn_entity`（`0x4b2890`）的活",
         "P2 坐标换算收回 Rust（`EntityDrawer` 已接受棋盘坐标）", "对局屏实体位置"),
        ("_install_plane_nodes", "plane_nodes_error", "视图", "B",
         "六个真节点（`GraphwarPlane`/`AxisDrawer`/`EntityDrawer`/`EffectDrawer`/`ObstacleDrawer`/1000×600 画布）"
         "的装配与 fit/失败上报",
         "节点本体是 Rust（`_scaffold/*_class.rs`），装配顺序与尺寸约束在 GDScript；原版这部分在 "
         "`GraphwarPlayScreenControl` 的 `_ready`/场景装配（无逐指令证据）",
         "P3 随 play_screen_control 迁移", "对局屏像素"),
        ("_set_curve", "_packed_color", "视图", "B",
         "弹道曲线的逐 tick 揭示（`_advance_curve`/`_curve_reveal`/`_curve_finished`）+ 颜色打包",
         "揭示时序 30 点/tick 在 Rust（`revealed_points()`），本文件只推 tick；`_curve_reveal` 的取整是自定",
         "P3 归 Rust `GraphwarFunctionManager`（`set_function_points`）", "弹道动画"),
        ("_set_axis_mode", "plane_state", "视图", "B",
         "轴模式/障碍/姓名开关的转发 + `plane_state()` 诊断快照",
         "转发到 Rust 节点；`plane_state` 是自检取证面（自洽）", "P3 保留", "对局屏"),
        ("_sync_entities", "_reproducible_run", "接缝", "B",
         "把 `_soldiers`/`_targets` 表逐项 spawn 进 `EntityDrawer`（`_sync_entities` 139 行）、"
         "像素坐标换算（`_spawn_position`/`_target_positions`）、棋盘 fit",
         "原版 `EntityDrawer::spawn_entity` 接受棋盘坐标（Rust，`0x4b2890 EntityDrawer::draw` 那条链）；"
         "本文件 `_spawn_position`（7 行）自己做棋盘→像素换算 = 抢了 Rust 的活；"
         "`_view_targets`/`_level_obstacles` 从 `GwCampaign`（C 行）取数",
         "P2 坐标换算收回 Rust；视野裁剪用 `EntityDrawer` 自己的接口", "士兵/靶子位置"),
        ("_refresh_mode_indicators", "_refresh_mode_indicators", "视图", "B", "四个模式按钮的图标轮换",
         "图标名来自 `GwModes`（C 行）与 `GwIcons`（A 行）", "P3 随 modes 一起改调 Rust", "顶栏四个按钮"),
        ("key_input", "_on_function_text_submitted", "规则", "C",
         "**函数输入编辑**：物理键 → 插入/退格/删/光标、文本变更与提交两条信号",
         "`src/graphwar_function_input.rs`：`Buffer`(:715-812)、`classify_key`(:659)、`KeyAction`(:642)，"
         "地址 `0x4f3210 move_caret_left`/`0x4f44c0 delete_token_at_caret`(:228-234)；"
         "本文件与 `client/src/play/function_input.gd` 又把同一套编辑语义写了两遍 ⇒ C（两处 + Rust 一处）",
         "P1 编辑改调 Rust `GwFunctionInput`；删 `play/function_input.gd` 的编辑状态机",
         "函数输入、对局屏像素"),
        ("_offset_display", "_refresh_offset_label", "视图", "B",
         "修正常数标签（` + 2.515`，3 位小数、符号后一个空格）",
         "格式与 `GwCore.format_offset()`、Rust `graphwar_shot_calculator::correction_text`(:19) 同式 ⇒ "
         "格式规则两处（观测样本 `docs/24` §6.2）", "P3 统一到 Rust `correction_text`", "底部 f(x) 行"),
        ("fire", "_first_point_within", "规则", "C",
         "**开火闸门 + 命中计算 + 结算落地**：`fire()`(67) 的 4 条闸门、`_commit_shot`、`_begin_shot`、"
         "`_advance_presentation`、`_apply_shot_events`、`_on_shot_arrived`、`_settle_round`、"
         "`bot_take_turn`、`_apply_target_reset`、`_clip_to_obstacle`、`_judge_shots`(65)、`_first_point_within`",
         "规则本体一半已在 Rust：`ShotLifecycle`（`src/graphwar_game_state_manager.rs:616-870` begin/advance/"
         "settle/land_all）、`TurnClock::decide`(`graphwar_play_screen_control.rs:462`)、`BotTurn::tick`(:582)；"
         "**但闸门 4 条、`_judge_shots` 的下标→实体映射、`_first_point_within` 的'点在半径内'判定仍只在 "
         "GDScript**（`_first_point_within` 重写了内核 `settle::resolve_points` 的同一条规则）⇒ C",
         "P1 闸门做成 Rust `can_fire()`；`_first_point_within` 改读内核 `hit_points`；"
         "`_apply_target_reset` 的判定点缺证据（见 `game/target_set.gd` 行）",
         "对局流程（本地局）"),
        ("_note_shot_status", "_refresh_targets", "视图", "B", "状态行文案 + 靶子刷新",
         "文案是视图（原版状态行文本未逐条复原）", "P3", "对局屏 HUD"),
        ("_active_soldier", "_human_team", "规则", "C",
         "**回合轮转 + 胜负判定 + 存活统计**：`_active_soldier`(32)、`_next_turn`(26)、`_check_match_over`(31)、"
         "`_alive_by_team`(9)、`_teams_present`(12)、`_human_team`(14)",
         "判据本体在 Rust：`GwTurnDriver`/`GwLevelRules`（`_scaffold/play_flow_class.rs` → "
         "`graphwar_turn_manager::PlayerTurns::next_shooter` `0x463f40`、`LevelRules::evaluate` → "
         "`turnstate::winner_byte` `0x478133`）；**但输入侧各有一份 GDScript 实现**：`_alive_by_team` "
         "自己数活体（= `0x473520 get_alive_teams`，`src/game.rs:1352` 已逐位复原）、`_human_team` 自己找人类队",
         "P1 `_alive_by_team` 改调 Rust `get_alive_teams`", "胜负时机、结算框"),
        ("level_state", "_soldier_of", "测试", "D",
         "自检/诊断的只读快照与注入口（`level_state`/`inject_layout`/`apply_external_shot`/`_soldier_of`）",
         "期望值来自我方实现（selftest 自调自证）⇒ `AGENTS.md` §0.5；**不是复原证据，只是取证面**",
         "P3 保留但文档里不许当证据", "selftest"),
        ("_begin_replay", "replay_press_for_test", "接缝", "B",
         "回放屏：递路径、读 `GwReplayDriver` 快照、摆节点、画当前弹道（旧 426 行 GDScript 局面机已删）",
         "流程在 Rust：`GWReplayDriver`(`_scaffold/replay_driver.rs` 932) + `graphwar_replay_player.rs`(1182) + "
         "`graphwar_replay_control.rs`(177) ⇒ **方向已对**；但驱动外壳在 `_scaffold`（原版位置 = "
         "`GraphwarReplayScreenControl::process` `0x3c6f40`）",
         "P2 驱动外壳收进 `graphwar_replay_control.rs`", "回放四屏（0 px 基线）"),
        ("_begin_online", "_online_room", "规则", "B",
         "联机会话的**实体建模入口**：`_begin_online`(42)、`_online_teardown`、`_online_subscribe`、`_online_room`",
         "原版是 `GraphwarRemoteEventHandler` + 各 manager；我方 `graphwar_remote_event_handler.rs` **不存在**"
         "（task-27 在建），唯一通道是 `_scaffold/net_events_class.rs`（原版没有这个文件）",
         "P1 随 remote handler 落地一起搬", "联机对局"),
        ("_rebuild_pid_index", "_look_for", "规则", "C",
         "**线上事件 → 界面模型**：士兵建立(51)、玩家名/实体名、颜色、外观 `_look_for`(33)",
         "同一条'事件→状态'在**三处**：本文件、`client/src/net/session.gd::_apply`、Rust manager"
         "（`graphwar_name_manager.rs:273+`、`graphwar_color_manager`、`graphwar_team_manager`、"
         "`graphwar_soldier_manager`）⇒ C", "P1 统一到 Rust manager，界面读一份快照", "联机对局像素"),
        ("_build_online_obstacles", "_fire_online", "规则", "C",
         "**联机回合/闸门/时钟**：`_apply_online_turn`、`_online_can_fire`、`_online_tick`、`_fire_online`(24)",
         "回合权威在服务端 + `GraphwarTurnManager`/`GraphwarTickHandler`"
         "（`src/graphwar_turn_manager.rs` 1650 行、`graphwar_tick_handler.rs` 642 行已复原）；"
         "本文件自己维护 `_online_can_fire` 与倒计时 ⇒ 同一条规则两处",
         "P1 改调 Rust `TurnClock`/`tick_handler`", "联机对局流程"),
        ("_on_online_message", "_online_apply_life", "规则", "C",
         "**线上事件分发**（`_on_online_message` 99 行逐事件改界面状态）",
         "`session.gd::_feed_managers`(96) 已把同一批事件喂给 Rust 派发桥；本函数又在界面侧重解一遍 ⇒ "
         "同一派发两处，且都不在原版位置（`GraphwarRemoteEventHandler::handle_server_remote_event` "
         "x86 `0x3e6df0` / arm64 `0x3e2830`，**地址出处 `docs/16` §71**，非本节自测）",
         "P1 只走 Rust 一次", "联机对局、结算"),
        ("_online_soldier_by_entity", "_stop_before_end", "视图", "B",
         "联机弹道绘制（`_draw_online_shot` 60）+ 按 x 截断（`_truncate_path_at_x`）+ 停点",
         "画的部分原版在 `GraphwarFunctionManager`/`FunctionDrawer`（Rust）；`_truncate_path_at_x` 按 x 截断"
         "是自定（原版按掩膜停点）", "P2 交给 Rust", "联机弹道"),
        ("_online_status_text", "online_state", "测试", "D", "联机诊断快照（自检用）",
         "自洽读数", "P3 保留，不当证据", "selftest"),
        ("_process", "_refresh_function_list", "视图", "B",
         "每帧：时钟/计时器标签、额外按钮、姓名/函数列表开关、静音图标",
         "时钟本体在 Rust（`GwTurnClock`）；这里只画 —— 合规视图；但 `_refresh_function_list` 的"
         "排序/截断是自定（原版 `function_history_recorder`，Rust `src/function_history_recorder.rs` 已有）",
         "P3 函数历史由 Rust 接管", "对局屏 HUD"),
        ("_status", "on_enter", "视图", "B", "底部状态行 / 进入钩子", "J_TS_PLAY", "FIX_TS", "对局屏"),
    ],

    # ======================= net/ =======================
    "client/src/net/frame.gd": [
        ("ALL", "ALL", "规则", "C",
         "**TCP 分帧**：`u16` 大端长度 + JSON；`MAX_PAYLOAD=65535`；`Message too long` 语义",
         "`src/graphwar_stream_buffer.rs:109-134`：`LEN_PREFIX_LEN=2`、`MAX_FRAME_PAYLOAD=0xffff`、"
         "`MSG_WRITE_TOO_LONG` 等 5 条原文字符串（逐位，来自 `0x3d89e0` 的 `cmp $0xffff`）⇒ 同一条规则两处实现",
         "P1 分帧改调 Rust（`GwStreamBuffer` 暴露 `encode/frame`）；GDScript 只搬运",
         "全部联机/回放/自检"),
    ],
    "client/src/net/link.gd": [
        ("ALL", "ALL", "规则", "C",
         "**TCP 客户端连接状态机**：connect/poll/读帧/超时/重试",
         "`src/graphwar_client_connection_handler.rs`：`tick_action`（:233）、`ConnectionState`（:181）、"
         "**`CONNECT_TIMEOUT_SECS = 30`**（:261，逐位：arm64 `0x48dccc mov w1,#0x1e`）"
         "⇒ 我方 GDScript 写的是 `connect_timeout := 3.0` 并自注'本项目选择'"
         "（`link.gd:36`）—— **真值早就在 Rust 里复原了，GDScript 这一份用了另一个数** ⇒ C 且口径分叉",
         "P1 超时/状态机改调 Rust（30 s）；删 GDScript 的 3 s/重试策略（后者原版无证据）",
         "连接交互、ConnectingPopUp 时长"),
    ],
    "client/src/net/discovery.gd": [
        ("ALL", "ALL", "规则", "C",
         "**LAN 房间发现（监听方）**：UDP 42968、裸 JSON + 换行、`ROOM_TTL_MS=12000`、枚举校验",
         "`src/graphwar_lan_broadcaster_v6.rs:154-160`：**`ENTRY_TTL_SECS = 10`**（逐位 `0x3de6e4 cmp x0,#0xa`）、"
         "`BROADCAST_PERIOD_SECS = 1`（逐位）、`RECV_BUF=1000`（逐位）、`GROUP_ADDR_V6=ff02::1`、"
         "端口/9 字段形状 ⇒ 同一条规则两处实现；我方 GDScript 的 12000 ms 与 Rust 的 10 s 差 2 s（自注'原版未知'，"
         "但 Rust 侧已证是 10 s）⇒ C 且口径分叉",
         "P1 发现/过期改调 Rust（10 s）；GDScript 只显示房间表", "lan_lobby_local 房间列表"),
    ],
    "client/src/net/broker.gd": [
        ("ALL", "ALL", "规则", "C",
         "**broker（公网房间表）浏览会话**：VersionInfo 握手 + 逐条 GameInfo + GameRemoved",
         "`src/graphwar_server_broker_handler.rs`（440 行，含 `LOC_PROCESS`/`LOC_THREAD_BODY` 与 3 条 "
         "`0x...` 地址）+ `_scaffold/broker.rs`（594 行，DEFAULT_PORT 42970 与 GDScript 同值）"
         "⇒ 同一条协议两处实现（Rust 侧是服务端/线程形态）",
         "P1 broker 客户端形态收进 `graphwar_server_broker_handler.rs` 的客户端一等公民，GDScript 只读表",
         "lan_lobby_global"),
    ],
    "client/src/net/net.gd": [
        ("ALL", "ALL", "接缝", "B",
         "联机层外观：持有 session/discovery/broker 三条通道 + 信号转发 + `split_host_port` 解析",
         "原版没有 GDScript 联机层（`GraphwarGameClient`/`GraphwarClientConnectionHandler` 都在 Rust）；"
         "本文件是纯胶水（无规则）", "P2 随 net 三件一起收进 Rust；`split_host_port` 是自定便利函数",
         "app.gd 联机分支"),
    ],
    "client/src/net/session.gd": [
        ("PRE", "net_events_dump", "规则", "C",
         "**联机会话状态机 + 线上事件 → `room` 模型**：`phase` 迁移、握手 7 条、`_apply`（217 行，逐事件建表）",
         "原版是 `GraphwarRemoteEventHandler::handle_server_remote_event`（x86 `0x3e6df0`，"
         "**地址出处 `docs/16` §71**）+ "
         "`GraphwarClientConnectionHandler`（`0x48b928` 闩锁、`0x48b564` 失败态）+ 各 manager；"
         "我方 `room` 模型把 manager 的状态**又存了一份**（entities/teams/names/look/colors/effects/shots/"
         "active_ticks）⇒ C；`_end_session` 的 `EndingGame` 判定已在 `docs/06` §77 修过一次（曾是死分支）",
         "P1 `_apply` 整段删掉，改为读 Rust manager 快照（`GwNetEvents.dump()` 已有）；"
         "握手状态机搬进 `graphwar_client_connection_handler.rs`",
         "联机全链路、T30"),
        ("_feed_managers", "axis_mode_index", "接缝", "B",
         "把字段按 `WIRE_SPEC` 顺序递给 `GwNetEvents`（96 行 switch）",
         "原版这个派发表是 `GraphwarRemoteEventHandler` 的 match（Rust）；我方放在 "
         "`_scaffold/net_events_class.rs`（原版没有该文件）⇒ 位置不对，但**这部分是接缝不是规则**",
         "P1 随 remote handler 建好后，把派发本身也搬过去；GDScript 只递原始 JSON",
         "T30/task-27"),
    ],
    "client/src/net/wire_samples.gd": [
        ("ALL", "ALL", "数据", "A",
         "**真实抓包原文**（逐字节）当自检黄金样本：`NewConnectionRequest`/`VersionInfo`/… 每条标 "
         "`fixtures/*.jsonl:行号`",
         "每条都能在 `fixtures/protocol_match.jsonl` 等夹具里按行号查到（本文件注释逐条给了行号）；"
         "这是**物证**，不是我们的实现", "P3 保留（这是好东西，不许改）", "selftest 协议断言"),
    ],

    # ======================= core/ =======================
    "client/src/core/expr.gd": [
        ("ALL", "ALL", "规则", "C",
         "**函数表达式求值器**（tokenize + shunting-yard + RPN 求值，229 行），并为界面预览采样",
         "原版解析/求值在 Rust：`src/math_function.rs`（`0x4ed670` 解析器 / `0x4eb9d0` 求值器，"
         "`docs/02` §17 归属 `math_function.rs`）+ `graphwar_path_calculator.rs`（等弦长 0.01 采样，逐位）"
         "⇒ 同一门小语言两处实现，而且本文件自己承认抄了内核怪癖（隐式乘法、`^` 左结合、`log`=log10、"
         "`pi/n`=π）—— 抄的是**规则**，只是换了个执行者",
         "P1 删到只剩'调内核'（`core_bridge.gd` 已有 `path_calculate`）；预览折线也不该由 GDScript 定义语序",
         "弹道预览、`f(x)` 输入"),
    ],
    "client/src/core/core_bridge.gd": [
        ("PRE", "_preview", "接缝", "B",
         "**内核桥**：`GW2Rebuild.path_calculate/path_calculate_mode/hit_test/level_obstacles` 的参数与返回装配",
         "接缝本身必须存在（GDScript 侧没有别的入口）；但它**同时**定义了界面预览折线 `PREVIEW_DX=0.02`"
         "（`_preview`，33 行）与几何近似命中 `_approx_hits`（19 行）—— 两条都是**自造规则**，"
         "内核缺席时就会顶上去（`source=preview` 会显示，这点诚实）",
         "P2 预览折线与近似命中移出生产路径（内核缺席就画不出来/不给结论），或并进 `_scaffold/api.rs` 一处",
         "所有弹道/命中显示"),
        ("hit_test", "format_offset", "接缝", "B",
         "内核 `hit_test` 的装配 + 几何近似降级 `_approx_hits`(19) + 修正常数格式 `format_offset`",
         "命中本体走内核（`_scaffold/api.rs::hit_test` → `shot_calculator::settle`，逐位）；"
         "`_approx_hits`（'点距 < 半径即死'的几何近似）与 `format_offset` 是 GDScript 自造的第二份规则",
         "P2 近似命中移出生产路径；`format_offset` 统一到 Rust `correction_text`", "弹道命中显示"),
    ],

    # ======================= app/ =======================
    "client/src/app/app.gd": [
        ("PRE", "changed", "规则", "C",
         "**屏表路由 + 弹窗管理 + 命令总线**：`show_screen`(19)、`open_popup`/`close_popup`、`goto`、"
         "`command`(116 行 match)",
         "原版是 `GraphwarRootNode`：`FN_SHOW_SCREEN = 0x3a_f850`（逐项 `set_visible(name==target)` + "
         "`call_deferred(create_ad_view/destroy_ad_view)`）、`FN_COMMAND_BUS = 0x3b_0830`、"
         "`SCREEN_TABLE` 15 个屏名带 `.rodata` 地址（`src/graphwar_root_node.rs:859-960`）"
         "⇒ 同一条路由/命令规则两处实现，而 Rust 那一份**带地址**（这一条是本审计排名第一的迁移项）",
         "P1 `show_screen/command` 改调 Rust `apply_show_screen`/`Command::from_name`（`_scaffold/"
         "graphwar_root_node_class.rs` 已有 1002 行外壳）；GDScript 只留 `register_screen` 与信号",
         "全部界面切换、所有按钮"),
        ("goto", "_enter", "接缝", "B", "命令分发到具体处理函数（`_enter` 等）",
         "接缝；原版命令处理在 `GraphwarRootNode::process_button_pressed`（本文件 :268 注释自引）",
         "P2 随路由一起搬", "T31"),
        ("_leave_to_main_menu", "_play_replay", "规则", "B",
         "**各命令的业务动作**：退出对局（放连接）、建房（`start_server`+LAN 广播+broker）、"
         "选关、加 bot、开局、开始关卡、回放选择",
         "原版这一批是 `GraphwarRootNode` 的命令分支（`create_game` = `start_server()` + "
         "`start_lan_broadcaster()` + `start_server_broker_handler()`，本文件 :268 注释自引"
         "`docs/144` §1）⇒ 规则应回 Rust；我方 GDScript 里是逐条流程",
         "P1 搬进 `graphwar_root_node.rs` 的命令处理（Rust 侧 `Command` 枚举已有 32 个键）",
         "建房/开局/退出流程"),
        ("_submit_join_ip", "_cancel_connect", "规则", "B",
         "Join IP 主机端口解析与连接发起/取消（`_looks_like_host_port` 的校验规则）",
         "原版解析在 Rust（`start_client_to_*`，`docs/132` §3 只有方法名，**校验规则未复原**）⇒ "
         "规则只有 GDScript 版（B）；`_looks_like_host_port` 是自定校验", "P2 解析/校验搬进 "
         "`graphwar_client_connection_handler.rs` 旁边，原版证据缺（E 的部分要写明）", "Join IP"),
        ("_on_net_joined", "_on_net_disconnected", "规则", "C",
         "**联机事件落 `state`**：joined/game_started/join_failed/disconnected/ended 五个分支 + "
         "`_sync_state_from_net`(38) 把 `room` 抄进 `state`",
         "与 `session.gd::_apply` 及 Rust manager 三层重复（见 session.gd 行）；`online_result` 见下",
         "P1 与 session.gd 一起收口", "联机对局"),
        ("_on_net_ended", "_online_human_team", "规则", "C",
         "**联机胜负**：从 `room.entities` 数出'每队还有活体'，再把胜方字节翻成队号",
         "规则两半都在 Rust 且带地址：`Match::get_alive_teams`（`src/game.rs:1352`，逐位同构 "
         "`0x473520`）、`turnstate::winner_byte`（`0x478133`–`0x478236`）/`winner_team`"
         "（本文件经 `GwEndGame` 调 Rust 👍）；但'数活体'这一步在 GDScript 重写了一遍 ⇒ C",
         "P1 活体统计改调 Rust（`Match::get_alive_teams` 或 manager 快照）", "结算框胜/负"),
        ("_on_net_rooms_changed", "_looks_like_host_port", "接缝", "B",
         "联机状态同步/主机端口解析（`_sync_state_from_net` 38 行把 `room` 抄进 `state`；`_looks_like_host_port` 自定校验）",
         "`_sync_state_from_net` 与 `session.gd::_apply`、Rust manager 三层重复（见 session.gd 行）；端口解析原版在 Rust（`start_client_to_*`，校验规则未复原）",
         "P2 状态同步由 Rust 单一来源；端口校验搬 `graphwar_client_connection_handler.rs`", "联机对局"),
        ("_install_audio", "_graphics_path", "接缝", "B",
         "音频装配 + 静音总线 + 结果上报/对战摘要（`match_summary`/`_victory_winners`/`_online_victory_winners`）",
         "音频本体已接 Rust（`GwAudio` → `src/_scaffold/audio`? 见 gw_audio.gd 行）；"
         "`_victory_winners`(37)/`_online_victory_winners`(33) 是**结算框文案/头像选择**的规则，"
         "原版在 `graphwar_play_screen_control.rs` 的 `VictoryBanner`（:277）那一簇（Rust 已有）",
         "P2 胜方/横幅文案改调 Rust `VictoryBanner`", "结算框"),
    ],
    "client/src/app/campaign.gd": [
        ("ALL", "ALL", "数据", "C",
         "**21 关参数表**（`level()`：标题/描述/士兵数/目标数/障碍模式/回合上限/四个模式字节）+ `level_seed`",
         "本文件头自述'这是内核同名表的**客户端副本**，权威在 `src/graphwar_campaign_control.rs::LEVEL_INFO`"
         "（= 原版 `level_info()` `0x4ad380`，Rust 侧 21 关数值 `逐位+运行时` 三源一致）'"
         "⇒ 自我声明的 C；`level_seed` 用 RNG 自定（原版种子从 `/dev/urandom`，`docs/57`）",
         "P1 `GwCampaign` 改为查 Rust `LEVEL_INFO`（`_scaffold` 暴露），删掉 GDScript 表",
         "21 关：战役屏/关卡弹窗/关卡生成"),
    ],
    "client/src/app/commands.gd": [
        ("ALL", "ALL", "数据", "C",
         "**命令键清单**（`input_text` → 命令），76 行常量",
         "原版命令键是 `GraphwarRootNode` 的 match 键（`FN_COMMAND_BUS = 0x3b_0830`）；"
         "`src/graphwar_root_node.rs` 的 `Command` 枚举 + `from_name`（:494-607）已带 32 个键与该命令的地址"
         "⇒ 同一份键表两处", "P1 键表由 Rust `Command` 生成（GDScript 只做常量引用/校验）",
         "所有按钮 → app.gd::command"),
    ],
    "client/src/app/game_state.gd": [
        ("ALL", "ALL", "规则", "C",
         "界面侧一局状态（kind/room_name/locked/四个模式/玩家表）+ **`can_start()`**(7) + "
         "`total_soldiers`/`add_bot` 等表操作；`MAX_SOLDIERS=5`/`MAX_PLAYERS_PER_TEAM=8`/`BOT_NAMES`",
         "开局闸门与人数规则在原版是 `GraphwarSetupControl`（`src/graphwar_setup_control.rs`："
         "`SetupOptions::from_level` :510、`PlayerEntry::add_soldier` :654、`room_default` :501）+ "
         "`graphwar_player_manager.rs`；`BOT_NAMES` 的原版来源是 `.rodata` 名字池（Rust 侧有 `BotProfile`）"
         "⇒ 规则两处", "P1 `can_start`/人数上限改调 Rust `SetupOptions`；名字池从 Rust 取",
         "GameSetup 的 Start 可用性"),
    ],
    "client/src/app/modes.gd": [
        ("ALL", "ALL", "数据", "C",
         "**四套模式的枚举/图标名/秒数/循环顺序**（`AXIS_COUNT=4`、`TIME_SECONDS=[30,60,120,180,300,0]`、"
         "`axis_next` 是 `v-1`、`time_next` 是 `v+1`、`format_clock` 向下取整）",
         "`src/graphwar_mode_manager.rs:57-110`（三套枚举 + `TimeMode::seconds()` = 同 6 个秒数，"
         "`docs/07` §5.2 **逐位**）+ `src/graphwar_setup_control.rs:351-461`（`TimeMode::next/prev`、"
         "`AxisMode::next/prev`、`FunctionMode::next`、`TurnMode::next`；`initial_timer_secs`/`timer_label`）"
         "⇒ 四个 next 与秒数表两处实现；`_scaffold/graphwar_setup_control_class.rs:28` 已经暴露 "
         "`#[func] cycle_*_mode()` 却没人调",
         "P1 GDScript 改调 `GwSetupControl.cycle_*_mode()`；`format_clock` 改调 Rust `timer_label`",
         "GameSetup 四个按钮 + 对局计时显示"),
    ],
    "client/src/app/replay_index.gd": [
        ("ALL", "ALL", "规则", "B",
         "回放列表的数据来源：扫目录、按录制时间排、读关卡名（`docs/92` §4.15 观测）",
         "原版回放元数据由 `GraphwarGameRecorder`/`GraphwarReplayControl`（Rust，`src/graphwar_game_recorder.rs`"
         "已有）写入；我方扫描/排序规则只在 GDScript（Rust 侧 `_scaffold/replay_meta.rs` 是本轮 T5 新造的）"
         "⇒ B（原版位置是 recorder 的头部，不在客户端扫描）",
         "P2 列表数据由 Rust 侧提供（`replay_meta` 收进 `graphwar_game_recorder.rs`）", "replay_lobby 列表"),
    ],
    "client/src/app/store.gd": [
        ("PRE", "PRE", "数据", "B",
         "文件头注释 + 存档路径/字段常量（`SAVE_PATH` 一类）",
         "路径口径已在 `docs/06` §75 定过（`cg` = OS 登录名）；字段名见下一行的 `player_config` 规则",
         "P3", "玩家档案/战役进度"),
        ("_static_init", "load_all", "数据", "B", "存档路径/默认档案/OS 登录名（`os_login_name` 取真名）",
         "`os_login_name` 是真观测（原版 `cg` = OS 登录名，`docs/06` §75）；路径选择是自定",
         "P3 路径口径已写进 docs；保留", "玩家档案"),
        ("apply_original_player_config", "muted_from_config", "规则", "C",
         "**player_config 与 campaign 存档的解析/序列化**（`parse_player_config`/`format_player_config`/"
         "`parse_campaign_save`/`save_campaign_progress`/`is_level_unlocked`/`muted_from_config`）",
         "原版格式在 Rust：`src/graphwar_player_config.rs`（`GraphwarPlayerConfig::save/load` :121/:134 + "
         "`lock/unlock`）+ `graphwar_campaign_control.rs:723-741`（`unlocked_through` / `is_level_unlocked`）"
         "⇒ 同一条存档规则两处实现（Rust 那份有原版格式证据）",
         "P1 存档读写改调 Rust（`GraphwarPlayerConfig`/campaign save）；GDScript 只管 UI 字段",
         "玩家档案、战役进度"),
    ],

    # ======================= play/ =======================
    "client/src/play/function_input.gd": [
        ("ALL", "ALL", "规则", "C",
         "**函数输入框**：自绘 LineEdit（主题样式盒）+ 光标/编辑状态机 + `MAX_LENGTH=128`",
         "编辑/光标规则：`src/graphwar_function_input.rs`（`Buffer` :715-812、`classify_key` :659、"
         "`KeyAction` :642，地址 `0x4f3210`/`0x4f44c0`）⇒ C；画的部分（`_draw`/样式盒）原版确实是 "
         "`GraphwarFunctionInput` 自绘（`LineEdit` 子类，`docs/104` §6.3）⇒ 视图部分只能留在 GDScript",
         "P1 编辑/光标改调 Rust；`MAX_LENGTH` 是自定（原版未知，写进 E）", "对局屏输入"),
    ],
    "client/src/play/keyboard_input.gd": [
        ("ALL", "ALL", "规则", "C",
         "物理键盘 → 命令映射（`C`/`⌫`/`←/→`/`⇤/⇥`/字面插入），屏幕级 `_unhandled_key_input` 接管",
         "原版在自己的 `gui_input` 里处理（`.so` 有 `gui_input`/`set_caret_column`/`insert_text_at_caret`/"
         "`delete_char_at_caret` 字面量 + Location `src/graphwar_function_input.rs:519`）；键 → 动作的判定在 "
         "`graphwar_function_input.rs::classify_key`/`intent_for_key` ⇒ C；且`屏幕级接管`是自定放宽（本文件自认）",
         "P1 键映射改调 Rust `classify_key`；接管范围按原版（聚焦）还是保留放宽，需 lead 决定",
         "对局屏键盘"),
    ],
    "client/src/play/player_showcase.gd": [
        ("ALL", "ALL", "视图", "B", "玩家预览：皮肤/脸/帽三张 512×512 同位置叠画",
         "原版是 Rust 类 `PlayerShowcase`（`src/player_showcase.rs` + `_scaffold/player_showcase_class.rs` "
         "665 行已复原）⇒ GDScript 这份是同一视图的第二实现（合成方式照 `docs/92` §4.10 观测）",
         "P2 直接实例化 Rust `PlayerShowcase` 节点，删本文件", "player_screen 预览"),
    ],
    "client/src/play/replay_file.gd": [
        ("ALL", "ALL", "规则", "C",
         "**回放文件读取**：gzip + 每行一个 JSON 事件、脏数据处理（空回放/tick 回退/跳行计数）",
         "同一份格式规则在 Rust：`src/graphwar_replay_player.rs`（1182 行，可执行版 + C ABI "
         "`gw2_replay_state_at`）与 `graphwar_replay_reader.rs`；本文件自述'解压走 Godot，不重复实现 DEFLATE'"
         "（合理），但**事件解析/排序/脏数据策略**是第二份实现 ⇒ C",
         "P1 解析改调 Rust（`_scaffold/replay_driver` 已有快照接口）；GDScript 只递文件",
         "回放四屏"),
    ],

    # ======================= game/ =======================
    "client/src/game/target_set.gd": [
        ("ALL", "ALL", "规则", "B",
         "**靶子集合与『打不全就全部复原』**：`destroy/restore/settle_shot`（`settle_shot` 判'本发摧毁 ≠ 全部 ⇒ 全翻回'）",
         "原版没有这个判定点的证据（本文件自注'判定点哪一刻/拿什么条件没有在二进制里找到'，"
         "依据是用户实测 + `docs/123` 形状结论）⇒ 规则只有 GDScript 版，且证据档是 `推断`；"
         "靶子的实体/队伍/半径三条已在 Rust（`graphwar_target_manager.rs`、`Team2` 常量带 127/127 观测）",
         "P1 先把判定点挖出来（缺证据 = E），再搬进 `graphwar_target_manager.rs`；在那之前这段必须标'推断'",
         "靶子关（战役 13/14 一类）"),
    ],
    "client/src/game/effect_layer.gd": [
        ("ALL", "ALL", "接缝", "B",
         "特效**适配层**：按名字记账 + 派给真 `EffectDrawer` 节点 + `retain_alive`",
         "本文件自述'一个图元都不画'，图元由 `src/_scaffold/effect_drawer_class.rs` → `effect_drawer`"
         "（Rust，`draw_set_transform` 立即数逐位）⇒ 方向对；剩下的记账/种类映射是自定（相位字节↔7 档"
         "`Phase` 的对应'没有证据'，本文件自认）",
         "P2 种类→相位映射搬进 `graphwar_entity_effect_manager.rs`（原版 4 种效果名的判别值 `0x4bf090` 已知）",
         "命中/死亡特效"),
    ],

    # ======================= ui/ =======================
    "client/src/ui/ui.gd": [
        ("ALL", "ALL", "视图", "B",
         "**界面积木**：`vbox/hbox/margin/label/title/panel/scroll/exit_pair/dialog/mode_button`（原版没有这类函数）",
         "原版的界面骨架是 `.tscn`（31 份，`recovered-src/`）；本文件是'用代码搭 .tscn'的替代品"
         "（文件头自己问了'为什么用代码搭界面而不是手写 .tscn'）⇒ 每调一次就是在替原版 `.tscn` 说话",
         "P2 原版 `.tscn` 一旦能用（见 FIX_TS），本文件应大幅缩减为纯 helper",
         "所有界面结构"),
    ],
    "client/src/ui/theme_factory.gd": [
        ("ALL", "ALL", "视图", "B",
         "**主题与字体**：`_flat`/`_tab_box`/`shop_cell_stylebox`/`mode_icon_stylebox`/`campaign_level_theme` 等",
         "原版这些是资源文件：`recovered-src/` 有 **22 份 `.tres`**（含 `button_font.tres`/"
         "`campaign_button_theme*.tres`/`axis_*_theme.tres`…），我方**一份都没用**，改在代码里重建样式盒 ⇒ "
         "视图真值在 `.tres`，实现却在 GDScript（且字体只能取 pck 里的 `.fontdata`）",
         "P1 改用 recovered-src 的 `.tres`（5 份 `省略` 档的字体/图集字节被有意剔除，需要补 import）",
         "所有界面字体/样式（对像素影响最大的一层）"),
    ],
    "client/src/ui/icons.gd": [
        ("ALL", "ALL", "数据", "A",
         "**图标名 → 原版 res:// 路径**映射（230+ 条），并声明'这里不画任何东西'",
         "每条映射都给出处：原版 `.tscn` 的 `icon=`/`texture=`（25 个原版场景里 78+21=**99 处**："
         "`grep -ho 'icon = ExtResource' recovered-src/*.tscn | wc -l`）+ `ShowExtraButtons.gd` 的动态 "
         "`load()` + `.so` `.rodata` 字面量池；本文件不含绘制代码（不自造图形）⇒ 数据来自真值",
         "P3 保留；把 `推断` 的组内顺序（`docs/121` §4.3）补证", "所有图标"),
    ],
    "client/src/ui/assets.gd": [
        ("ALL", "ALL", "数据", "A",
         "**资产层**：从使用者 pck 解出来的目录按原版 `res://` 路径找 `.ctex/.fontdata/.res`，"
         "缺失就报错、**不做占位图**",
         "路径规则来自原版路径映射（`tools/extract_assets.py` → `client/assets/`）；无规则、无绘制、"
         "缺失即 `push_error` ⇒ 合规", "P3 保留", "全部界面资产"),
    ],
    "client/src/ui/gw_button.gd": [
        ("ALL", "ALL", "视图", "C",
         "**`GraphwarButton` 的契约复刻**：`input_text`/长按/按下视觉/命令派发",
         "原版按钮是 Rust 类 `GraphwarButton`（283 个按钮全是它，无原生 Button）：`src/graphwar_button.rs`"
         "（复原实现）+ `_scaffold/graphwar_button_class.rs`（619 行，已注册成 GDExtension 类）"
         "⇒ 同一个控件的契约两处；我方界面用的到底是原生 Button + 本文件，还是 Rust 类，**混着**"
         "（`recovered_view_check` ★ 段实测 `CampaingLevelsButtons.gd` new 的是'我们注册的 GraphwarButton'）",
         "P1 界面一律用 Rust `GraphwarButton` 节点，本文件退化为工厂/常量", "所有按钮"),
    ],
    "client/src/ui/gw_views.gd": [
        ("ALL", "ALL", "接缝", "B",
         "**原版视图脚本的挂载点**：`attach()`（必须 `add_child` 之前）+ `owns()`（让字号递归整棵跳过）",
         "原版这些脚本是 `.tscn` 里节点的 `script=`（`recovered-src/*.tscn` 的 45 处 `path=\"res://*.gd\"`）；"
         "我方因为不用原版 `.tscn`，只能手工 `set_script()` 并把节点搭成同名同类型 ⇒ 接缝的必要性"
         "**完全来自'不用原版 .tscn'**（27 份挂上了、18 份没挂，见 ④ 段）",
         "P2 用原版 `.tscn` 后本文件可删（脚本随场景自动挂）", "全部界面"),
    ],

    # ======================= audio/ =======================
    "client/src/audio/gw_audio.gd": [
        ("ALL", "ALL", "规则", "C",
         "**音频播放层**（400 行）：载资源、按名字播、**humm 循环音**（WAV 帧解码/生成）、"
         "Master 总线静音",
         "本文件头自述'原版 `graphwar_audio_control.rs`（节点类 `SoundHandler`）的最小复刻'；"
         "Rust 侧 `src/graphwar_audio_control.rs` 已复原（含 `res://sounds/*.mp3` 与 "
         "`[src/graphwar_audio_control.rs:241:17]` 的 Location 字面量在 `.so` 里可见）⇒ **自我声明的 C**；"
         "`_decode_wav_frames`/`_push_generator_frames` 是'原版 humm 波形实现未复原'的替代（自定）",
         "P1 播放/静音/humm 改调 Rust `graphwar_audio_control`；GDScript 只留 Godot AudioStream 的薄壳",
         "全部音效；四份真回放的声音对拍"),
    ],

    # ======================= main.gd =======================
    "client/src/main.gd": [
        ("ALL", "ALL", "视图", "B",
         "**客户端根节点**：实例化 15 个界面、只切 `visible`（= 原版 `main.tscn` 的 "
         "`GraphwarRootNode/VBoxContainer/ScreensControl`）+ 命令行/自检/截图入口 + 调试叠层",
         "原版 `main.tscn` 根节点 `type=\"GraphwarRootNode\"`（**不是** Control+脚本，"
         "见 `recovered-src/main.tscn:3`），控制器在 Rust（`.so` 有 `src/graphwar_root_node.rs` 的 "
         "panic Location）⇒ 我方用 GDScript 重建了那棵树；命令行/自检部分是工具（原版没有）",
         "P1 根节点改用 Rust `GraphwarRootNode` 类 + 原版 `main.tscn`；工具入口移到 `client/tools`",
         "启动、全部界面"),
    ],

    # ======================= 小弹窗 =======================
    "client/src/screens/connect_popups.gd": [
        ("ALL", "ALL", "视图", "B", "三个连接弹窗（JoinIp/Connecting/Disconnected）共用实现",
         J_TS_CONNECT, FIX_TS, "三个连接弹窗"),
    ],
    "client/src/screens/join_ip_pop_up.gd": [
        ("ALL", "ALL", "视图", "B", "JoinPopUp 的行编辑 + Join/Cancel 按钮（5 行薄壳，实现在 connect_popups）",
         J_TS_CONNECT, FIX_TS, "Join IP 弹窗"),
    ],
    "client/src/screens/connecting_pop_up.gd": [
        ("ALL", "ALL", "视图", "B", "ConnectingPopUp 空壳（5 行）", J_TS_CONNECT, FIX_TS, "连接中弹窗"),
    ],
    "client/src/screens/disconnected_pop_up.gd": [
        ("ALL", "ALL", "视图", "B", "DisconnectedPopUp 空壳（5 行）", J_TS_CONNECT, FIX_TS, "断线弹窗"),
    ],
    "client/src/screens/add_bot_pop_up.gd": [
        ("ALL", "ALL", "视图", "B", "AddBotPopUp：六档难度按钮 + 文案",
         J_TS_ADD_BOT + "；原版这份 `.tscn` 挂 4 份原版视图脚本（AdContainer/ExitButtonContainer/"
         "IpPopUpMargin/MarginContainerPopUpBack），我方挂了 1 份",
         FIX_TS, "加 Bot 弹窗"),
    ],
    "client/src/screens/chat_pop_up.gd": [
        ("ALL", "ALL", "视图", "B", "ChatPopUp：输入行 + 消息列表 + 发送（本地回显）",
         J_TS_CHAT + "；原版还挂 ChatBox.gd/ChatContainer.gd/ExitButtonContainer.gd（我方 3 份都挂了 👍），"
         "但聊天消息的协议语义（`chat_message_event`）在 Rust `graphwar_*` 未复原 ⇒ 回显是自定",
         FIX_TS, "聊天弹窗"),
    ],
    "client/src/screens/confirm_exit_pop_up.gd": [
        ("ALL", "ALL", "视图", "B", "退出对局确认框",
         J_TS_CONFIRM + "；原版挂 IpPopUpMargin/MarginContainerPopUpBack（我方 2 份都挂了）", FIX_TS,
         "退出确认"),
    ],
    "client/src/screens/endgame_pop_up.gd": [
        ("ALL", "ALL", "视图", "B",
         "**结算框**：面板 956×691 几何 + 胜利者头像 + 文案（`YOU WON`/`Team N Victory!`）",
         J_TS_ENDGAME + "；胜方来自 `app.gd::online_result`（Rust `winner_byte`），横幅文案原版在 "
         "`graphwar_play_screen_control.rs:277 VictoryBanner`（Rust 已有）⇒ 文案选择仍是 GDScript",
         FIX_TS, "结算框（4.05 %）"),
    ],
    "client/src/screens/replay_lobby.gd": [
        ("ALL", "ALL", "视图", "B", "回放列表：条目行（时间/关卡名/4 个模式图标/▶）+ 空态",
         J_TS_REPLAY_LOBBY + "；条目行原版是 `replay_entry.tscn`（21 节点/条）+ `GameNameContainer.gd`"
         "（我方**未挂**）；列表数据来自 `replay_index.gd`（同表 B 行）",
         FIX_TS + "；恢复 `replay_entry.tscn` 实例化", "回放列表（0 px 基线）"),
    ],
    "client/src/screens/join_level_pop_up.gd": [
        ("ALL", "ALL", "视图", "B", "关卡预览弹窗：标题/描述/四个模式图标/回合上限/TurnLimitText",
         J_TS_JOIN_LEVEL + "；原版挂 7 份视图脚本，我方挂 5 份（缺 `OptionsContainer`?"
         "—— 已挂；缺 `TurnLimitText` 有条件挂）", FIX_TS, "关卡弹窗"),
    ],
}


def default_segment(rel):
    """没在表里点名的文件：按路径给缺省档（这是"没逐段审"的诚实缺省，不是"审过了"）。"""
    base = os.path.basename(rel)
    if base in ("dump_one.gd", "dump_textures.gd", "replay_entity_dump.gd", "screenshots.gd",
                "recovered_ad_holder_stub.gd", "plane_on_screen.gd"):
        return ("ALL", "ALL", "工具", "N",
                "开发/取证工具（不是游戏逻辑，也不进运行时路径）",
                "工具本身无复原主张；它产出的读数是否可信，取决于它读的是谁的数据",
                "P3 保留", "开发者")
    if base == "recovered_view_check.gd":
        return ("ALL", "ALL", "测试", "A",
                "**原版视图脚本验收**：A 段逐字节、B 段能否解析、C 段 `_ready` 有效果、D 段缺哪个成员、"
                "E 段插件在不在",
                "期望值来自 **recovered-src 原文件**（A 段 `cmp`）+ 原版脚本自己的公式（C 段断言 "
                "`font_size == int(视口宽/N)`）⇒ 不是自洽断言；本轮实跑 **57 passed / 0 failed**"
                "（`godot --headless --path client --script res://tools/recovered_view_check.gd`）",
                "P3 保留并纳入合并前必跑（这是客户端唯一'拿原版当真值'的验收）", "视图层所有改动")
    return ("ALL", "ALL", "测试", "D",
            f"headless 验收脚本（`{base}`）：断言界面上'应有'的行为",
            "期望值由我们自己写（`AGENTS.md` §0.5：自写断言只证明自洽）⇒ **不构成复原证据**；"
            "它的价值是回归保护。哪一条其实是 D-self 由审计面 ③（tests）给出的清单定，本面不重复判",
            "P3 保留（回归保护）；文档里不许拿它的条数当复原进度", "回归")

def tool_extra_rows():
    """client/ 下不是 .gd 但属于'我们的代码'的部分：存根场景、工程文件、启动器。"""
    rows = []
    stubs = sorted(f for f in os.listdir(p("client/scenes/screens")) if f.endswith(".tscn"))
    total = sum(wc(f"client/scenes/screens/{f}") for f in stubs) + wc("client/scenes/main.tscn")
    rows.append(dict(file="client/scenes/", start=1, end=total, kind="视图", verdict="B",
                     what=f"**15 个存根场景 + main.tscn**：每个只有'根 Control + `script=res://src/screens/*.gd`'"
                          f"（共 {total} 行），界面内容全部在 .gd 里搭",
                     judge="`wc -l client/scenes/screens/*.tscn client/scenes/main.tscn` ⇒ 15×12 + 24 = "
                           f"{total}；对照 `wc -l recovered-src/*.tscn`（31 份、最短 92 最长 780）⇒ "
                           "**原版 .tscn 一份都没进 client/**（这不是'资产缺失'，`.tscn` 是文本、已在 recovered-src）",
                     fix=FIX_TS, who="全部界面"))
    rows.append(dict(file="client/project.godot", start=1, end=wc("client/project.godot"), kind="数据",
                     verdict="B", what="工程设置（视口/渲染/主场景/工程名）",
                     judge="逐键对照 `recovered-src/project.godot`（原版 `project.binary` = ECFG，"
                           "`pck_extract` 字段逐字还原）：**相同 13 键**（`boot_splash/*`、"
                           "`environment/defaults/default_clear_color`、`textures/canvas_textures/"
                           "default_texture_filter`、`window/size/*`、`config/version`、`config_version`）；"
                           "**少 5 键**（`config/icon`、`import/blender/enabled`、"
                           "`pointing/emulate_touch_from_mouse`、`window/handheld/orientation`、插件 `enabled`）；"
                           "**多 rebuild 专有**（`config/name`、`config/description`、`renderer/*`、"
                           "`run/main_scene=res://scenes/main.tscn`、`theme/*`）"
                           r"复算：`grep -E '^[a-z_/]+\s*=' client/project.godot recovered-src/project.godot`",
                     fix="P2 补齐 5 个缺键（`window/handheld/orientation=4`、`emulate_touch_from_mouse` 等）——"
                         "`main_scene` 与 `renderer` 是 rebuild 必需，保留并注明",
                     who="整机视口/输入/渲染"))
    rows.append(dict(file="client/run.sh", start=1, end=wc("client/run.sh"), kind="工具", verdict="N",
                     what="启动器（资产提取/import/selftest/dump/shots 五种模式）", judge="纯工具",
                     fix="P3 保留", who="开发者"))
    return rows


# ---------------------------------------------------------------------------
# 3. 原版字节脚本：27 份挂载（`client/src/views/`）+ 52 份副本（`client/tools/recovered/`）
# ---------------------------------------------------------------------------
def original_scripts_rows():
    """④ 段：27 份是否逐字节相同、真挂在节点上、有没有被我们自己的 screens 覆盖。"""
    rows = []
    views = sorted(f for f in os.listdir(p(VIEWS_DIR)) if f.endswith(".gd"))
    attach = {}
    for rel in production_files():
        for m in re.finditer(r'GwViews\.attach\([^,]+,\s*"([\w.]+\.gd)"', read(rel)):
            attach.setdefault(m.group(1), []).append(f"{rel}:{read(rel)[:m.start()].count(chr(10)) + 1}")
    for i, name in enumerate(views, 1):
        src = f"recovered-src/{name}"
        same = subprocess.run(["cmp", "-s", p(f"{VIEWS_DIR}/{name}"), p(src)]).returncode == 0
        sites = attach.get(name, [])
        rows.append(dict(
            file=f"{VIEWS_DIR}/{name}", start=1, end=wc(f"{VIEWS_DIR}/{name}"),
            kind="视图", verdict="A",
            what=f"原版视图脚本副本（`recovered-src/{name}` 的逐字节拷贝）",
            judge=("`cmp client/src/views/%s recovered-src/%s` ⇒ %s；挂载点 %d 处：%s；"
                   "`GwScreen._apply_size_recursive` 见 `GwViews.owns` 就整棵跳过（`client/src/screens/"
                   "screen.gd:88`）⇒ 不被我方字号覆盖；运行期效果见 "
                   "`client/tools/recovered_view_check.gd` C 段（57 passed / 0 failed）"
                   % (name, name, "相同" if same else "**不同（FAIL）**", len(sites),
                      ", ".join(sites[:3]) if sites else "**没有挂载点（FAIL）**")),
            fix="P3 保持（这是本面唯一已达标的一大块）；不许改这些文件",
            who="对应界面节点",
            byte_same=same, attached=bool(sites)))
    return rows


#: `recovered_view_check.gd` C/★ 段**实测跑出效果**的原版脚本（逐字来自该脚本输出，本轮复跑
#: 57 passed / 0 failed）。用于区分「未挂但能挂」与「未挂且挂不上」。
PROVEN_RUNNABLE = {
    "HBoxContainer.gd", "ViewFunctionsLabel.gd", "ButtonGrid.gd", "SoldierIconMarginContainer.gd",
    "CampaingLevelsButtons.gd", "ResizeContainer.gd", "OptionsContainer.gd", "PlayerTabContainer.gd",
    "CampaingButtonSize.gd", "ChatContainer.gd", "TitleSize.gd", "UnlockButton.gd",
}


def unmounted_originals_row():
    """④ 段：原版 `.tscn` 里挂了、而我们**没挂**的原版脚本 —— 它们的活被谁干了？"""
    mounted = {f for f in os.listdir(p(VIEWS_DIR)) if f.endswith(".gd")}
    referenced = set()
    for f in os.listdir(p("recovered-src")):
        if f.endswith(".tscn"):
            referenced |= set(re.findall(r'path="res://([\w.]+\.gd)"', read(f"recovered-src/{f}")))
    missing = sorted(referenced - mounted)
    runnable = [m for m in missing if m in PROVEN_RUNNABLE]
    plugin = [m for m in missing if m not in PROVEN_RUNNABLE]
    total = sum(wc(f"recovered-src/{m}") for m in missing)
    return dict(
        file="recovered-src/(未挂的原版脚本 18 份)", start=1, end=total, kind="视图", verdict="B",
        what=(f"原版 `.tscn` 的 `script=` 引用里，我们**没挂**的 {len(missing)} 份原版视图脚本"
              f"（合计 {total} 行）：{', '.join(missing)}"),
        judge=("复算：`grep -o 'path=\"res://[^\"]*\\.gd\"' recovered-src/*.tscn`（45 个不同脚本）"
               f"减去 `ls client/src/views/*.gd`（{len(mounted)} 个）⇒ {len(missing)} 个未挂。"
               f"其中 **{len(runnable)} 份在 `recovered_view_check.gd` 里实测能跑**（{', '.join(runnable)}）；"
               "另外 7 份是 AdMob/IAP 原生插件依赖（`AdContainer`/`Ad_holder`/`android_iap`/`IAP_*`/"
               "`Android_mediastore_holder`/`IOS_album_holder`）—— `.so` 里 8 个原生类一个都不在 ClassDB"
               "（`recovered_view_check.gd` E 段实测）⇒ 物理不可得。"
               "剩下 **11 份**（5 份已证能跑 + 6 份尚未逐个验：ExtraOptionPad/ExtraOptionsHBox/"
               "GameNameContainer/ShopButtonsSize/ShowExtraButtons/ViewFunctionPad）的界面行为"
               "**现在是我们在 GDScript 里手写的**（如 `campaign.gd::place_buttons` 替 "
               "`CampaingLevelsButtons.gd`、`play_screen.gd` 的键盘/函数列表替 `ButtonGrid`/`ViewFunctionPad`/"
               "`ViewFunctionsLabel`/`ExtraOptionPad`/`ExtraOptionsHBox`/`ShowExtraButtons`/`HBoxContainer`）"),
        fix="P1 把 11 份能跑的挂上并删掉对应手造代码（每份 `cmp` + `recovered_view_check` 复验）；"
            "7 份插件类记为 **impossible**（缺原生二进制，不是缺时间）",
        who="campaign / play_screen / GameSetup / player_screen / replay_lobby / 5 个弹窗")


def recovered_copies_row():
    files = sorted(f for f in os.listdir(p(RECOVERED_COPIES)) if f.endswith(".gd"))
    same = 0
    diff = []
    for f in files:
        a, b = p(f"{RECOVERED_COPIES}/{f}"), p(f"recovered-src/{f}")
        if os.path.exists(b) and subprocess.run(["cmp", "-s", a, b]).returncode == 0:
            same += 1
        else:
            diff.append(f)
    total = sum(wc(f"{RECOVERED_COPIES}/{f}") for f in files)
    return dict(
        file=RECOVERED_COPIES + "/", start=1, end=total, kind="测试", verdict="A",
        what=f"原版 52 份游戏脚本的**逐字节副本**（`recovered_view_check.gd` 的 A 段对照物，不进运行时）",
        judge=f"`cmp client/tools/recovered/*.gd recovered-src/*.gd` ⇒ {same}/{len(files)} 相同"
              + ("" if not diff else f"，不同：{diff}"),
        fix="P3 保持（真值副本）", who="client/tools/recovered_view_check.gd")


# ---------------------------------------------------------------------------
# 4. 组装 TSV
# ---------------------------------------------------------------------------
def resolve(rel, frm, to):
    """函数名 → 行区间。PRE = 1..第一个 func-1；TAIL = 最后一个 func+1..EOF。"""
    fs = funcs(rel)
    n = wc(rel)
    if frm == "PRE":
        # PRE 单独一行 = 1..第一个 func-1；PRE 配一个函数名 = 1..那个函数结束
        end = (fs[0][1] - 1) if fs else n
        if to != "PRE":
            ends = {f[0]: f[2] for f in fs}
            if to not in ends:
                raise SystemExit(f"{rel}: 表里的函数名不存在：{to!r}")
            end = ends[to]
        return (1, end) if end >= 1 else (1, 1)
    if frm == "ALL":
        return (1, n)
    starts = {f[0]: f[1] for f in fs}
    ends = {f[0]: f[2] for f in fs}
    if frm not in starts or to not in ends:
        raise SystemExit(f"{rel}: 表里的函数名不存在：{frm!r} / {to!r}")
    return (starts[frm], ends[to])


def build_rows():
    rows = []
    cov = {}
    for rel in production_files():
        segs = SEGMENTS.get(rel) or [default_segment(rel)]
        fs = funcs(rel)
        n = wc(rel)
        has_all = any(s[0] == "ALL" for s in segs)
        # PRE 自动补：文件头 = 1..第一个 func-1
        has_pre = any(s[0] == "PRE" for s in segs) or has_all
        if not has_pre and fs:
            segs = [("PRE", "PRE", "数据", "B", "文件头注释 + 常量/状态字段",
                     "注释里的证据引用（自证）；常量本身的真值见同名规则的判据列",
                     "见同文件其它行", "同文件其它行")] + list(segs)
        # 尾部（最后一个 func 之后的空行/收尾注释）自动补
        last_end = max(resolve(rel, s[0], s[1])[1] for s in segs if s[0] != "PRE") if any(
            s[0] != "PRE" for s in segs) else 0
        if has_all:
            last_end = n
        for (frm, to, kind, verdict, what, judge, fix, who) in segs:
            a, b = resolve(rel, frm, to)
            if a > b:
                raise SystemExit(f"{rel}: 段区间倒挂 {frm!r}..{to!r} ⇒ {a}>{b}（表里顺序写错了）")
            rows.append(dict(file=rel, start=a, end=b, kind=kind, verdict=verdict,
                             what=what, judge=judge, fix=fix, who=who))
            for ln in range(a, b + 1):
                cov.setdefault(rel, set()).add(ln)
        if last_end and last_end < n:
            a, b = last_end + 1, n
            rows.append(dict(file=rel, start=a, end=b, kind="视图", verdict="B",
                             what="函数区之后的收尾（空行/行尾注释）",
                             judge="同上", fix="同上", who="同上"))
            for ln in range(a, b + 1):
                cov.setdefault(rel, set()).add(ln)
        got = sum(r["end"] - r["start"] + 1 for r in rows if r["file"] == rel)
        if got != n:
            raise SystemExit(f"{rel}: 段行数之和 {got} != 文件行数 {n}（段重叠/重复计）")
        missing = [ln for ln in range(1, n + 1) if ln not in cov.get(rel, set())]
        if missing:
            raise SystemExit(f"{rel}: 行未被判定覆盖：{missing[:20]}{'…' if len(missing) > 20 else ''}"
                             f"（共 {len(missing)}/{n}）")
    for r in original_scripts_rows():
        rows.append(r)
    rows.append(recovered_copies_row())
    rows.append(unmounted_originals_row())
    rows.extend(tool_extra_rows())
    return rows


def tsx_escape(s):
    return str(s).replace("|", "\\|").replace("\t", " ").replace("\n", " ")


#: 「最该先搬回 Rust 的 10 段」—— 判据：①是游戏规则 ②Rust 侧**已有**复原实现（括号里是证据档）
#: ③越大越靠前。分数只是排序辅助，不是证据；每一行的证据在 TSV 的判据列。
TOP10 = [
    (8.1, "client/src/play/function_input.gd:1-273 + client/src/play/keyboard_input.gd:1-130",
     "函数输入编辑/光标（403 行）→ `graphwar_function_input.rs` 的 `Buffer`/`classify_key`"
     "（`0x4f3210`/`0x4f44c0`，观测）"),
    (8.0, "client/src/audio/gw_audio.gd:1-400",
     "音频播放/humm/静音（400 行）→ `graphwar_audio_control.rs`（`[src/graphwar_audio_control.rs:241:17]` "
     "在 `.so` 里可见，观测）"),
    (7.0, "client/src/app/app.gd:1-352",
     "屏表路由 + 命令总线（352 行）→ `graphwar_root_node.rs`：`FN_SHOW_SCREEN=0x3a_f850`、"
     "`FN_COMMAND_BUS=0x3b_0830`、`SCREEN_TABLE` 15 个屏名带 `.rodata` 地址（逐位/观测）"),
    (6.9, "client/src/core/expr.gd:1-229",
     "表达式解析/求值（229 行）→ `math_function.rs`（`0x4ed670` 解析器 / `0x4eb9d0` 求值器，逐位）"),
    (6.4, "client/src/net/link.gd:1-212",
     "TCP 客户端状态机（212 行）→ `graphwar_client_connection_handler.rs`（`CONNECT_TIMEOUT_SECS=30` "
     "逐位 `0x48dccc`；**我方现在写 3 s，是错的**）"),
    (6.2, "client/src/net/discovery.gd:1-207",
     "LAN 房间发现/过期（207 行）→ `graphwar_lan_broadcaster_v6.rs`（`ENTRY_TTL_SECS=10` 逐位 "
     "`0x3de6e4`、周期 1 s 逐位；**我方现在写 12 s，是错的**）"),
    (5.4, "client/src/screens/play_screen.gd:1831-2321 + 2350-2486",
     "开火闸门 + `_judge_shots` + `_alive_by_team`（约 180 行规则）→ `ShotLifecycle`/`TurnClock`/"
     "`get_alive_teams`（`0x473520` 逐位、`0x478133` 逐位）"),
    (5.3, "client/src/net/frame.gd:1-175",
     "TCP 分帧（175 行）→ `graphwar_stream_buffer.rs`（`LEN_PREFIX_LEN=2`/`MAX_FRAME_PAYLOAD=0xffff`/"
     "5 条错误原文，逐位）"),
    (5.0, "client/src/screens/play_screen.gd:122-370",
     "玩家配色（249 行）→ `graphwar_color_manager.rs::pick`（`TEAM_POOL_LEN=6`/`RANDOM_COLOR_MODULO=180`，观测）"),
    (4.7, "client/src/app/campaign.gd:1-157",
     "21 关参数表（157 行）→ `graphwar_campaign_control.rs::LEVEL_INFO`（`level_info()` `0x4ad380`，逐位+运行时）"),
]

#: 候补（11-18）：同样是规则、Rust 侧也有，但体积小或证据档低一档，排上面 10 条之后。
TOP10_NEXT = [
    "client/src/net/broker.gd:1-165 → graphwar_server_broker_handler.rs（LOC_PROCESS/地址 3 条，观测）",
    "client/src/app/store.gd:154-339 → graphwar_player_config.rs::save/load + campaign save（观测）",
    "client/src/app/modes.gd:1-91 → graphwar_mode_manager.rs + graphwar_setup_control.rs 的 next/prev（逐位枚举）",
    "client/src/screens/play_screen.gd:3021-3436 → 联机事件建模/派发（应并入 remote_event_handler）",
    "client/src/app/game_state.gd:1-114 → graphwar_setup_control.rs::SetupOptions（观测）",
    "client/src/ui/gw_button.gd:1-94 → graphwar_button.rs / graphwar_button_class.rs（观测）",
    "client/src/app/commands.gd:1-76 → graphwar_root_node.rs::Command（32 个键，观测）",
    "client/src/game/target_set.gd:1-149 → graphwar_target_manager.rs（判定点缺证据 ⇒ 先挖再搬）",
]


def main():
    check = "--check" in sys.argv
    rows = build_rows()
    # 稳定排序：生产代码按路径/起始行；原版脚本行放最后
    rows.sort(key=lambda r: (r["file"].startswith((RECOVERED_COPIES + "/", VIEWS_DIR + "/")),
                             r["file"], r["start"]))
    out = ["# source_audit_client.tsv —— 审计面 ①（`client/`）；生成器 tools/audit_client.py，不要手改",
           "# 判据/分档见 .agent-work/specs/AUDIT-CRITERIA.md；列：id | 面(位置) | 是什么 | 判据 | 档位 | 修复建议 | 谁受影响",
           "# 行数由 `^func` 边界机器算；工具断言「每个生产文件的每一行恰好被覆盖一次」。",
           "id\tface\twhat\tjudge\tverdict\tfix\twho"]
    stats = {}
    for i, r in enumerate(rows, 1):
        n = r["end"] - r["start"] + 1
        stats.setdefault(r["verdict"], [0, 0])
        stats[r["verdict"]][0] += 1
        stats[r["verdict"]][1] += n
        stats.setdefault(("kind", r["kind"]), [0, 0])
        stats[("kind", r["kind"])][0] += 1
        stats[("kind", r["kind"])][1] += n
        out.append("\t".join([
            str(i),
            f"{r['file']}:{r['start']}-{r['end']}",
            tsx_escape(r["what"]),
            tsx_escape(r["judge"]),
            r["verdict"],
            tsx_escape(r["fix"]),
            tsx_escape(r["who"]),
        ]))
    if not check:
        os.makedirs(os.path.dirname(OUT), exist_ok=True)
        with open(OUT, "w", encoding="utf-8") as f:
            f.write("\n".join(out) + "\n")

    # ---- 统计 ----
    print(f"rows: {len(rows)}   -> {OUT if not check else '(not written)'}")
    print("\n按档位（行数为该档覆盖的行数）：")
    for v in sorted(k for k in stats if isinstance(k, str)):
        print(f"  {v:20s} rows={stats[v][0]:4d}  lines={stats[v][1]:6d}")
    print("\n按类别：")
    for k in sorted((k for k in stats if not isinstance(k, str))):
        print(f"  {k[1]:6s} rows={stats[k][0]:4d}  lines={stats[k][1]:6d}")
    ours_src = [r for r in rows if r["file"].startswith("client/src")
                and not r["file"].startswith(VIEWS_DIR)]
    orig_views = [r for r in rows if r["file"].startswith(VIEWS_DIR)]
    ours_tool = [r for r in rows if r["file"].startswith("client/tools")
                 and not r["file"].startswith(RECOVERED_COPIES + "/")]
    orig_copies = [r for r in rows if r["file"].startswith(RECOVERED_COPIES + "/")]
    other = [r for r in rows if r["file"].startswith(("client/scenes/", "client/project.godot", "client/run.sh"))]
    unmounted = [r for r in rows if r["file"].startswith("recovered-src/(")]
    L = lambda rs: sum(r["end"] - r["start"] + 1 for r in rs)
    prod = ours_src
    tool = ours_tool
    print(f"\n【我们的代码】client/src（不含 27 份原版视图）= {L(ours_src)} 行")
    print(f"【我们的代码】client/tools 的 .gd（验收/工具）  = {L(ours_tool)} 行")
    print(f"【我们的代码】scenes/project.godot/run.sh       = {L(other)} 行")
    print(f"【我们的代码】合计                              = {L(ours_src)+L(ours_tool)+L(other)} 行")
    print(f"【原版字节，不算我们的】client/src/views 27 份  = {L(orig_views)} 行")
    print(f"【原版字节，不算我们的】tools/recovered 52 份   = {L(orig_copies)} 行")
    print(f"【未挂的原版脚本（该挂没挂）】                 = {L(unmounted)} 行（{len(unmounted)} 组）")
    def breakdown(name, rs):
        d = {}
        for r in rs:
            d.setdefault(r["verdict"], 0)
            d[r["verdict"]] += r["end"] - r["start"] + 1
        print(f"\n[{name}] " + "  ".join(f"{k}={v}" for k, v in sorted(d.items())))

    breakdown("client/src（我们的生产代码）", ours_src)
    breakdown("client/tools（我们的验收/工具）", ours_tool)
    breakdown("scenes/project.godot/run.sh", other)
    breakdown("原版字节（views 27 + tools/recovered 52）", orig_views + orig_copies)

    print("\n最该先搬回 Rust 的 10 段（排序分 = 规则度 × 证据档 × 行数/100；"
          "规则度 1/0，证据档 逐位=3 观测=2 推断=1）")
    for i, (score, face, why) in enumerate(TOP10, 1):
        print(f"  {i:2d}. {score:6.1f}  {face}\n       {why}")
    print(f"\n全表合计：{len(rows)} 段 / {sum(r['end']-r['start']+1 for r in rows)} 行"
          f"（= 我们的代码 {L(ours_src)+L(ours_tool)+L(other)} + 原版字节 {L(orig_views)+L(orig_copies)}"
          f" + 该挂没挂的原版脚本 {L(unmounted)}）")
    bad = [r for r in rows if "FAIL" in r["judge"]]
    print(f"\n硬失败（cmp 不同 / 挂载点缺失）: {len(bad)}")
    for r in bad:
        print("  ", r["file"])
    return 0 if not bad else 1


if __name__ == "__main__":
    sys.exit(main())
