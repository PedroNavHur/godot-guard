func f(x, y):
	var z = 1
	if x:
		return 1
	elif y:
		return 2
	else:
		var z = 3
		return z
func g(x):
	if x:
		match x:
			1:
				return 1
			_:
				return 2
	else:
		return 3
	while x:
		return 4
func h(x):
	if x:
		if x > 1:
			return 1
		else:
			return 2
	elif x == x:
		pass
	else:
		return 3
