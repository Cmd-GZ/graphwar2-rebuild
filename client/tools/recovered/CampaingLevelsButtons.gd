extends Control

var buttons = []


func _ready():
	var screen_size = get_parent().get_parent().size
	var font_size = screen_size.x / 15

	for i in 21:
		var button = GraphwarButton.new()
		button.text = str(i)
		button.input_text = "level_" + str(i)
		button.set_name("level_" + str(i))
		buttons.push_back(button)
		button.add_theme_font_size_override("font_size", font_size)
		add_child(button);


	self.resized.connect(_on_size_changed)
	call_deferred("place_buttons")


func _on_size_changed():
	call_deferred("place_buttons")

func place_buttons():


	var screen_size = self.size
	var font_size = screen_size.x / 15

	var pos_x_increment = screen_size.x / 8
	var pos_y_increment = screen_size.y / 4

	var pos = Vector2(pos_x_increment, pos_y_increment / 2)






	for button in buttons:
		button.add_theme_font_size_override("font_size", font_size)
		button.size.x = abs(pos_x_increment)
		button.position = pos - button.size / 2



		pos.x += pos_x_increment
		if pos.x > screen_size.x * 9 / 10 or pos.x < screen_size.x / 10:
			pos_x_increment = - pos_x_increment
			pos.x += pos_x_increment
			pos.y += pos_y_increment


	get_parent().queue_redraw()
