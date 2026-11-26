extends MarginContainer

func _ready():
	var plane_container = find_parent("*PlayMainHBox").get_node("PlaneContainer")

	plane_container.item_rect_changed.connect(_on_size_changed)

func _on_size_changed():
	update_sizes()

func update_sizes():
	var plane_container = find_parent("*PlayMainHBox").get_node("PlaneContainer")
	var minimum_y = plane_container.size.x / 1.66667
	var extra_y = plane_container.size.y - minimum_y

	var left_top_button_container = find_parent("*PlayMainHBox").get_node("LeftControls/LeftTopButtonsContainer")
	var target_y = left_top_button_container.size.y

	var extra_options_box = find_parent("*PlayMainHBox").get_node("PlaneContainer/ExtraOptionsVBox/ExtraOptionsHBox")
	var force_show = extra_options_box.force_show

	if extra_y >= target_y || !force_show:
		self.custom_minimum_size.y = 0
		self.visible = false
	else:
		self.custom_minimum_size.y = target_y
		self.visible = true
