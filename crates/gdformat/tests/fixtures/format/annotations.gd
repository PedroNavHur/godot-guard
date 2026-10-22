@tool
extends Node
@export_group("Group")
@export var a: int = 1
@export_range(0, 100, 1, "or_greater", "or_less", "suffix:px", "hide_slider", "exp") var very_long_name: float = 0.5
@onready @export var b := $Node
@warning_ignore("unused_parameter")
func f(x):
	@warning_ignore("unused_variable")
	var y = 1
@abstract
func g() -> int
@abstract class_name Base
@warning_ignore("return_value_discarded") func h():
	pass
