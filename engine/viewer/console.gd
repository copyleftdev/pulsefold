extends Control

const BG := Color("#0a111a")
const INK := Color("#d6e6ed")
const MINT := Color("#78f4d0")
const BAD := Color("#ff806d")
const DIM := Color("#8cabb9")
const HEADER := "turn\tstep\tvariant\toperation\texpected\tresult\tledger\tindex\toriginal_window\treference_original\timplementation_original\tattempted_window\treference_attempted\timplementation_attempted\tmarker\trefactor\tverdict\treason"
const WIDTH := 18
# Trace v2 columns.
enum { TURN, STEP, VARIANT, OP, EXPECTED, RESULT, LEDGER, INDEX, OW, RO, IO, AW, RA, IA, MARKER, REFACTOR, VERDICT, REASON }
var rows: Array[PackedStringArray] = []
var step := 0
var mutant := false
var status_note := ""
var controls: HBoxContainer

func _ready() -> void:
	controls = HBoxContainer.new()
	controls.position = Vector2(38, 565)
	controls.add_theme_constant_override("separation", 14)
	add_child(controls)
	add_button("BACK", func(): change_step(-1))
	add_button("STEP", func(): change_step(1))
	add_button("RESET", func(): step = 0; queue_redraw())
	add_button("TOGGLE FAULT", func(): mutant = not mutant; load_trace())
	load_trace()
	if "--validate" in OS.get_cmdline_user_args():
		var normal: bool = rows.size() == 9 and rows.all(func(r): return r[VERDICT] == "PASS")
		mutant = true
		load_trace()
		var failing := rows.filter(func(r): return r[VERDICT] == "FAIL")
		var broken: bool = rows.size() == 9 and failing.size() == 1 and failing[0][RESULT] == "CONFLICT" \
			and failing[0][MARKER] == "EXACT" and failing[0][RO] != failing[0][IO] and failing[0][RA] == failing[0][IA]
		if normal and broken:
			print("Viewer loaded both traces successfully.")
		get_tree().quit(0 if normal and broken else 1)

func add_button(label: String, callback: Callable) -> void:
	var b := Button.new()
	b.text = label
	b.custom_minimum_size = Vector2(144, 44)
	b.pressed.connect(callback)
	controls.add_child(b)

func change_step(delta: int) -> void:
	step = clampi(step + delta, 0, maxi(0, rows.size() - 1))
	queue_redraw()

func load_trace() -> void:
	rows.clear()
	step = 0
	var path := "res://mutant.tsv" if mutant else "res://control.tsv"
	status_note = "PACKAGED EXAMPLE TRACE"
	for arg in OS.get_cmdline_user_args():
		var prefix := "--mutant=" if mutant else "--control="
		if arg.begins_with(prefix):
			path = arg.trim_prefix(prefix)
			status_note = "IMPORTED ASSAY TRACE"
	var raw := FileAccess.get_file_as_string(path)
	var lines := raw.replace("\r\n", "\n").split("\n", false)
	if lines.is_empty() or lines[0] != HEADER:
		status_note = "NOT A TRACE V2 FILE"
		queue_redraw()
		return
	for i in range(1, lines.size()):
		var fields := lines[i].split("\t", true)
		if fields.size() == WIDTH and fields[STEP] == str(i):
			rows.append(fields)
		else:
			rows.clear()
			status_note = "MALFORMED TRACE ROW %d" % i
			break
	queue_redraw()

func txt(content: String, x: float, y: float, px: int, color: Color = INK) -> void:
	draw_string(ThemeDB.fallback_font, Vector2(x, y), content, HORIZONTAL_ALIGNMENT_LEFT, -1, px, color)

func panel(x: float, y: float, title: String) -> void:
	draw_rect(Rect2(x, y, 470, 140), Color("#111e2c"), true)
	draw_rect(Rect2(x, y, 470, 140), Color("#274257"), false, 1.0)
	txt(title, x + 20, y + 33, 18, DIM)

func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), BG)
	txt("P U L S E F O L D   /   ORBITAL METABOLISM", 38, 44, 17, MINT)
	txt("The Ghost Cycle", 38, 95, 40)
	txt("%s   /   %s" % [status_note, "STALE INDEX INJECTED" if mutant else "CONTROL"], 40, 129, 14, BAD if mutant else DIM)
	if rows.is_empty():
		txt("No usable trace loaded.", 40, 195, 23, BAD)
		return
	var row := rows[step]
	var failed := row[VERDICT] == "FAIL"
	txt("TURN %s  %s   STEP %s / %d     %s     >     %s" % [row[TURN], row[VARIANT], row[STEP], rows.size(), row[OP], row[RESULT]], 40, 190, 20)
	panel(38, 224, "REFERENCE LEDGER")
	panel(525, 224, "IMPLEMENTATION ANSWERS FROM")
	txt(row[LEDGER], 58, 298, 20)
	txt(row[INDEX], 545, 298, 20, BAD if row[INDEX] != row[LEDGER] else INK)
	if row[OW] != "-":
		txt("Original " + row[OW] + "     " + row[RO], 58, 327, 17)
		txt("Original " + row[OW] + "     " + row[IO], 545, 327, 17, BAD if row[RO] != row[IO] else INK)
	txt("Attempted " + row[AW] + "     " + row[RA], 58, 350, 17)
	txt("Attempted " + row[AW] + "     " + row[IA], 545, 350, 17, BAD if row[RA] != row[IA] else INK)
	txt("CANARY  " + row[MARKER] + "     REFACTOR  " + row[REFACTOR], 40, 430, 20, MINT)
	txt("INDEPENDENT VERDICT    " + row[VERDICT], 527, 430, 20, BAD if failed else MINT)
	txt(row[REASON] if failed else "The canary can be exact while the chamber display is wrong.", 40, 484, 18 if failed else 22, BAD if failed else DIM)
