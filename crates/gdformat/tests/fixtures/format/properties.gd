var a: int = 0:
	set(value):
		a = value
	get:
		return a
var b = 1: set = _set_b, get = _get_b
var c:
	get():
		return 2
var d = 3:
	set = _set_d,
	get = _get_d
