extends HBoxContainer

var force_show = false


func _ready():
	var left_top_button_container = get_node("../../../LeftControls/LeftTopButtonsContainer")
	left_top_button_container.item_rect_changed.connect(_on_size_changed)

	var plane_container = get_node("../../../PlaneContainer")
	plane_container.item_rect_changed.connect(_on_size_changed)

	update_sizes()
	update_child_sizes()

func set_show_extra_buttons(show):
	force_show = show
	update_sizes()

func _on_size_changed():
	update_sizes.call_deferred()
	update_child_sizes.call_deferred()

func update_sizes():
	var left_top_button_container = get_node("../../../LeftControls/LeftTopButtonsContainer")
	self.custom_minimum_size.y = left_top_button_container.size.y

	var plane_container = get_node("../../../PlaneContainer")
	var minimum_y = plane_container.size.x / 1.66667
	var extra_y = plane_container.size.y - minimum_y

	if extra_y >= self.custom_minimum_size.y:
		self.visible = true
	else:
		self.visible = force_show

func update_child_sizes():
	var screen_size = get_viewport_rect().size
	var desired_size = Vector2(screen_size.x * 0.15 / 3.0, screen_size.x * 0.15 / 3.0)
	var font_size = screen_size.x / 30

	for node in get_children():
		node.custom_minimum_size = desired_size
		node.add_theme_font_size_override("font_size", font_size)
