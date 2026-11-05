extends MarginContainer



func _ready():
	add_theme_constant_override("margin_left", 5)
	add_theme_constant_override("margin_right", 5)
	add_theme_constant_override("margin_top", 5)
	add_theme_constant_override("margin_bottom", 5)



func _process(delta):
	pass
