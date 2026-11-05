extends HBoxContainer



func _ready():
	self.resized.connect(_on_size_changed)

func _on_size_changed():
	call_deferred("set_minimum_size")

func set_minimum_size():
	var parent_size = get_parent().size
	self.custom_minimum_size.y = 0.25 * parent_size.y
