extends RefCounted
class_name GwReplayIndex
## 回放列表的数据来源。
##
## **已观测**（`docs/92` §4.15、`docs/78`）：ReplayLobby 每个条目对应回放目录里的一个文件，
## 条目文本形如 `2026-11-05 12:48:11`（= 录制时间），另有 `Graphwar Game`（房间名）与
## 四个模式图标。原版把回放存在游戏自己的目录里。
##
## **未知**：回放文件的内部格式细节与我们的解析器覆盖面（`docs/78` 有格式，但本项目
## 不读真实回放，见 docs/108 §7）。因此这里只做**目录枚举**：
##   * 扫 `user://replays/`（本项目自己的目录），不碰原版存档目录；
##   * 每个文件一条，标题 = 文件名（去掉扩展名），副标题 = 文件大小 + 修改时间；
##   * 解析不了的字段（房间名、四个模式）留空 → 界面显示 `--`，不编造。

const DIR := "user://replays"
const EXTENSIONS := ["replay", "gw2r", "json", "dat"]

## 返回 [{title, game_name, size, mtime, modes_known, path}]
static func scan() -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	var dir := DirAccess.open(DIR)
	if dir == null:
		return out
	var names := dir.get_files()
	names.sort()
	for f in names:
		var ext := f.get_extension().to_lower()
		if not EXTENSIONS.has(ext):
			continue
		var path := DIR.path_join(f)
		var size := 0
		var mtime := 0
		if FileAccess.file_exists(path):
			size = int(FileAccess.get_file_as_bytes(path).size())
			mtime = int(FileAccess.get_modified_time(path))
		out.append({
			"title": f.get_basename(),
			"game_name": "--",
			# modes_known = false：本重建不解析回放头，模式图标一律按默认值画（不假装知道）。
			"modes_known": false,
			"size": size,
			"mtime": mtime,
			"path": path,
		})
	return out


static func ensure_dir() -> void:
	if not DirAccess.dir_exists_absolute(DIR):
		DirAccess.make_dir_recursive_absolute(DIR)
