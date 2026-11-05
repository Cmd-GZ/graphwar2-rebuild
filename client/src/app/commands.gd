extends RefCounted
class_name GwCommands
## 命令键清单 —— **这是从恢复出的 `.tscn` 里观测到的 Rust 侧契约**。
##
## 每个 `GraphwarButton.input_text` 就是 Rust `GraphwarRootNode` 的一个命令键；
## 原版的点击处理、切屏、加 Bot、开火全在 Rust 里（`docs/92` §2.3、`docs/104` §6）。
## 这份清单是把 31 个 `.tscn` 里的 `input_text` / `long_press_input_text` 全量提取后
## 去重得到的（提取脚本见 docs/108 §9），本项目的 `App.command()` 按它分发。
##
## 标记：✅ = 本项目已实现行为；🔶 = 已接线但行为是占位（明写）；❌ = 键存在但我们不做。

# ---- 界面切换 / 返回 ------------------------------------------------------
const CAMPAIGN := "campaign"                     # ✅ MainMenu → Campaign
const PLAYER_SETTINGS := "player_settings"       # ✅ MainMenu → PlayerScreen
const MULTIPLAYER := "multiplayer"               # ✅ MainMenu → LanLobby
const REPLAY := "replay"                         # ✅ MainMenu → ReplayLobby
const EXIT_CAMPAIGN := "exit_campaign"           # ✅ → MainMenu
const EXIT_LAN_LOBBY := "exit_lan_lobby"         # ✅ → MainMenu
const EXIT_GAME_SETUP := "exit_game_setup"       # ✅ → MainMenu（同时销毁本地房间）
const EXIT_PLAYER_SETTINGS := "exit_player_settings" # ✅ → MainMenu
const EXIT_REPLAY_LOBBY := "exit_replay_lobby"   # ✅ → MainMenu

# ---- 联机大厅 ------------------------------------------------------------
const SHOW_LOCAL := "show_local"                 # ✅ 房间列表 = 本地房间
const SHOW_GLOBAL := "show_global"               # 🔶 公网列表：来源未知（docs/92 §8），用静态占位
const CREATE_GAME := "create_game"               # ✅ → GameSetup
const JOIN_IP := "join_ip"                       # ✅ → JoinIpPopUp
const JOIN_JOIN_IP := "join_join_ip"             # 🔶 校验 host:port → ConnectingPopUp → Disconnected（无真实网络层）
const CANCEL_JOIN_IP := "cancel_join_ip"         # ✅
const CANCEL_CONNECTING := "cancel_connecting"   # ✅
const OK_DISCONNECTED := "ok_disconnected"       # ✅

# ---- 对局设置 ------------------------------------------------------------
const LOCK_GAME := "lock_game"                   # ✅ 图标切换（锁/解锁）
const ADD_BOT := "add_bot"                       # ✅ → AddBotPopUp
const CANCEL_ADD_BOT := "cancel_add_bot"         # ✅
const START_GAME := "start_game"                 # ✅ 无 Bot 时按原版行为**静默无操作**（docs/22 §5.1）
const TOGGLE_CHAT := "toggle_chat"               # ✅ → ChatPopUp / 关闭

# ---- 关卡 ---------------------------------------------------------------
const START_LEVEL := "start_level"               # ✅ → PlayScreen（campaign 模式）
const CANCEL_LEVEL := "cancel_level"             # ✅

# ---- 对局中 --------------------------------------------------------------
const ASK_EXIT_PLAY := "ask_exit_play"           # ✅ → ConfirmPlayExitPopUp
const CANCEL_EXIT_PLAY := "cancel_exit_play"     # ✅
const EXIT_PLAY := "exit_play"                   # ✅ → MainMenu（结算框的返回也是它）
const TOGGLE_NAMES := "toggle_names"             # ✅ 士兵名字显示开关
const TOGGLE_VIEW_FUNCTIONS := "toggle_view_functions" # ✅ 顶部"各玩家函数"列表开关
const TOGGLE_MUTE := "toggle_mute"               # 🔶 只切图标（本项目没有音频资源）
const FIRE := "fire"                             # ✅ 求值 + 画弹道
const PLAY_REPLAY := "play_replay"               # 🔶 进回放模式，回放数据未解（docs/78）
const PLAY_PAUSE := "play_pause"                 # 🔶
const SAVE_SCREENSHOT := "save_screenshot"       # 🔶 键名已观测；原版语义（存哪、什么格式）未知
const SAVE_SUMMARY := "save_summary"             # 🔶 同上
const SAVE_CLIP := "save_clip"                   # 🔶 同上
const UNLOCK_SKINS := "unlock_skins"             # 🔶 内购：Linux 端没有（用户明确要求不做）

# ---- 键盘（函数输入）-----------------------------------------------------
## 左键盘/右键盘每个键的 `input_text` 直接被 Rust 追加/编辑函数串。
## 键位表见 `docs/92` §4.10，本项目 `src/play/function_input.gd` 实现同一套编辑语义。
const KEY_CLEAR := "C"
const KEY_BACKSPACE := "⌫"      # 左删
const KEY_DELETE := "⌦"         # 右删
const KEY_LEFT := "←"
const KEY_RIGHT := "→"
const KEY_HOME := "⇤"
const KEY_END := "⇥"
const KEY_PI := "π"
const KEY_PREV := "⏮︎"
const KEY_NEXT := "⏭"

## 关卡按钮的命令键是 `level_<n>`（`CampaingLevelsButtons.gd` 里 `input_text = "level_" + str(i)`）。
const LEVEL_PREFIX := "level_"
## Bot 难度档的命令键是 `add_bot_<0..5>`（恢复出的 add_bot_pop_up.tscn）。
const ADD_BOT_PREFIX := "add_bot_"
