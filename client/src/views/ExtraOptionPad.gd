extends MarginContainer

func _ready():
	var plane_container = find_parent("*PlayMainHBox").get_node("PlaneContainer")

	plane_container.item_rect_changed.connect(_on_size_changed)

func _on_size_changed():
	var plane_container = find_parent("*PlayMainHBox").get_node("PlaneContainer")
	var minimum_y = plane_container.size.x / 1.66667
	var extra_y = plane_container.size.y - minimum_y

	var left_top_button_container = find_parent("*PlayMainHBox").get_node("LeftControls/LeftTopButtonsContainer")
	var target_y = left_top_button_container.size.y

	if extra_y >= target_y:
		self.custom_minimum_size.y = target_y
		self.visible = true
	else:
		self.custom_minimum_size.y = 0
		self.visible = false
