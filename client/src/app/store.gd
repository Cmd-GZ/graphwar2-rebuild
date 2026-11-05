extends RefCounted
class_name GwStore
## 本地存档：玩家档案 + 战役进度。
##
## 位置：`user://gw2_client.cfg`（Godot 的 user 目录；Linux 下通常是
## `$XDG_DATA_HOME/godot/app_userdata/<项目名>/`）。**不写游戏自己的存档目录**，
## 也不读它——原版存档格式未解，见 docs/108 §7。

const PATH := "user://gw2_client.cfg"

const SECTION_PROFILE := "profile"
const SECTION_CAMPAIGN := "campaign"

## 玩家的外观选项数量。观测依据：`docs/92` §4.14 —— Skins 15 个按钮、
## Faces 15 个槽、Hats 27 个槽（5×3 / 5×3 / 9×3，来自 `shop_button_entry.tscn` 实例数 42）。
const SKIN_COUNT := 15
const FACE_COUNT := 15
const HAT_COUNT := 27

## 原版玩家的**默认外观**（`docs/92` §4.14 / §4.10 观测）：yellow 皮肤 + regular_eyes + soldier_helmet。
## 这三个下标 = `GwIcons` 表里的位置，不是随手取的 0：
##   skin_7 = skins/yellow、face_2 = faces/regular_eyes、hat_1 = hats/soldier_helmet。
## （0 号分别是 blue / no_face / no_hat，其中后两个是**全透明**图，预览会变成一颗光头。）
const DEFAULT_SKIN := 7
const DEFAULT_FACE := 2
const DEFAULT_HAT := 1

var profile := {
	"name": "user",
	"skin": DEFAULT_SKIN,
	"face": DEFAULT_FACE,
	"hat": DEFAULT_HAT,
}

## 已通关的关卡 id 列表（战役解锁状态）。
var completed_levels: Array[int] = []
## 是否已"解锁皮肤"（内购模拟位；Linux 端不做内购）。
var skins_unlocked := false

func load_all() -> void:
	var cf := ConfigFile.new()
	if cf.load(PATH) != OK:
		return
	profile.name = String(cf.get_value(SECTION_PROFILE, "name", profile.name))
	profile.skin = clampi(int(cf.get_value(SECTION_PROFILE, "skin", DEFAULT_SKIN)), 0, SKIN_COUNT - 1)
	profile.face = clampi(int(cf.get_value(SECTION_PROFILE, "face", DEFAULT_FACE)), 0, FACE_COUNT - 1)
	profile.hat = clampi(int(cf.get_value(SECTION_PROFILE, "hat", DEFAULT_HAT)), 0, HAT_COUNT - 1)
	skins_unlocked = bool(cf.get_value(SECTION_PROFILE, "skins_unlocked", false))
	var done: Array = cf.get_value(SECTION_CAMPAIGN, "completed", [])
	completed_levels.clear()
	for v in done:
		completed_levels.append(int(v))

func save_all() -> void:
	var cf := ConfigFile.new()
	cf.set_value(SECTION_PROFILE, "name", profile.name)
	cf.set_value(SECTION_PROFILE, "skin", profile.skin)
	cf.set_value(SECTION_PROFILE, "face", profile.face)
	cf.set_value(SECTION_PROFILE, "hat", profile.hat)
	cf.set_value(SECTION_PROFILE, "skins_unlocked", skins_unlocked)
	cf.set_value(SECTION_CAMPAIGN, "completed", completed_levels)
	var err := cf.save(PATH)
	if err != OK:
		push_warning("[gw2] 存档写入失败：%s" % PATH)

func mark_completed(level_index: int) -> void:
	if not completed_levels.has(level_index):
		completed_levels.append(level_index)
		save_all()

func is_completed(level_index: int) -> bool:
	return completed_levels.has(level_index)
