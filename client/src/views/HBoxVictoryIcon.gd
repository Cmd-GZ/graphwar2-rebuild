extends HBoxContainer

func _ready():
	get_tree().root.size_changed.connect(_on_viewport_size_changed)
	update_sizes()

func _on_viewport_size_changed():
	update_sizes()

func update_sizes():
	var screen_size = get_viewport_rect().size
	var margin = - screen_size.x / 10
	add_theme_constant_override("separation", margin)
