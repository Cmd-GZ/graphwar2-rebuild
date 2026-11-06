extends RefCounted
class_name GwWireSamples
## **真实抓包原文**（逐字节），当自检的黄金样本用。
##
## 每一条都能在仓库的夹具里查到（行号是 1-based，`fixtures/*.jsonl`）：
##   * `NEW_CONNECTION_REQUEST`  `fixtures/protocol_match.jsonl:2`
##   * `TICK_REQUEST`            `fixtures/protocol_match.jsonl:4`
##   * `GAME_START_REQUEST`      `fixtures/protocol_match.jsonl:18`
##   * `FUNCTION_UPDATE_REQUEST` `fixtures/protocol_match.jsonl:19`
##   * `FUNCTION_FIRE_REQUEST`   `fixtures/protocol_match.jsonl:21`
##   * `NEW_CONNECTION` … `TIME_MODE_INFO`（7 条状态同步）`protocol_match.jsonl:22/23/30/32/25/26/27`
##   * `BROADCAST_1`             `fixtures/protocol_match.jsonl:81`
##   * `VERSION_INFO`            `fixtures/protocol_match.jsonl:83`
##   * `GAME_INFO_1/2`           `fixtures/protocol_match.jsonl:84/85`
##
## ⚠️ 为什么要抄一份进 `client/`：`client/` 是**独立的 Godot 工程**（`res://` 根是
## `client/`），运行时读不到仓库根的 `fixtures/`；把黄金样本写死在客户端里，才能让
## `bash client/run.sh selftest` 在任何机器上**只依赖仓库里的字节**做逐字节对拍。
## 抄写时**一个空格都没动** —— 改动这里等于改真值，必须在 `docs/134` 里说明并回查夹具。

## 客户端握手第一帧（`docs/33` §1：发 `NewConnectionRequest` 之后服务端回 7 条）。
const NEW_CONNECTION_REQUEST := '{"NewConnectionRequest":{"major_version":"2.0","minor_version":"60"}}'
const TICK_REQUEST := '{"TickRequest":{}}'
const GAME_START_REQUEST := '{"GameStartRequest":{}}'
const FUNCTION_UPDATE_REQUEST := '{"FunctionUpdateRequest":{"player_id":229,"function":""}}'
const FUNCTION_FIRE_REQUEST := '{"FunctionFireRequest":{"player_id":232}}'

## 服务端在 `NewConnectionRequest` 之后回的 7 条，**顺序即抓包顺序**（`docs/33` §1）。
const STATE_SYNC: Array[String] = [
	'{"NewConnection":{"connection_id":228}}',
	'{"TurnModeInfo":{"turn_mode":"SequentialTurns"}}',
	'{"GameStateInfo":{"game_state":"Setup"}}',
	'{"GameNameInfo":{"game_name":"Graphwar Game"}}',
	'{"AxisModeInfo":{"axis_mode":"EveryUnit"}}',
	'{"FunctionModeInfo":{"function_mode":"NormalFunction"}}',
	'{"TimeModeInfo":{"time_mode":"Timer1m"}}',
]

## LAN 广播原文（`docs/24` §1.1、`docs/115` §2.2）：裸的 9 字段对象、
## **没有**事件外壳、**没有**长度前缀（LAN 通道是换行分帧，与 TCP 不同）。
const BROADCAST_1 := '{"room_name":"Graphwar Game","game_state":"Setup","current_player_count":1,"address":"[::1]:43717","axis_mode":"EveryUnit","function_mode":"NormalFunction","turn_mode":"SequentialTurns","time_mode":"Timer1m","locked":false}'

## broker 会话（`docs/115` §2.1/§2.2）：客户端先报 `VersionInfo`，broker 逐条推 `GameInfo`。
const VERSION_INFO := '{"VersionInfo":{"version":"2.0.60","system":"linux"}}'
const GAME_INFO_1 := '{"GameInfo":{"game_info":{"room_name":"US Room 2","game_state":"ExecutingResults","current_player_count":2,"address":"69.164.213.144:35981","axis_mode":"EveryUnit","function_mode":"NormalFunction","turn_mode":"SequentialTurns","time_mode":"Timer1m","locked":false}}}'
const GAME_INFO_2 := '{"GameInfo":{"game_info":{"room_name":"US Room 1","game_state":"Setup","current_player_count":0,"address":"69.164.213.144:37933","axis_mode":"EveryUnit","function_mode":"NormalFunction","turn_mode":"SequentialTurns","time_mode":"Timer1m","locked":false}}}'

# ---------------------------------------------------------------- 测试用构造器

## 按 `src/_scaffold/protocol.rs::Broadcast::to_json` 的**键序**拼一条 LAN 广播。
##
## 这不是复原结论，是给自检造"一台房主在广播"用的：线的形状（9 个键、键序、
## 无空格、枚举当字符串、`locked` 布尔）来自抓包；**参数**由测试给。
## 自检里有一条断言：用它拼出来的默认样本必须与 `BROADCAST_1` **逐字节相同** ——
## 这样"构造器"和"黄金样本"互相钉住，任何一边漂了都会红。
static func broadcast(room_name: String, game_state: String, player_count: int,
		address: String, axis_mode: String, function_mode: String,
		turn_mode: String, time_mode: String, locked: bool) -> String:
	return ('{"room_name":%s,"game_state":%s,"current_player_count":%d,"address":%s,'
		+ '"axis_mode":%s,"function_mode":%s,"turn_mode":%s,"time_mode":%s,"locked":%s}') % [
			GwNetFrame.json_string(room_name), GwNetFrame.json_string(game_state),
			player_count, GwNetFrame.json_string(address),
			GwNetFrame.json_string(axis_mode), GwNetFrame.json_string(function_mode),
			GwNetFrame.json_string(turn_mode), GwNetFrame.json_string(time_mode),
			"true" if locked else "false",
		]
