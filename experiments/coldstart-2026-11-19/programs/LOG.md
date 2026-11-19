# Yavo cold-start log
T01 attempt 1: ok — byte scan with s[i], s[a..b].toLower(), Map.update, sortBy; worked first time
T02 attempt 1: ok — helper fn returning int ! io.Error so 'while let line = try io.readLine()' works; main uses catch block with os.exit(1); used isSome() (only seen on ?Box in sheet) and ?? 0
T03 attempt 1: ok — os.args()[1..2], fs.readText catch _e block + os.exit(2), for i, line in text.lines(); all three cases (match/no match rc1/missing rc2) correct
T04 attempt 1: ok — enum with payload, 'st.pop() ?? return err(.Underflow)', tok.parseInt() ?? return err(...), match ok/err; worked first time
T05 attempt 1: ok — copied insert from sheet; read-only ?Box[Node] params passed without &, inout Vec passed with &out recursively; line.push(format(...))
T06 attempt 1: ok — Map getCopy(str)/set(toString)/remove(str) with 'let _old =', collect keys + sort() for report
T07 attempt 1: ok — greedy wrap with String{} push(str), cur.len() byte length, width from os.args parseInt
T08 attempt 1: ok — interface Shape + three 'struct X: Shape', Vec[Box[Shape]], Box(Circle{...}), sortBy with totalCmp == .Less, math.pi
T09 attempt 1: ok — guessed generic struct syntax 'struct Stack[T] { items: Vec[T] ... }' (sheet only shows generic fns), push(inout self, x: sink T); printed chars via line[i..i+1] slices to avoid byte interpolation; fn closer(c: byte) -> str of static
T10 attempt 1: ok — Vec[Vec[int]] grid, nested Vec moved with push, 'grid = next' reassign, read-only Vec[Vec[int]] param, bounds checks on ints
