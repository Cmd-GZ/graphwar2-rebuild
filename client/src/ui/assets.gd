extends RefCounted
class_name GwAssets
## 资产层。**仓库里没有任何媒体文件**（`.svg`/`.ttf`/`.png`/`.tres`/`.tscn`/`.gd`/`.pck`），
## 所有美术与字体都在运行时从使用者自己的 `Graphwar.pck` 解出来的目录读
## （`tools/extract_assets.py` → `client/assets/`，那份目录已 gitignore）。
##
## 为什么不能"没有资产就画个占位图"：那样启动看起来正常，但界面上的图标是假货，
## 用户根本不知道自己跑的不是原版的美术。所以本层**只**返回真资产；找不到就记下来、
## 返回 null、并且由 `main.gd` / `selftest.gd` 把它报出来（`docs/121` §5）。
##
## 查找规则（一个路径，多种容器格式）
## --------------------------------
## 解出来的文件名是**原版 `res://` 路径 + 实际容器扩展名**：
##
##     res://hats/no_hat.svg    → client/assets/hats/no_hat.ctex
##     res://ComicRelief-Bold.ttf → client/assets/ComicRelief-Bold.fontdata
##     res://button_font.tres   → client/assets/button_font.res
##
## 所以 `texture("hats/no_hat")` 会依次试 `ctex/png/webp/svg`：有 pck 的人命中 `.ctex`，
## 手上有完整工程（能拿到真 svg）的人把同名 `.svg` 放进去也能命中，代码不用改。

const DEFAULT_DIR := "res://assets"
const MANIFEST := "MANIFEST.tsv"
const SETTINGS := "original_project_settings.json"

## pck 解出来的是容器格式；后面几个是"有完整工程的人"可能直接放进去的原件。
const TEXTURE_EXTS := ["ctex", "png", "webp", "svg"]
const FONT_EXTS := ["fontdata", "ttf", "otf"]
const RESOURCE_EXTS := ["res", "tres"]

static var _texture_cache := {}
static var _font_cache := {}
static var _missing: Array[String] = []
static var _dir := ""

## 资产目录。`GW2_ASSET_DIR` 可覆盖（必须是 `res://` 路径），默认 `res://assets`。
static func dir() -> String:
	if _dir != "":
		return _dir
	var override := OS.get_environment("GW2_ASSET_DIR")
	_dir = override if override.begins_with("res://") else DEFAULT_DIR
	return _dir

## 资产目录是否就绪：以清单文件为准（解了一半的目录不算就绪）。
static func available() -> bool:
	return FileAccess.file_exists(dir().path_join(MANIFEST))

## 当前会话里找不到的资产（去重、保序）。空 = 全部命中。
static func missing() -> Array[String]:
	return _missing

static func _note_missing(what: String) -> void:
	if _missing.has(what):
		return
	_missing.append(what)
	push_error("[gw2] 缺少资产：%s（先跑 tools/extract_assets.py）" % what)

## 在资产目录里找一个资源：返回带扩展名的完整 `res://` 路径，找不到返回 ""。
static func find(rel: String, exts: PackedStringArray) -> String:
	var base := dir().path_join(rel)
	for ext in exts:
		var p := "%s.%s" % [base, ext]
		if FileAccess.file_exists(p):
			return p
	return ""

static func has(rel: String, exts: PackedStringArray = PackedStringArray(TEXTURE_EXTS)) -> bool:
	return find(rel, exts) != ""

static func texture(rel: String) -> Texture2D:
	if _texture_cache.has(rel):
		return _texture_cache[rel]
	var path := find(rel, PackedStringArray(TEXTURE_EXTS))
	if path == "":
		_note_missing(rel)
		return null
	var res := load(path)
	if not (res is Texture2D):
		_note_missing("%s（%s 不是 Texture2D）" % [rel, path])
		return null
	_texture_cache[rel] = res
	return res

static func font(rel: String) -> Font:
	if _font_cache.has(rel):
		return _font_cache[rel]
	var path := find(rel, PackedStringArray(FONT_EXTS + RESOURCE_EXTS))
	if path == "":
		_note_missing(rel)
		return null
	var res := load(path)
	if not (res is Font):
		_note_missing("%s（%s 不是 Font）" % [rel, path])
		return null
	_font_cache[rel] = res
	return res

## 原版工程设置（从包里的 `project.binary` 解出来）。没有资产目录时返回 {}。
static func original_settings() -> Dictionary:
	var p := dir().path_join(SETTINGS)
	if not FileAccess.file_exists(p):
		return {}
	var txt := FileAccess.get_file_as_string(p)
	var parsed = JSON.parse_string(txt)
	return parsed if parsed is Dictionary else {}

## 批量校验：返回**缺失**的 rel 列表（空 = 齐备）。用于启动前的硬检查。
static func check(rels: Array) -> Array[String]:
	var bad: Array[String] = []
	for rel in rels:
		var r := String(rel)
		if find(r, PackedStringArray(TEXTURE_EXTS)) == "" \
				and find(r, PackedStringArray(FONT_EXTS + RESOURCE_EXTS)) == "":
			bad.append(r)
	return bad
