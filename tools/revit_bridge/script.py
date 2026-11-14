# -*- coding: utf-8 -*-
"""pyRevit pushbutton: export the active family or the editable families of a project.

Uses the public Revit API, never the undocumented binary format: each family's parameters (data type, instance or type,
shared GUID, formula, group), each type's values and each type's geometry as triangles. All temporary type
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
    """The family's category as the public BuiltInCategory names it (OST_Furniture); BIMCraft maps it."""
    if family.FamilyCategory is None:
        return ""
    try:
        return text(family.FamilyCategory.BuiltInCategory)
    except Exception:
        return ""


def same(a, b):
    """Whether two ForgeTypeIds are one, however the engine compares them."""
    try:
        return a is not None and b is not None and a.TypeId == b.TypeId
    except Exception:
        return False


def kind(parameter):
    """BIMCraft's kind for a parameter's data type, a public SpecTypeId; `other` for those it has none for."""
    try:
        spec = parameter.Definition.GetDataType()
    except Exception:
        return "other"
    S = DB.SpecTypeId
    for known, name in [(S.Length, "length"), (S.Area, "area"), (S.Volume, "volume"), (S.Angle, "angle"),
                        (S.Number, "number"), (S.Int.Integer, "integer"), (S.Boolean.YesNo, "yesno"),
                        (S.String.Text, "text"), (S.String.MultilineText, "text"), (S.String.Url, "url"),
                        (S.Reference.Material, "material")]:
        if same(spec, known):
            return name
    if parameter.StorageType == DB.StorageType.String:
        return "text"
    return "other"


def group(parameter):
    """The heading a parameter is listed under, a public GroupTypeId, as BIMCraft names its own."""
    try:
        own = parameter.Definition.GetGroupTypeId()
    except Exception:
        return None
    G = DB.GroupTypeId
    for known, name in [(G.Constraints, "constraints"), (G.Construction, "construction"), (G.Geometry, "dimensions"),
                        (G.Graphics, "graphics"), (G.IdentityData, "identity-data"), (G.Materials, "materials-and-finishes"),
                        (G.Text, "text"), (G.Visibility, "visibility"), (G.Electrical, "electrical"),
                        (G.Mechanical, "mechanical"), (G.Plumbing, "plumbing"), (G.Structural, "structural"),
                        (G.Data, "data"), (G.General, "other")]:
        if same(own, known):
            return name
    return None


# Revit's internal units are feet and radians: what one of them is in BIMCraft's.
FACTORS = {"length": 304.8, "area": 304.8 ** 2, "volume": 304.8 ** 3, "angle": 57.29577951308232}


def unit(parameter):
    """The label of the unit an `other` parameter is shown in, or None."""
    try:
        return text(DB.LabelUtils.GetLabelForUnit(parameter.GetUnitTypeId()))
    except Exception:
        return None


def value(document, family_type, parameter, name):
    """A type's value of a parameter, in BIMCraft's units; None where it has none or the API holds none of that storage."""
    try:
        if not family_type.HasValue(parameter):
            return None
        storage = parameter.StorageType
        if storage == DB.StorageType.Double:
            number = family_type.AsDouble(parameter)
            if number is None:
                return None
            if name in FACTORS:
                return number * FACTORS[name]
            if name == "other":
                try:
                    return DB.UnitUtils.ConvertFromInternalUnits(number, parameter.GetUnitTypeId())
                except Exception:
                    return number
            return number
        if storage == DB.StorageType.Integer:
            number = family_type.AsInteger(parameter)
            if number is None:
                return None
            return bool(number) if name == "yesno" else int(number)
        if storage == DB.StorageType.String:
            return text(family_type.AsString(parameter) or "")
        if storage == DB.StorageType.ElementId:
            # A material, an image, a nested family's type: its name.
            element = document.GetElement(family_type.AsElementId(parameter))
            return text(element.Name) if element is not None else None
    except Exception:
        return None
    return None


def parameters(manager):
    """Every parameter of the family as the API gives it: name, kind, instance or type, shared GUID, formula, group."""
    listed = []
    for parameter in manager.Parameters:
        name = kind(parameter)
        item = {"name": text(parameter.Definition.Name), "kind": name, "instance": bool(parameter.IsInstance),
                "reporting": bool(parameter.IsReporting), "group": group(parameter)}
        if parameter.IsShared:
            item["shared"] = text(parameter.GUID)
        if parameter.Formula:
            item["formula"] = text(parameter.Formula)
        if name == "other":
            item["unit"] = unit(parameter)
        listed.append((parameter, item))
    return listed


def options(owner):
    """The family's own options, read from its built-in parameters."""
    found = {}
    for key, builtin in [("work_plane_based", "FAMILY_WORK_PLANE_BASED"), ("always_vertical", "FAMILY_ALWAYS_VERTICAL"),
                         ("shared", "FAMILY_SHARED"), ("cut_with_voids_when_loaded", "FAMILY_ALLOW_CUT_WITH_VOIDS")]:
        try:
            parameter = owner.get_Parameter(getattr(DB.BuiltInParameter, builtin))
            if parameter is not None and parameter.HasValue:
                found[key] = parameter.AsInteger() == 1
        except Exception:
            pass
    return found


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
    listed = parameters(manager)
    snapshot = {"name": text(owner.Name), "category": category(owner), "options": options(owner),
                "parameters": [item for _, item in listed], "types": [], "notes": []}

    def values(family_type):
        found = {}
        for parameter, item in listed:
            got = value(document, family_type, parameter, item["kind"])
            if got is not None:
                found[item["name"]] = got
        return found

    transaction = DB.Transaction(document, "BIMCraft temporary type evaluation")
    try:
        if transaction.Start() != DB.TransactionStatus.Started:
            raise ValueError("could not start temporary type evaluation")
        if types:
            for current in types:
                manager.CurrentType = current
                document.Regenerate()
                snapshot["types"].append({"name": text(current.Name), "values": values(current),
                                          "triangles": family_geometry(document)})
        else:
            snapshot["types"].append({"name": "Default", "values": {}, "triangles": family_geometry(document)})
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
    forms.alert("Exported %s families; %s failures. Read report.json.\n%s" %
                (len(report["exported"]), len(report["failed"]), destination))


if __name__ == "__main__":
    export_active()
