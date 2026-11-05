extends HBoxContainer


func _ready():
	get_tree().root.size_changed.connect(_on_viewport_size_changed)
	update_sizes()


func _on_viewport_size_changed():
	update_sizes()

func update_sizes():
	var screen_size = get_viewport_rect().size

	var font_size = screen_size.x / 30

	for node in get_children():

		node.add_theme_font_size_override("font_size", font_size)
