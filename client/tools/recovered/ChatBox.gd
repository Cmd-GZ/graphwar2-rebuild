extends RichTextLabel



func _ready():
	pass



func _process(delta):
	pass

func _gui_input(event):
	if event is InputEventMouseButton:
		if event.pressed:
			var chat_line_edit = get_node("../ChatLineEdit")
			chat_line_edit.grab_focus()
	elif event is InputEventScreenTouch:
		if event.pressed:
			var chat_line_edit = get_node("../ChatLineEdit")
			chat_line_edit.grab_focus()
