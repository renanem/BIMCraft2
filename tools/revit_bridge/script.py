# -*- coding: utf-8 -*-
"""pyRevit pushbutton: export the active family or the editable families of a project.

Uses the public Revit API, never the undocumented binary format. All temporary type
changes are rolled back. No transaction is committed and no Revit document is saved.
See README.md for installation, supported content and fidelity limits.
"""
from __future__ import print_function

import os
import sys

from pyrevit import DB, forms, revit

sys.path.insert(0, os.path.dirname(__file__))
import bridge


def text(value):
    return type(u"")(value)


def element_id(element):
    # Value is the 64-bit ID API; IntegerValue supports older Revit releases.
    try:
        return element.Id.Value
    except AttributeError:
        return element.Id.IntegerValue


def category(family):
    """Public BuiltInCategory names, not undocumented numeric category IDs."""
    names = {
        "OST_Furniture": "furniture", "OST_Casework": "casework",
        "OST_PlumbingFixtures": "plumbing", "OST_PipeFitting": "pipefitting",
        "OST_PipeAccessory": "pipeaccessory", "OST_ElectricalEquipment": "equipment",
        "OST_ElectricalFixtures": "outlet", "OST_LightingFixtures": "lighting",
        "OST_LightingDevices": "switch", "OST_DataDevices": "dataoutlet",
        "OST_TelephoneDevices": "voiceoutlet",
        "OST_Doors": "door", "OST_Windows": "window",
    }
    if family.FamilyCategory is not None:
        value = element_id(family.FamilyCategory)
        for builtin, target in names.items():
            if value == int(getattr(DB.BuiltInCategory, builtin)):
                return target
    return "generic"


def triangles(geometry, output, depth=0):
    if depth > 32:
        raise ValueError("nested geometry exceeds 32 levels")
    if geometry is None:
        return
    for item in geometry:
        if isinstance(item, DB.GeometryInstance):
            # GetInstanceGeometry returns geometry in the containing document's coordinates.
            triangles(item.GetInstanceGeometry(), output, depth + 1)
        elif isinstance(item, DB.Solid):
            for face in item.Faces:
                append_mesh(face.Triangulate(), output)
        elif isinstance(item, DB.Mesh):
            append_mesh(item, output)


def append_mesh(mesh, output):
    if len(output) + mesh.NumTriangles > bridge.MAX_TRIANGLES:
        raise ValueError("type geometry exceeds %s triangles" % bridge.MAX_TRIANGLES)
    for index in range(mesh.NumTriangles):
        triangle = mesh.get_Triangle(index)
        vertices = []
        for corner in range(3):
            p = triangle.get_Vertex(corner)
            vertices.append([bridge.number(p.X * 304.8), bridge.number(p.Y * 304.8), bridge.number(p.Z * 304.8)])
        output.append(vertices)


def family_geometry(document):
    options = DB.Options()
    options.DetailLevel = DB.ViewDetailLevel.Fine
    options.IncludeNonVisibleObjects = False
    output = []
    for item in DB.FilteredElementCollector(document).WhereElementIsNotElementType():
        if isinstance(item, DB.GenericForm):
            if item.IsSolid:
                triangles(item.get_Geometry(options), output)
        elif isinstance(item, DB.FamilyInstance):
            triangles(item.get_Geometry(options), output)
    return output


def read_family(document):
    owner = document.OwnerFamily
    manager = document.FamilyManager
    types = list(manager.Types)
    if len(types) > bridge.MAX_TYPES:
        raise ValueError("more than %s family types" % bridge.MAX_TYPES)
    snapshot = {"name": text(owner.Name), "category": category(owner), "types": []}
    transaction = DB.Transaction(document, "BIMCraft temporary type evaluation")
    try:
        if transaction.Start() != DB.TransactionStatus.Started:
            raise ValueError("could not start temporary type evaluation")
        if types:
            for current in types:
                manager.CurrentType = current
                document.Regenerate()
                snapshot["types"].append({"name": text(current.Name), "triangles": family_geometry(document)})
        else:
            snapshot["types"].append({"name": "Default", "triangles": family_geometry(document)})
    finally:
        try:
            if transaction.GetStatus() == DB.TransactionStatus.Started:
                if transaction.RollBack() != DB.TransactionStatus.RolledBack:
                    raise RuntimeError("temporary type evaluation could not be rolled back; stop the export")
        finally:
            transaction.Dispose()
    return snapshot


def export_active():
    document = revit.doc
    if document is None:
        forms.alert("Open a family, project, or a project created from your template first.", exitscript=True)
    parent = forms.pick_folder(title="Folder for a new BIMCraft exchange")
    if not parent:
        return
    destination = os.path.join(parent, "BIMCraft-exchange")
    if os.path.exists(destination):
        forms.alert("BIMCraft-exchange already exists. Select another parent folder; exports are never overwritten.", exitscript=True)
    snapshot = {"title": text(document.Title), "families": [], "levels": [], "notes": []}
    if document.IsFamilyDocument:
        snapshot["families"].append(read_family(document))
    else:
        snapshot["levels"] = [{"name": text(level.Name), "elevation": level.ProjectElevation * 304.8}
                              for level in DB.FilteredElementCollector(document).OfClass(DB.Level)]
        for family in DB.FilteredElementCollector(document).OfClass(DB.Family):
            if not family.IsEditable or family.IsInPlace:
                snapshot["notes"].append("Family %s (%s) is not editable or is in-place; not exported" % (family.Name, element_id(family)))
                continue
            opened = None
            try:
                opened = document.EditFamily(family)
                snapshot["families"].append(read_family(opened))
            except RuntimeError:
                raise
            except Exception as error:
                snapshot["notes"].append("Family %s (%s) not exported: %s" % (family.Name, element_id(family), error))
            finally:
                if opened is not None:
                    opened.Close(False)
    report = bridge.write_bundle(snapshot, destination)
    forms.alert("Exported %s families; %s conversion failures. Read report.json.\n%s" %
                (len(report["converted"]), len(report["failed"]), destination))


if __name__ == "__main__":
    export_active()
