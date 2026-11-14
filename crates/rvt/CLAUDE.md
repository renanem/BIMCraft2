# bimcraft-rvt

The reader of Autodesk Revit files (`.rvt`, `.rfa`), which puts a project's elements into a BIMCraft model. Policy comes first:
the crate is Apache-2.0 only, ported in part from rvt-rs and fenced by [docs/rvt-provenance.md](../../docs/rvt-provenance.md)
(what may be read from where, and what is never copied). Autodesk's files are never committed: tests build synthetic ones
(`fixture.rs`, `project_fixture.rs`) and the real samples are read only where `BIMCRAFT_RVT_SAMPLES` names a folder of them.

- **Read only what a record says, and say what is not.** Anything the reader cannot decode (groups, families, rebar, a skylight,
  a room no walls enclose) is left out and listed in the import's `skipped` as `N kinds left out, why`, which the desktop adds up;
  a rule of the model (`Model::check`) a made element breaks leaves that element out with the rule. Where the record's own box
  gives a stand-in (a curtain wall as the plane of its box, a sloped slab level, a sketch that closes nothing as the footprint of
  its box, a beam along its box, a stair's run as its box) the element comes in as that stand-in and a note says what was not read: that is an approximation, never counted as
  read whole, and never a way to bring the "left out" count down.
- **A module for each part of a file** (`container`, `gzip`, `records`, `schema`, `inventory`, `elem_table`, `atom`...) and **for
  each element kind** it imports (`walls`, `floors`, `roofs`, `columns`, `openings`, `rooms`, `pipes`, `ducts`, `framing`, `generic`), joined by `import.rs`. A new kind
  is a module with its own tests, a line in `import.rs`, and its place in the write-up
  [Revit files and the Revit import](../../docs/areas/revit-import.md).
- **A family file is its types and their parameters** (`family.rs`, from the `PartAtom` entry): the params of the `family`
  command, checked against `Family::check` as they are built, with notes of what the entry does not hold (geometry, instance
  parameters, formulas) and of each parameter or type left out or renamed. Data types are told by unit, never by a
  translated name.
- **What the bridge exported is not Revit's format** (`exchange.rs`): JSON of BIMCraft's own, written from Revit's public API
  by `tools/revit_bridge`, read as hostile and built the same way, formulas tried one at a time and a mesh for each type.
  A change to what the bridge writes changes `exchange.rs`, its `VERSION` when an older reader would misread it, and the
  layout at the top of the module.
- **A template from the bridge is made by commands** (`template.rs`): each entry of the export runs its command on a
  new `Session`, and what a command refuses is a note. A new kind of setting is a list in the export, a command here, a
  count in the first note and a line less in the last.
- Lengths and counts a file claims are bounded before anything is allocated (`Limits`), and the sweep of hostile files is part of
  the tests.
- References to levels and hosts in the model are GUIDs (`Model::guid`), and what the import puts in the library is given its ids
  by `sync_library` before the model is checked.
- `cargo test -p bimcraft-rvt` runs in about twenty seconds; filter by module (`walls::`, `floors::`, `container::`).
