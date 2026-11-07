extends Label


func _ready():
	get_tree().root.size_changed.connect(_on_viewport_size_changed)
	update_sizes()

func _on_viewport_size_changed():
	update_sizes()

func update_sizes():
	var screen_size = get_viewport_rect().size
	var desired_size = Vector2(screen_size.x * 0.12, 0.0)


	for node in get_children():
		node.custom_minimum_size = desired_size
