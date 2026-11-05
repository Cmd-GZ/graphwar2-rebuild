extends TabContainer



func _ready():
	set_tab_title(0, "   Skins   ")
	set_tab_title(1, "   Faces  ")
	set_tab_title(2, "   Hats    ")

	get_tree().root.size_changed.connect(_on_viewport_size_changed)
	update_sizes()

func _on_viewport_size_changed():
	update_sizes()

func update_sizes():
	var safe_area = DisplayServer.get_display_safe_area()
	var font_size = safe_area.size.x / 30

	add_theme_font_size_override("font_size", font_size)
