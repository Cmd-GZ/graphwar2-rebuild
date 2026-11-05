extends GraphwarButton

var showing_buttons = false

func _ready():
	connect("pressed", _button_pressed)

	var plane_container = get_node("../../../../PlaneContainer")
	plane_container.item_rect_changed.connect(_on_size_changed)

func _button_pressed():
	var extra_options_box = get_node("../../../../PlaneContainer/ExtraOptionsVBox/ExtraOptionsHBox")
	showing_buttons = !showing_buttons
	extra_options_box.set_show_extra_buttons(showing_buttons)

	var view_function_pad = get_node("../../../../PlaneContainer/GamePlane/PlaneContainer/VBoxContainer/ViewFunctionPad")
	view_function_pad.update_sizes()

	_update_icon()

func _update_icon():
	if showing_buttons:
		self.set_button_icon(load("res://hide_extra_buttons.svg"))
	else:
		self.set_button_icon(load("res://show_extra_buttons.svg"))

func _on_size_changed():
	var left_top_button_container = get_node("../../../../LeftControls/LeftTopButtonsContainer")
	var target_size = left_top_button_container.size.y

	var plane_container = get_node("../../../../PlaneContainer")
	var minimum_y = plane_container.size.x / 1.66667
	var extra_y = plane_container.size.y - minimum_y

	if extra_y >= target_size:
		self.disabled = true
	else:
		self.disabled = false
