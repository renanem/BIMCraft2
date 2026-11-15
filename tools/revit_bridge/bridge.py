"""Write what the Revit API said of each family as BIMCraft's exchange files (standard library only).

This module has no dependency on Revit: script.py asks the API and hands a snapshot here. Each family becomes one
`NNNN_name.rfa.json` file, format `bimcraft-revit-family` version 1, which BIMCraft reads (Insert > Revit Family, or
`bimcraft-cli run project.bimc NNNN_name.rfa.json --save project.bimc`). A project or a template also becomes one
`NAME.rte.json`, format `bimcraft-revit-template` version 1: its levels, phases, line and fill patterns, line styles,
text and dimension types, object styles, project parameters, view filters, view templates, title blocks and sheets,
and the names of its families' files, which BIMCraft opens as a new project to save as a `.bimt`. BIMCraft, not this module, decides what a family
there may hold: a parameter, a formula or a mesh its rules refuse is left out there, by name, in its import report.

Lengths are millimetres, areas mm2, volumes mm3, angles degrees. Run: python bridge.py snapshot.json NEW_OUTPUT_FOLDER
"""
from __future__ import print_function

import io
import json
import math
import os
import re
import sys

FORMAT = "bimcraft-revit-family"
VERSION = 1
TEMPLATE_FORMAT = "bimcraft-revit-template"
TEMPLATE_VERSION = 1
MAX_TRIANGLES = 100000
MAX_TYPES = 1000
MAX_PARAMETERS = 2000
MAX_COORD = 1000000000
MAX_FILE = 256 * 1024 * 1024
KINDS = set("length area volume angle number integer yesno text url material other".split())
GROUPS = set("constraints construction dimensions graphics identity-data materials-and-finishes text visibility "
             "electrical mechanical plumbing structural data other".split())
OPTIONS = ("work_plane_based", "always_vertical", "cut_with_voids_when_loaded", "shared")
LIMITATIONS = [
    "Geometry is a triangulated snapshot of each exported type, in millimetres, not reconstructed parametric forms: "
    "changing a parameter in BIMCraft does not reshape it.",
    "Parameters come with their data type, instance or type, shared GUID, group and formula, and each type's values. "
    "A formula is exported as Revit writes it; BIMCraft keeps the ones it reads and reports the others.",
    "Reference planes, dimensions and constraints, connectors, materials, textures, nested families and 2D annotation "
    "are not converted.",
    "Doors and windows keep their category only where BIMCraft's own rule for one holds (length parameters Width and "
    "Height); their hosts and openings are not reconstructed.",
    "From a project, only loaded editable families and level names and elevations are exported. System family types, "
    "view templates, filters, sheets, schedules, project and shared parameters, units and routing preferences are not.",
]

try:
    STRING_TYPES = (basestring,)
    TEXT_TYPE = unicode
except NameError:
    STRING_TYPES = (str,)
    TEXT_TYPE = str


def name(value, what="a name"):
    if not isinstance(value, STRING_TYPES) or not value.strip() or len(value) > 256:
        raise ValueError("%s must contain 1 to 256 characters" % what)
    return value.strip()


def number(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError("coordinates must be numbers in millimetres")
    if math.isnan(value) or math.isinf(value) or abs(value) > MAX_COORD:
        raise ValueError("coordinate is nonfinite or outside the bridge's range")
    return value


def scalar(value):
    """A parameter's value as JSON holds one: None, a boolean, text, or a finite number."""
    if value is None or isinstance(value, bool) or isinstance(value, STRING_TYPES):
        return value
    if isinstance(value, (int, float)) and not (math.isnan(value) or math.isinf(value)):
        return value
    raise ValueError("a parameter's value must be text, yes or no, or a finite number")


def export_parameter(source):
    kind = source.get("kind", "other")
    if kind not in KINDS:
        raise ValueError("unknown parameter kind: %s" % kind)
    result = {"name": name(source["name"]), "kind": kind}
    if source.get("instance"):
        result["instance"] = True
    if source.get("reporting"):
        result["reporting"] = True
    for key in ("shared", "formula", "unit"):
        value = source.get(key)
        if isinstance(value, STRING_TYPES) and value.strip():
            result[key] = value.strip()
    if source.get("group") in GROUPS:
        result["group"] = source["group"]
    return result


def export_family(source):
    """One family as an exchange file's object. Raises ValueError for what no file should hold."""
    types = source.get("types", [])
    if not types or len(types) > MAX_TYPES:
        raise ValueError("a family needs 1 to %s types" % MAX_TYPES)
    parameters = source.get("parameters", [])
    if len(parameters) > MAX_PARAMETERS:
        raise ValueError("more than %s parameters" % MAX_PARAMETERS)
    result = {"format": FORMAT, "version": VERSION, "name": name(source["name"]),
              "category": source.get("category") if isinstance(source.get("category"), STRING_TYPES) else "",
              "parameters": [], "types": [],
              "notes": [note for note in source.get("notes", []) if isinstance(note, STRING_TYPES)]}
    options = source.get("options", {})
    kept = dict((key, options[key]) for key in OPTIONS if isinstance(options.get(key), bool))
    if kept:
        result["options"] = kept
    seen = set()
    for parameter in parameters:
        exported = export_parameter(parameter)
        if exported["name"] in seen:
            raise ValueError("duplicate parameter: " + exported["name"])
        seen.add(exported["name"])
        result["parameters"].append(exported)
    seen = set()
    total = 0
    for item in types:
        type_name = name(item["name"], "a type's name")
        if type_name in seen:
            raise ValueError("duplicate family type: " + type_name)
        seen.add(type_name)
        triangles = item.get("triangles", [])
        total += len(triangles)
        if total > MAX_TRIANGLES:
            raise ValueError("more than %s triangles per family" % MAX_TRIANGLES)
        for triangle in triangles:
            if len(triangle) != 3:
                raise ValueError("a triangle needs three vertices")
            for vertex in triangle:
                if len(vertex) != 3:
                    raise ValueError("a vertex needs three coordinates")
                for value in vertex:
                    number(value)
        values = dict((name(key, "a value's parameter name"), scalar(value)) for key, value in item.get("values", {}).items())
        result["types"].append({"name": type_name, "values": values, "triangles": triangles})
    return result


def listed(snapshot, key):
    """A snapshot's list of that name, or none: what is no list is passed over."""
    value = snapshot.get(key, [])
    return value if isinstance(value, list) else []


def export_template(snapshot, families):
    """A project's or a template's own settings as the template file's object, naming the family files beside it.

    BIMCraft runs a command for each entry and leaves out what the command refuses, so entries are passed on as the API
    gave them: only what no file should hold (a number that is none, a name that is none) is dropped here, with a note.
    """
    notes = [note for note in listed(snapshot, "notes") if isinstance(note, STRING_TYPES)]
    result = {"format": TEMPLATE_FORMAT, "version": TEMPLATE_VERSION, "name": name(snapshot.get("title", "Revit template")),
              "levels": [], "phases": [], "line_patterns": [], "line_styles": [], "fill_patterns": [], "text_types": [],
              "dimension_types": [], "object_styles": [], "parameters": [], "filters": [], "view_templates": [],
              "title_blocks": [], "sheets": [],
              "families": list(families), "notes": notes}
    for level in listed(snapshot, "levels"):
        try:
            result["levels"].append({"name": name(level["name"]), "elevation": number(level["elevation"])})
        except (ValueError, KeyError, TypeError) as error:
            notes.append("A level was not exported: %s" % error)
    for phase in listed(snapshot, "phases"):
        if isinstance(phase, STRING_TYPES) and phase.strip():
            result["phases"].append(phase.strip())
    for key in ("line_patterns", "line_styles", "fill_patterns", "text_types", "dimension_types", "object_styles", "parameters",
                "filters", "view_templates", "title_blocks", "sheets"):
        for entry in listed(snapshot, key):
            try:
                entry = dict(entry)
                # An object style is told by its category and a sheet by its number; everything else by its name.
                told = {"object_styles": "category", "sheets": "number"}.get(key, "name")
                entry[told] = name(entry[told])
                # allow_nan=False refuses what JSON cannot hold, before anything is written.
                # ensure_ascii=False as the file is written: IronPython 2's text is its `str`, which the ASCII encoder
                # takes for bytes and fails to decode at the first accented letter.
                json.dumps(entry, ensure_ascii=False, allow_nan=False)
                result[key].append(entry)
            except (ValueError, KeyError, TypeError) as error:
                notes.append("An entry of %s was not exported: %s" % (key, error))
    return result


def encode(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":"))


def write_text(path, text):
    with io.open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(TEXT_TYPE(text))


class Bundle(object):
    """An export folder written as it is read, never over a previous one: `families/NNNN_name.rfa.json`, `NAME.rte.json`
    (the project's or template's own settings, naming those families: what BIMCraft opens as a new project) and
    `report.json`.

    `settings` writes the template and the report before any family; `family` writes one family's file at once and the
    two again, so a run that stops leaves what it read, the report saying `complete: false`; `finish` writes them last.
    """

    def __init__(self, destination):
        self.destination = str(destination)
        os.makedirs(os.path.join(self.destination, "families"))
        self.report = {"exported": [], "failed": [], "files": [], "notes": [], "limitations": LIMITATIONS,
                       "source": "Revit", "complete": False}
        self.seen = set()
        self.count = 0
        self.snapshot = {}

    def settings(self, snapshot):
        self.snapshot = snapshot
        self._write()

    def family(self, source):
        index = self.count
        self.count += 1
        try:
            family = export_family(source)
            if family["name"] in self.seen:
                raise ValueError("duplicate family name")
            encoded = encode(family)
            if len(encoded.encode("utf-8")) > MAX_FILE:
                raise ValueError("the family's export would exceed 256 MiB")
            safe = re.sub(r'[^\w.-]', "_", family["name"], flags=re.UNICODE)[:80]
            file_name = "%04d_%s.rfa.json" % (index, safe)
            write_text(os.path.join(self.destination, "families", file_name), encoded + "\n")
            self.seen.add(family["name"])
            self.report["exported"].append(family["name"])
            self.report["files"].append("families/" + file_name)
        except (ValueError, KeyError, TypeError, AttributeError) as error:
            failed = source.get("name", "?") if isinstance(source, dict) else "?"
            self.report["failed"].append({"family": failed, "reason": str(error)})
        self._write()

    def finish(self, snapshot=None):
        if snapshot is not None:
            self.snapshot = snapshot
        self.report["complete"] = True
        self._write()
        return self.report

    def _write(self):
        template = export_template(self.snapshot, self.report["files"])
        safe = re.sub(r'[^\w.-]', "_", template["name"], flags=re.UNICODE)[:80]
        self.report["template"] = safe + ".rte.json"
        self.report["notes"] = template["notes"]
        self.report["source"] = self.snapshot.get("title", "Revit")
        write_text(os.path.join(self.destination, self.report["template"]), encode(template) + "\n")
        write_text(os.path.join(self.destination, "report.json"),
                   json.dumps(self.report, ensure_ascii=False, allow_nan=False, indent=2) + "\n")


def write_bundle(snapshot, destination):
    """A whole snapshot as a bundle, at once: its settings, each of its families, the report."""
    bundle = Bundle(destination)
    bundle.settings(snapshot)
    for source in snapshot.get("families", []):
        bundle.family(source)
    return bundle.finish()


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("Usage: python bridge.py snapshot.json NEW_OUTPUT_FOLDER")
    if os.path.getsize(sys.argv[1]) > MAX_FILE:
        sys.exit("Snapshot exceeds 256 MiB")
    with io.open(sys.argv[1], encoding="utf-8") as stream:
        snapshot = json.load(stream)
    result = write_bundle(snapshot, sys.argv[2])
    print("Exported %s families; %s failures. Read report.json." % (len(result["exported"]), len(result["failed"])))
