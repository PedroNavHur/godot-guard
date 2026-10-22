class_name bad_class extends Node
class inner_class:
	pass
enum my_enum {lower, UPPER}
signal BadSignal
func f(BadArg):
	const L = preload("x")
	var P = preload("y")
	var Q = load("z")
	for I in BadArg:
		pass
	for J: int in BadArg:
		pass
	return P == Q
