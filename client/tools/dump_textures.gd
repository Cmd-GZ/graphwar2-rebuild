extends SceneTree
## 把 res://assets/**/*.ctex 解成 PNG（真值比对用；产物落在 out dir，不入库）。

func _init() -> void:
	var out := ""
	for a in OS.get_cmdline_user_args():
		if a.begins_with("--out="):
			out = a.substr(6)
	var dir := DirAccess.open("res://assets")
	var n := 0
	for sub in ["skins", "faces", "hats"]:
		var d := DirAccess.open("res://assets/" + sub)
		if d == null:
			continue
		DirAccess.make_dir_recursive_absolute(out + "/" + sub)
		for f in d.get_files():
			if not f.ends_with(".ctex"):
				continue
			var tex: Texture2D = load("res://assets/%s/%s" % [sub, f])
			if tex == null:
				print("@@FAIL %s/%s" % [sub, f])
				continue
			var img := tex.get_image()
			if img == null:
				print("@@FAIL img %s/%s" % [sub, f])
				continue
			img.save_png("%s/%s/%s.png" % [out, sub, f.get_basename()])
			n += 1
	print("@@DUMP done n=%d out=%s" % [n, out])
	quit(0)
