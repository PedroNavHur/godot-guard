extends SceneTree

class Helper extends RefCounted:
	func _process(_delta: float) -> void:
		var root := get_node("Main")
