extends VBoxContainer



func _ready():
	get_tree().root.size_changed.connect(_on_viewport_size_changed)
	call_deferred("update_sizes")


func _on_viewport_size_changed():
	call_deferred("update_sizes")


func update_sizes():







	var safe_area = DisplayServer.get_display_safe_area()
	var minimum_size = (safe_area.size.x / 2 - 50) / 3



	set_visible(false)
	update_sizes_rec(self, minimum_size)
	call_deferred("set_visible", true)


func update_sizes_rec(node, minimum_size):
	for child_node in node.get_children():
		if child_node is Container == false:

			child_node.custom_minimum_size.x = minimum_size
			child_node.custom_minimum_size.y = minimum_size
		update_sizes_rec(child_node, minimum_size)
