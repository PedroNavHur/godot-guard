var a = "esc \" q" + 'it\'s' + "\\" + r"raw \d" + R'R'
var b = """long "quoted"
text""" + '''x'''
var c = $Node/Child + $"Quoted/Path" + %Unique + $%Unique/Child + ^"node/path" + &"name"
var d = x is not Node and y is Node or not z in w
var e = "# not a comment" # but this is
var iffy = notable and isx or forx # keyword prefixes stay NAMEs
