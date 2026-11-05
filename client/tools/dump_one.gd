extends SceneTree
func _init() -> void:
	var out := ""
	var names: Array[String] = []
	for a in OS.get_cmdline_user_args():
		if a.begins_with("--out="):
			out = a.substr(6)
		elif a.begins_with("--name="):
			names.append(a.substr(7))
	for n in names:
		var tex: Texture2D = load("res://assets/%s.ctex" % n)
		if tex == null:
			print("@@MISS %s" % n); continue
		print("@@TEX %-22s %dx%d" % [n, int(tex.get_width()), int(tex.get_height())])
		if out != "":
			tex.get_image().save_png("%s/%s.png" % [out, n])
	quit(0)
