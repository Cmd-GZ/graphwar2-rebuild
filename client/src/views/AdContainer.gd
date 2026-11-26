extends MarginContainer


func _ready():
	visibility_changed.connect(_update_size)

	var ad_holder = get_node("/root/GraphwarRootNode/Ad_holder")
	ad_holder.ad_loaded.connect(_update_size)

func _on_visibility_changed():

	_update_size.call_deferred()

func _update_size():


	var ad_holder = get_node("/root/GraphwarRootNode/Ad_holder")
	self.custom_minimum_size.x = ad_holder.ad_size.x
	self.custom_minimum_size.y = ad_holder.ad_size.y
