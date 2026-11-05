extends RefCounted
class_name GwIcons
## 图标名 → **原版资源路径**的映射表，以及加载入口。
##
## 这里**不画任何东西**：所有图都是 `GwAssets` 从使用者自己的 `Graphwar.pck` 解出来的
## 真资源（`docs/121` §5）。找不到就返回 null 并记进 `GwAssets.missing()` —— 不做占位图。
##
## 映射的证据来源（每条都能核对）
## ------------------------------
## 1. **原版 `.tscn` 的 `icon=` / `texture=` 引用**（31 个场景，99 处节点→svg）
##    —— 例如 `play_screen.tscn` 的 `GraphwarButton3.icon = share_summary.svg`。
## 2. **原版 `.gd` 里的动态 `load()`** —— `ShowExtraButtons.gd` 在展开/收起时切
##    `show_extra_buttons.svg` / `hide_extra_buttons.svg`。
## 3. **游戏二进制 `libgraphwarrust.so` 的 `.rodata` 字面量池**（文件偏移 0x1c2f97 起）
##    —— 界面逻辑在 Rust 里，所以"第 N 档用哪张图"只能从二进制读：
##    `res://skins/bot_0.svg` … `bot_5`、`turn_30s…turn_inf`、`axis_option_0…3`、
##    `ranks/rank_0…21`、`skins/*`、`faces/*`、`hats/*` 都是一段连续字面量。
##    组内顺序 = 索引顺序：`ranks/rank_0…rank_21` 严格递增可以反证这一点；
##    其余三组沿用同一约定（**推断**，见 `docs/121` §4.3 写明缺什么证据）。
##
## 名字沿用原版文件名（不含目录、不含扩展名），与 `docs/92` 的台账对得上。

## 固定映射：界面里的名字 → 原版 `res://` 路径（不含扩展名）。注释是证据出处。
const FIXED := {
	# --- 返回 / 退出（每个界面都有 ExitButtonContainer）---
	"return": "return",                              # 31 个 .tscn 的 ClipButton/LongPressButton
	# --- 聊天 ---
	"no_chat": "no_chat",                            # GameSetup ChatButtonSetup / play_screen ChatButtonPlay
	"view_chat": "view_chat",                        # libgraphwarrust.so .rodata
	"new_chat": "no_chat_new_chat",                  # libgraphwarrust.so .rodata
	# --- 锁 ---
	"unlock": "unlock",                              # GameSetup.tscn LockButton 初值
	"lock": "lock",                                  # libgraphwarrust.so .rodata（LockButton 切换）
	# --- 玩家行 ---
	"x": "x",                                        # team1_entry / team2_entry RemovePlayerButton
	"plus": "plus",                                  # team*_entry AddSoldierButton
	"minus": "minus",                                # team*_entry RemoveSoldierButton
	"arrow_left": "arrow_left",                      # team2_entry ChangeTeamButton
	"arrow_right": "arrow_right",                    # team1_entry ChangeTeamButton
	"empty_soldier": "empty_soldier",                # team*_entry EmptyIcon
	"empty_set": "empty_set",                        # libgraphwarrust.so .rodata
	# --- 函数输入键盘 ---
	"arrow_left_bar": "arrow_left_bar",              # play_screen GraphwarButton16
	"arrow_right_bar": "arrow_right_bar",            # play_screen GraphwarButton18
	"delete_left": "delete_left",                    # play_screen GraphwarButton18
	"delete_right": "delete_right",                  # play_screen GraphwarButton16
	# --- 对局附加按钮 ---
	"show_extra_buttons": "show_extra_buttons",      # ShowExtraButtons.gd（收起态）
	"hide_extra_buttons": "hide_extra_buttons",      # ShowExtraButtons.gd（展开态）
	"view_names": "view_names",                      # play_screen ShowNames
	"hide_names": "hide_names",                      # libgraphwarrust.so .rodata（同名按钮的切换态）
	"view_functions": "view_recent_functions",       # play_screen ViewFunctionButton 的初值
	"view_single_function": "view_single_function",  # pck 里有这张图（.so/.tscn 无引用点，docs/121 §4.4）
	"view_recent_functions": "view_recent_functions",# play_screen ViewFunctionButton 的初态图标
	"view_all_functions": "view_all_functions",      # libgraphwarrust.so .rodata
	"mute": "mute",                                  # libgraphwarrust.so .rodata（MuteButton 切换）
	"unmute": "unmute",                              # play_screen MuteButton 初值
	# --- 回放 ---
	"play_option": "play_option",                    # replay_entry PlayReplayButton / play_screen PlayPauseButton
	"play_pause": "pause_option",                    # libgraphwarrust.so .rodata（play_option 的配对态）
	"prev": "skip_backward_option",                  # play_screen GraphwarButton
	"next": "skip_forward_option",                   # play_screen GraphwarButton3
	"save_screenshot": "share_screenshot",           # play_screen GraphwarButton
	"save_summary": "share_summary",                 # play_screen GraphwarButton3
	"save_clip": "share_video",                      # play_screen GraphwarButton4
	# --- 对局设置 ---
	"add_bot_option": "add_bot_option",              # GameSetup.tscn AddBotButton
	"fx_option": "fx_option",                        # GameSetup FunctionModeButton / play_screen FxTextureButton
	"dx_dy_option": "dx_dy_option",                  # libgraphwarrust.so .rodata
	"turns_sequential": "turns_sequential",          # libgraphwarrust.so .rodata
	"turns_simultaneous": "turns_simultaneous",      # GameSetup TurnModeButton
	# --- 时限（六档，顺序同二进制字面量池）---
	"turn_30s": "turn_30s",
	"turn_1m": "turn_1m",
	"turn_2m": "turn_2m",
	"turn_3m": "turn_3m",
	"turn_5m": "turn_5m",
	"turn_inf": "turn_inf",
}

## `axis_option_0..3`
const AXIS_OPTIONS := ["axis_option_0", "axis_option_1", "axis_option_2", "axis_option_3"]

## 皮肤：`libgraphwarrust.so` .rodata 顺序（23 条，后 6 条是 Bot 专用）。
## 界面格子数按 `docs/92` §4.14 的观测（15），取前 15 —— 见 docs/121 §4.3 的"推断"说明。
const SKINS := [
	"skins/blue", "skins/gray", "skins/pink", "skins/purple", "skins/red",
	"skins/target", "skins/white_skin", "skins/yellow", "skins/green",
	"skins/light_green", "skins/true_purple", "skins/brown", "skins/calico",
	"skins/wargod", "skins/arrow",
	# 原版存在但 15 格放不下的两个（`face_paint_1/2`）。保留下标可达，不进网格。
	"skins/face_paint_1", "skins/face_paint_2",
]

## Bot 头像：`add_bot_pop_up.tscn` 里 6 个 AddBotButton 的 icon 就是 `skins/bot_N.svg`。
const BOT_SKINS := [
	"skins/bot_0", "skins/bot_1", "skins/bot_2",
	"skins/bot_3", "skins/bot_4", "skins/bot_5",
]

## 脸：`libgraphwarrust.so` .rodata 顺序，去掉 `bot_0`（Bot 专用）与 `reference`（设计参照）。
const FACES := [
	"faces/no_face", "faces/anime_eyes", "faces/regular_eyes", "faces/blue_eyes",
	"faces/green_eyes", "faces/purple_eyes", "faces/aviator_glare", "faces/cute_eyes",
	"faces/nerd_glasses", "faces/mustache", "faces/baggy_eyes", "faces/asian",
	"faces/cat_eyes", "faces/eye_patch", "faces/baby_eyes", "faces/alien_eyes",
]

## 帽子：`libgraphwarrust.so` .rodata 顺序（28 条）。
const HATS := [
	"hats/no_hat", "hats/soldier_helmet", "hats/blonde_boy", "hats/blonde_girl",
	"hats/brunette_girl", "hats/brown_boy", "hats/anime_hair", "hats/mohawk",
	"hats/mohawk_red", "hats/cowboy", "hats/straw_hat", "hats/powdered_wig",
	"hats/ponytail", "hats/witch_hat", "hats/hood", "hats/horned_helmet",
	"hats/hair_band", "hats/cat_ears", "hats/unicorn", "hats/roman_helmet",
	"hats/chef_hat", "hats/top_hat", "hats/tricone", "hats/asian_hat",
	"hats/mad_hair", "hats/baseball_cap", "hats/crown", "hats/antenna_0",
]

## 原版默认外观（`docs/92` §4.10、§4.14 观测）：
## 玩家 = yellow 皮肤 + regular_eyes + soldier_helmet；Bot = bot_N + no_face + no_hat。
const DEFAULT_SKIN := "skins/yellow"
const DEFAULT_FACE := "faces/regular_eyes"
const DEFAULT_HAT := "hats/soldier_helmet"
const BOT_FACE := "faces/no_face"
const BOT_HAT := "hats/no_hat"


## 界面用的名字 → 原版 `res://` 路径（不含扩展名）。查不到返回 ""。
static func path_for(name: String) -> String:
	if FIXED.has(name):
		return String(FIXED[name])
	if name.begins_with("axis_option_"):
		return AXIS_OPTIONS[wrapi(int(name.substr(12)), 0, AXIS_OPTIONS.size())]
	if name.begins_with("bot_"):
		return BOT_SKINS[wrapi(int(name.substr(4)), 0, BOT_SKINS.size())]
	if name.begins_with("skin_"):
		return SKINS[wrapi(int(name.substr(5)), 0, SKINS.size())]
	if name.begins_with("face_"):
		return FACES[wrapi(int(name.substr(5)), 0, FACES.size())]
	if name.begins_with("hat_"):
		return HATS[wrapi(int(name.substr(4)), 0, HATS.size())]
	if name.begins_with("rank_"):
		return "ranks/rank_%d" % int(name.substr(5))
	if name == "basic_soldier":
		return "basic_soldier"
	return ""


## 取图标纹理。**没有占位图**：缺了就返回 null 并进 `GwAssets.missing()`。
static func get_icon(name: String) -> Texture2D:
	var rel := path_for(name)
	if rel == "":
		push_error("[gw2] 未知图标名 %s（GwIcons.FIXED 里没有，也不是 *_N 形式）" % name)
		return null
	return GwAssets.texture(rel)


## 启动前自检用：本客户端会引用到的**全部**图标路径（不含扩展名）。
## 任何一个缺失都说明资产目录不完整 —— 见 `main.gd` 的硬检查。
static func all_referenced_paths() -> Array[String]:
	var out: Array[String] = []
	for k in FIXED:
		out.append(String(FIXED[k]))
	out.append_array(PackedStringArray(AXIS_OPTIONS))
	out.append_array(PackedStringArray(SKINS))
	out.append_array(PackedStringArray(BOT_SKINS))
	out.append_array(PackedStringArray(FACES))
	out.append_array(PackedStringArray(HATS))
	out.append("ranks/rank_0")
	out.append("basic_soldier")
	return out
