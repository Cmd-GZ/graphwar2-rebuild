extends MarginContainer



func _ready():
	var margin_value = 10
	add_theme_constant_override("margin_top", margin_value)
	add_theme_constant_override("margin_left", margin_value)
	add_theme_constant_override("margin_bottom", margin_value)
	add_theme_constant_override("margin_right", margin_value)

	get_tree().root.size_changed.connect(_on_viewport_size_changed)
	update_child_sizes()

func _on_viewport_size_changed():
	update_child_sizes()

func update_child_sizes():
	var screen_size = get_viewport_rect().size

	var font_size = screen_size.x / 40

	for node in get_children():

		node.add_theme_font_size_override("font_size", font_size)
