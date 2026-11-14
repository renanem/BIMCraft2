"""Write what the Revit API said of each family as BIMCraft's exchange files (standard library only).

This module has no dependency on Revit: script.py asks the API and hands a snapshot here. Each family becomes one
`NNNN_name.rfa.json` file, format `bimcraft-revit-family` version 1, which BIMCraft reads (Insert > Revit Family, or
`bimcraft-cli run project.bimc NNNN_name.rfa.json --save project.bimc`). BIMCraft, not this module, decides what a family
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


def name(value):
    if not isinstance(value, STRING_TYPES) or not value.strip() or len(value) > 256:
        raise ValueError("a name must contain 1 to 256 characters")
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
        type_name = name(item["name"])
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
        values = dict((name(key), scalar(value)) for key, value in item.get("values", {}).items())
        result["types"].append({"name": type_name, "values": values, "triangles": triangles})
    return result


def encode(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":"))


def write_text(path, text):
    with io.open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(TEXT_TYPE(text))


def write_bundle(snapshot, destination):
    """Never overwrite a previous export: `families/NNNN_name.rfa.json`, `project.txt` (the levels) and `report.json`."""
    destination = str(destination)
    os.makedirs(os.path.join(destination, "families"))
    report = {"exported": [], "failed": [], "files": [], "notes": snapshot.get("notes", []),
              "limitations": LIMITATIONS, "source": snapshot.get("title", "Revit")}
    seen = set()
    for index, source in enumerate(snapshot.get("families", [])):
        try:
            family = export_family(source)
            if family["name"] in seen:
                raise ValueError("duplicate family name")
            encoded = encode(family)
            if len(encoded.encode("utf-8")) > MAX_FILE:
                raise ValueError("the family's export would exceed 256 MiB")
            safe = re.sub(r'[^\w.-]', "_", family["name"], flags=re.UNICODE)[:80]
            file_name = "%04d_%s.rfa.json" % (index, safe)
            write_text(os.path.join(destination, "families", file_name), encoded + "\n")
            seen.add(family["name"])
            report["exported"].append(family["name"])
            report["files"].append("families/" + file_name)
        except (ValueError, KeyError, TypeError, AttributeError) as error:
            failed = source.get("name", "?") if isinstance(source, dict) else "?"
            report["failed"].append({"family": failed, "reason": str(error)})
    # Apply to a fresh project only: this script changes its two starter levels by their documented IDs.
    project = ["new " + encode({"name": name(snapshot.get("title", "Revit library"))})]
    levels = sorted(snapshot.get("levels", []), key=lambda item: number(item["elevation"]))
    for index, level in enumerate(levels):
        values = {"name": name(level["name"]), "elevation": number(level["elevation"])}
        if index < 2:
            values["id"] = 100000 + index
            project.append("modify " + encode(values))
        else:
            project.append("level " + encode(values))
    write_text(os.path.join(destination, "project.txt"), "\n".join(project) + "\n")
    write_text(os.path.join(destination, "report.json"),
               json.dumps(report, ensure_ascii=False, allow_nan=False, indent=2) + "\n")
    return report


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("Usage: python bridge.py snapshot.json NEW_OUTPUT_FOLDER")
    if os.path.getsize(sys.argv[1]) > MAX_FILE:
        sys.exit("Snapshot exceeds 256 MiB")
    with io.open(sys.argv[1], encoding="utf-8") as stream:
        snapshot = json.load(stream)
    result = write_bundle(snapshot, sys.argv[2])
    print("Exported %s families; %s failures. Read report.json." % (len(result["exported"]), len(result["failed"])))
