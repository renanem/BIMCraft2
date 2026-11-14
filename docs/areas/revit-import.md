# Revit files and the Revit import

Moved from the root `CLAUDE.md`, word for word: the root keeps the rules and the map, and this file is read when the area is touched.

- Revit files: Autodesk's files are never committed. Download sample projects into folders of their own outside the repo (the
  URLs, sizes and hashes are in [docs/rvt-provenance.md](../rvt-provenance.md)), then `BIMCRAFT_RVT_SAMPLES=<folder> cargo test -p
  bimcraft-rvt` runs the real-file tests (they skip when it is unset; the variable takes several folders, parted like a PATH) and
  `bimcraft-cli info x.rvt` shows what a file holds. `bimcraft-cli run x.rvt --save /tmp/plan.png` imports one (`--cmd 'render
  {"view": "{3D}", "path": "/tmp/3d.png"}'` for the 3D view; look at both) and `--save out.ifc` writes it as IFC, which IfcOpenShell
  validates (`express_rules=True`) and builds shapes of, to compare with rvt-rs's own export. After touching the reader, compare it
  with rvt-rs built from its own clone and target directory on the same files (a measuring stick, never a dependency), pin what
  matched in `crates/rvt/src/tests/rvt_rs.rs` as numbers (never Autodesk's strings), extend the mutation sweeps (`tests::sweep` for a
  new kind of element), and add a row to the provenance document for every module and every format finding. Mutation checks of the
  reader go on a scratch copy of the workspace with its own target, as the other crates' do.

  The import (`crates/rvt/src/import.rs`) reads a Revit file into a `bimcraft_core` model and says what it left out. `build` runs one
  survey of the file (`inventory::Survey`: the records of the categories it maps, the type records, the levels), then one reader per
  kind in this order, because each reads what the one before made: levels, walls, floors and ceilings, roofs, doors and windows
  (hosted in the walls made), columns, rooms (their point found among the walls made). A reader takes the newest copy of a record
  across the file's partitions, converts feet to millimetres (`mm`, `micron`), and builds elements the way a command would; it makes
  no rule of its own. `finish` runs `Model::check` and drops, with the rule's message, whatever fails it (a wall takes its doors along)
  until the model passes, so what comes back always checks. A reader never invents: what a record does not say, or the reader cannot
  decode, becomes a line of `Imported::skipped` through `Import::left_out` or `note`, grouped by reason with the ids. Every length it
  takes is within `records::MAX_FEET` of zero, since the importer asks the model for geometry (a room's region) before the check.
  Wall sides follow the file's flip (set: the exterior is on the right, so start and end swap and an arc's sign changes); an opening's
  host is the nearest wall below its own id in its reference list, else above, and its hole is its box along an axis-parallel wall;
  a column is its box's rectangle (a turned one: its symbol's size and the placement's angle); a floor, ceiling or roof is the loops
  of its `OST_SketchLines` (the owner of a curve is the one id among its second reference list's that is a floor, a ceiling or a
  roof, first, middle or last; a curve's record reads as both a line and an arc, and the box it carries says which it is);
  a floor edited with Revit's shape tools comes in shaped by its top's vertices, which its data holds, its top at the height most
  of them have (the creases between them are BIMCraft's own, and a variable layer's level underside is not read); a sketch that
  does not close is tried again without the lines that cannot bound it (another sketch's, past the element's box, and a line
  drawn twice); a roof by footprint comes in level (one that rises is a stand-in, its slopes unread), one by extrusion only where its box
  confirms it, and a wall that names a flat roof or one by extrusion has its top cut by it. A room tries the
  middle of its box and then a grid of points over it, and making the arrangement of a level's walls is what costs (`geom::faces`
  splits and matches every segment), so the faces of a level are made once and every point of every room of that level is asked of
  them (`geom::region_in`): the Snowdon sample's import went from 133 s to 2.4 s in a release build when the rooms stopped remaking
  their level's plan for each point. The notes are part of the product: a change that leaves something out says so.

  A workshared (central) file up to 2023 differs in two ways the survey allows for. A record's header tag is followed by a counted
  list of 14-byte items, empty in a file that is not workshared, so `records::id_before` finds a record's id back from its category
  (14 bytes, and 14 more for each item) instead of at a fixed distance from the marker; and the element table's two id columns differ in
  many records, an element's record carrying either, so `elem_table::parse` lists both. A real 128 MB central model inflates its one
  partition to 664 MiB, which is why `Limits::max_inflated` is 2 GiB. Such a file imports what the importer has a reader for (its
  levels, walls, floors, pipes, ducts and cable trays as runs, and its fittings, fixtures, equipment and terminals as boxes of a family of their category, `generic.rs`). A category the name table has no name for (`inventory::category_name`) is not
  guessed at: its placed elements come in as boxes too, of a family named by the category's id (`Category -2008123`), and the note
  says its category has no name here; one whose box is the box of an element of a named category is that element's companion (every
  pipe, fitting and duct has one in such a file), so it is left out and listed. The categories that have a name and no reader (model
  lines, rebar, areas, assemblies, wall reveals) are listed in the notes with how many there are.

  The categories the name table knows that had no reader come in from the survey's own records, their category and box, with no
  new finding in an element's data (`framing.rs`, `generic.rs`'s `KINDS`). Structural framing is a beam along its box's longer side,
  through the middle of the box across it, its top the box's top on the level at or below it, its section the box across it; a member
  whose box runs across the axes in plan (wider than a third of its length) or stands (as deep as it is long) is left out, since its
  line is not read and its box would make a beam of another width and direction. Structural foundations are isolated footings of their
  box (a wall foundation or a mat too). Stairs' runs, landings and supports, railings, curtain panels and mullions, wall sweeps, slab
  edges, conduits, furniture systems (of the furniture category), site pads, masses, trusses, beam systems, stiffeners and
  connections come in as boxes of a family of their category, as fittings do, each saying so (a model group, class
  `ElementGroup`, and terrain, class `Toposolid` or `SiteSurface`, are left out instead: a group's members come in on their
  own, and terrain's box is a solid block over the site), but a box is a fair stand-in only for
  some (`generic::Shape`): a path's (a railing, a mullion, a sweep, a slab edge, a conduit) only where it runs along an axis, so one
  that turns a corner or runs across the axes, whose box is a block where there is none, is left out with that; and a whole (a stair,
  a truss, a beam system) is left out where its box holds a box of its parts' categories, which come in on their own. Conduits come
  in as boxes and not as runs: the finding that a duct's or a cable tray's data holds its line (below) was made on those two
  categories, and no conduit was looked at. A curtain wall still comes in as the plane of its box, its panels and mullions as boxes
  over it.

  Every note of an element left out reads `N kinds left out, why (ids …)` (`Import::left_out`; an element the model's rules drop
  after it was made goes through `Import::dropped`), because the desktop adds those numbers up for its status line
  (`revit::count_left_out`); a note about something that came in reads `N kinds: why (ids …)`. A roof by extrusion (class `Roof`) is its
  sketch lines' chain as the profile of its top, with its type's thickness square to each slope, cut plumb, and it comes in only where
  the box that makes is the record's box to a millimetre (`roofs::extruded`): rvt-rs does not read it, so the provenance row says it is
  read from the sample's bytes. The desktop (`crates/ui-egui/src/revit.rs`) never depends on this crate: the window has a hook,
  `BimApp::revit: Option<Importer>`, which `apps/bimcraft`'s `main` fills when built with the `rvt` feature, so a test never reads a real
  file and a build without it has no `.rvt` option (`open_extensions`) and says why when given one. `open_revit` reads the file on a thread
  of its own behind `catch_unwind`; `poll_revit` takes the answer each frame (a refusal, a panic or a lost thread is a status message),
  `importing_box` holds the window still (keys stand down, `os_requests` queues the files that come meanwhile, the last wins, and Quit is
  still heard) and `install_revit` makes it a new unsaved project (`Session::reset(model, None)`, `reopened`; never in File > Open Recent)
  with a `Report` that File > Revit import report shows until another project replaces it (`reopened` drops it); its Copy Report button
  puts `Report::text`, the whole report in plain text with every note and its ids, on the clipboard, since the window's labels select
  one at a time (issue 45). A file named on the
  command line is `BimApp::open_later`: the window comes up first. Tests give the window a closure for a reader and block it at a gate
  (`tests/revit.rs`, with `step`, never `run`).

## Revit families

A family file (`.rfa`) is read by `crates/rvt/src/family.rs` (`family_path`, `Revit::family`) from its `PartAtom` entry, the Atom
document every family file holds and Revit's public API writes (`Application.ExtractPartAtomFromFamilyFile`). What comes back
(`FamilyFile`) is the params of the `family` command and the notes: the family's category (Revit's English name of it where
BIMCraft has that category, else generic and said; a door or a window without length parameters `Width` and `Height` is
generic too, with the rule), its options (work plane-based, always vertical, shared, cut with voids, Default Elevation, which
is the family's here), its types, and each type's parameters: name, kind, value in the model's units and a shared one's GUID.
Revit names a data type in the language it saved the file in, so only the English names are told by name; any other is told by
its unit (a unit of length makes a length), a yes or no by its data type being its two answers, and what is left is a number in
the file's unit (said, with the unit) or text. A length may carry its unit or be feet and fractional inches. A name the
family's rules refuse is rewritten with `_` and said; a parameter or type they still refuse (`Family::check` runs on each
parameter as it is added) is left out by name with the rule, as are those past `MAX_PARAMETERS` and `MAX_TYPES`. The entry
holds types' values only: geometry, reference planes, dimensions, instance parameters, formulas, connectors, materials, nested
families and plan symbols are not read, the last note says so, and the bridge through Revit's API
([tools/revit_bridge](../../tools/revit_bridge/README.md)) is where they come from. On the command line a `.rfa` named to `run`
is loaded into the session's project (`revit::load_family`: `bimcraft-cli run project.bimc chair.rfa --save project.bimc`),
its reply listing the notes in `skipped`; `convert` refuses one, since a family is no project. To look at one: load it, read it
back with `family {"name": …}`, place it with `component` and list it with `elements`. `BIMCRAFT_RFA_SAMPLES=<folders> cargo
test -p bimcraft-rvt family:: -- --nocapture` reads every `.rfa` under the folders (a Revit installation's library, read in
place), loads each through the `family` command and tallies the notes.

On the desktop it is Insert > Revit Family (`crates/ui-egui/src/revit_family.rs`), there only when the window has a reader:
`BimApp::revit_family: Option<FamilyReader>`, which `apps/bimcraft`'s `main` fills with `bimcraft_rvt::family_path` under the
`rvt` feature, so the window never depends on the reader and a test's reader is a closure. The button asks for a `.rfa`,
`import_revit_family` reads it at once behind `catch_unwind` (the entry is at most a mebibyte, so no thread; a refusal or a
panic is a status message) and the screen Import Revit Family shows it before anything changes: the file, the name (a field:
one the project has already, or none, greys Import and says so), the category, the types, a row for each parameter (name,
data type, the value of the first type in the project's units, Shared with the GUID as its tip) and every note. Copy Report
puts `FamilyBox::text` on the clipboard. Import is one `family` command, one undo step, after which the Component tool holds
the family and its first type and the status box says how many types and parameters came in; what the command refuses stays
on the screen in red. `BIMCRAFT_REVIT_FAMILY_DUMP=<folder> cargo test -p bimcraft-ui-egui revit_family_screenshots` writes the
screen light and dark, and with a name already taken.

## Revit family library exchange and project templates (fork)

The desktop also offers `.rte` project templates through the existing project importer. These open as unsaved projects, with an explicit report note: unused definitions and template settings are not reconstructed. A binary `.rfa`/`.rft` file is not opened as a project: its types and parameters load as a family (above), and its geometry comes through the public-API [family library bridge](../../tools/revit_bridge/README.md). It exports per-type mesh snapshots and a self-contained native command script, not parametric constraints or an entire template. No Autodesk assets are bundled. The API exporter still needs validation in Revit; converter tests use synthetic geometry.
