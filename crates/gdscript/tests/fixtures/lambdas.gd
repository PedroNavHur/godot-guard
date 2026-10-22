func f():
	call(func(a):
		print(a)
		, func():
			pass, 3)
	var g = func named(x) -> int:
		return [func():
			return x
		]
	return g
