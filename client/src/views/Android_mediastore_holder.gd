extends Control

var android_mediastore = null
var root = null


func _ready():
	root = get_node("/root/GraphwarRootNode")

	if Engine.has_singleton("GodotAndroidMediaStorePlugin"):
		root.log("GodotAndroidMediaStorePlugin found")
		android_mediastore = Engine.get_singleton("GodotAndroidMediaStorePlugin")

func publish_image_to_album(image_path: String):
	root.log("publishing image: " + image_path)
	if android_mediastore != null:
		android_mediastore.publishImageToGallery(image_path)

func publish_video_to_album(video_path: String):
	root.log("publishing video: " + video_path)
	if android_mediastore != null:
		android_mediastore.publishVideoToGallery(video_path)
