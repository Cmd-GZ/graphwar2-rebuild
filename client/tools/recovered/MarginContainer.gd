extends MarginContainer

var _cached_safe_area: = Rect2i()


func _ready():












	get_tree().root.size_changed.connect(_on_viewport_size_changed)
	update_sizes()

func _process(_delta):
	var safe_area = DisplayServer.get_display_safe_area()
	if safe_area != _cached_safe_area:
		_cached_safe_area = safe_area
		update_sizes()

func _on_viewport_size_changed():
	update_sizes()

func update_sizes():






	var safe_area = DisplayServer.get_display_safe_area()
	var window_size = DisplayServer.window_get_size()

	if (window_size.x > safe_area.size.x || window_size.y > safe_area.size.y):
		add_theme_constant_override("margin_left", safe_area.position.x)
		add_theme_constant_override("margin_right", window_size.x - safe_area.end.x)
		add_theme_constant_override("margin_top", safe_area.position.y)
		add_theme_constant_override("margin_bottom", window_size.y - safe_area.end.y)
	else:
		remove_theme_constant_override("margin_left")
		remove_theme_constant_override("margin_right")
		remove_theme_constant_override("margin_top")
		remove_theme_constant_override("margin_bottom")
