"""Convert an API snapshot into BIMCraft's existing family commands (standard library only).

This module has no dependency on Revit. Coordinates in the snapshot are millimetres.
Run: python bridge.py snapshot.json NEW_OUTPUT_FOLDER
"""
from __future__ import print_function

import io
import json
import math
import os
import re
import sys

MAX_TRIANGLES = 20000
MAX_TYPES = 256
MAX_COORD = 100000000
MAX_FILE = 128 * 1024 * 1024
CATEGORIES = set("furniture casework plumbing drain pipefitting pipeaccessory equipment outlet switch lighting generic dataoutlet voiceoutlet patchpanel rack airterminal earthing inspectionbox".split())
LIMITATIONS = [
    "Geometry is a triangulated snapshot for each exported type, in millimetres, not reconstructed parametric forms.",
    "Revit formulas, constraints, connectors, instance parameters, materials, textures and 2D annotation are not converted.",
    "Doors and windows become generic components: their hosts, openings and operation are not reconstructed.",
    "The library script transfers loaded editable families and level names/elevations only. It does not convert an entire template.",
    "System family types, view templates, filters, sheets, schedules, project/shared parameters, units and routing preferences are not converted.",
    "Project instances and model geometry are not included in this library export; use the RVT/RTE or IFC importer separately.",
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
    if abs(value) > MAX_COORD or math.isnan(value) or math.isinf(value):
        raise ValueError("coordinate is nonfinite or outside the bridge's range")
    return value


def convert_family(source):
    """One mesh per type, gated by a native numeric type parameter; no asset extraction."""
    title = name(source["name"])
    types = source.get("types", [])
    if not types or len(types) > MAX_TYPES:
        raise ValueError("a family needs 1 to %s types" % MAX_TYPES)
    category = source.get("category", "generic")
    if category not in CATEGORIES:
        category = "generic"
    selector = "Imported Type"
    result = {"name": title, "category": category,
              "parameters": [{"name": selector, "kind": "number", "value": 0}],
              "types": {}, "forms": []}
    total = 0
    for index, item in enumerate(types):
        type_name = name(item["name"])
        if type_name in result["types"]:
            raise ValueError("duplicate family type: " + type_name)
        triangles = item.get("triangles", [])
        total += len(triangles)
        if not triangles or total > MAX_TRIANGLES:
            raise ValueError("empty geometry or more than %s triangles per family" % MAX_TRIANGLES)
        for triangle in triangles:
            if len(triangle) != 3:
                raise ValueError("a triangle needs three vertices")
            for vertex in triangle:
                if len(vertex) != 3:
                    raise ValueError("a vertex needs three coordinates")
                for value in vertex:
                    number(value)
        result["types"][type_name] = {selector: index}
        result["forms"].append({"mesh": triangles, "visible": "Imported Type = %s" % index})
    return result


def encode(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":"))


def write_text(path, text):
    with io.open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(TEXT_TYPE(text))


def write_bundle(snapshot, destination):
    """Never overwrite a previous export. Scripts embed definitions, not machine-specific paths."""
    destination = str(destination)
    os.makedirs(destination)
    report = {"converted": [], "failed": [], "notes": snapshot.get("notes", []),
              "limitations": LIMITATIONS, "source": snapshot.get("title", "Revit")}
    commands = []
    seen = set()
    encoded_size = 0
    for index, source in enumerate(snapshot.get("families", [])):
        try:
            family = convert_family(source)
            if family["name"] in seen:
                raise ValueError("duplicate family name")
            encoded = encode(family)
            size = len(encoded.encode("utf-8")) + 8
            if encoded_size + size > MAX_FILE:
                raise ValueError("library script would exceed 128 MiB")
            safe = re.sub(r'[^\w.-]', "_", family["name"], flags=re.UNICODE)[:80]
            write_text(os.path.join(destination, "%04d_%s.json" % (index, safe)), encoded + "\n")
            seen.add(family["name"])
            encoded_size += size
            commands.append("family " + encoded)
            report["converted"].append(family["name"])
        except (ValueError, KeyError, TypeError) as error:
            report["failed"].append({"family": source.get("name", "?"), "reason": str(error)})
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
    project.extend(commands)
    write_text(os.path.join(destination, "library.txt"), "\n".join(commands) + "\n")
    write_text(os.path.join(destination, "project.txt"), "\n".join(project) + "\n")
    write_text(os.path.join(destination, "report.json"), json.dumps(report, ensure_ascii=False, allow_nan=False, indent=2) + "\n")
    return report


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("Usage: python bridge.py snapshot.json NEW_OUTPUT_FOLDER")
    if os.path.getsize(sys.argv[1]) > MAX_FILE:
        sys.exit("Snapshot exceeds 128 MiB")
    with io.open(sys.argv[1], encoding="utf-8") as stream:
        snapshot = json.load(stream)
    result = write_bundle(snapshot, sys.argv[2])
    print("Converted %s families; %s failures. Read report.json." % (len(result["converted"]), len(result["failed"])))
