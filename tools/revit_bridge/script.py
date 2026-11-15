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


def definition_kind(definition):
    """BIMCraft's kind for a definition's data type, a public SpecTypeId; None for those told no further by it."""
    try:
        spec = definition.GetDataType()
    except Exception:
        return None
    S = DB.SpecTypeId
    for known, name in [(S.Length, "length"), (S.Area, "area"), (S.Volume, "volume"), (S.Angle, "angle"),
                        (S.Number, "number"), (S.Int.Integer, "integer"), (S.Boolean.YesNo, "yesno"),
                        (S.String.Text, "text"), (S.String.MultilineText, "text"), (S.String.Url, "url"),
                        (S.Reference.Material, "material")]:
        if same(spec, known):
            return name
    return None


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
    if parameter.StorageType in (DB.StorageType.String, DB.StorageType.ElementId):
        # Words, or what names something of the document (an image, a nested family's type): its name, as text.
        return "text"
    # Revit has many data types measured as a length is (a bar's diameter, a section's dimension, a pipe's size): what a
    # parameter is here is told by the unit it is measured in, as BIMCraft has one kind for each.
    try:
        own = parameter.GetUnitTypeId()
        for known, name in [(S.Length, "length"), (S.Area, "area"), (S.Volume, "volume"), (S.Angle, "angle")]:
            if DB.UnitUtils.IsValidUnit(known, own):
                return name
    except Exception:
        pass
    return "other"


def group(parameter):
    """The heading a parameter is listed under, a public GroupTypeId, as BIMCraft names its own."""
    return definition_group(parameter.Definition)


def definition_group(definition):
    try:
        own = definition.GetGroupTypeId()
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


def tidy(number):
    """A converted number without the last bits a multiplication by 304.8 leaves: 240, not 239.99999999999997."""
    return round(number, 9)


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
                return tidy(number * FACTORS[name])
            if name == "other":
                try:
                    return tidy(DB.UnitUtils.ConvertFromInternalUnits(number, parameter.GetUnitTypeId()))
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


def is_host(document, item):
    """Whether an element is the template's own host: the solid a face-based or hosted family is drawn on, which is no part
    of the family and the only form Revit does not let the user delete."""
    try:
        return not DB.DocumentValidation.CanDeleteElement(document, item.Id)
    except Exception:
        return False


def family_geometry(document):
    """The triangles of what the family shows: its solid forms, its joined forms and its nested instances, never its host."""
    options = DB.Options()
    options.DetailLevel = DB.ViewDetailLevel.Fine
    options.IncludeNonVisibleObjects = False
    output = []
    for item in DB.FilteredElementCollector(document).WhereElementIsNotElementType():
        if isinstance(item, DB.GenericForm):
            # A form joined to others has no geometry of its own: its combination holds it.
            if item.IsSolid and not is_host(document, item):
                triangles(item.get_Geometry(options), output)
        elif isinstance(item, (DB.GeomCombination, DB.FamilyInstance)):
            if not is_host(document, item):
                triangles(item.get_Geometry(options), output)
    return output


def family_name(document):
    """The family's name: a family document opened from its file gives its owner family none, so its title names it."""
    name = text(document.OwnerFamily.Name or "").strip()
    if name:
        return name
    title = text(document.Title or "").strip()
    return title[:-4] if title.lower().endswith(".rfa") else title


def read_family(document, name=None):
    owner = document.OwnerFamily
    manager = document.FamilyManager
    types = list(manager.Types)
    if len(types) > bridge.MAX_TYPES:
        raise ValueError("more than %s family types" % bridge.MAX_TYPES)
    listed = parameters(manager)
    snapshot = {"name": name or family_name(document), "category": category(owner), "options": options(owner),
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
                # A family that names no type has one of a blank name: loaded, Revit names it after the family.
                type_name = text(current.Name)
                if not type_name.strip():
                    type_name = snapshot["name"]
                snapshot["types"].append({"name": type_name, "values": values(current),
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


def element_name(element):
    """An element's name: some engines hide the property on a subclass, so the base class is asked."""
    try:
        return text(element.Name)
    except Exception:
        pass
    try:
        # IronPython's way to a property a subclass hides.
        return text(DB.Element.Name.__get__(element))
    except Exception:
        # A type's name is also a parameter of it.
        return text(element.get_Parameter(DB.BuiltInParameter.SYMBOL_NAME_PARAM).AsString())


def phases(document):
    """The project's phases, in order."""
    return [element_name(phase) for phase in document.Phases]


def line_patterns(document, notes):
    """Each line pattern as its segments: a dash, a space (mm on paper) or a dot."""
    found = []
    for element in DB.FilteredElementCollector(document).OfClass(DB.LinePatternElement):
        try:
            pattern = element.GetLinePattern()
            segments = []
            for segment in pattern.GetSegments():
                kind = text(segment.Type)
                if kind == "Dot":
                    segments.append("dot")
                elif kind in ("Dash", "Space"):
                    segments.append({kind.lower(): tidy(segment.Length * 304.8)})
            found.append({"name": text(pattern.Name), "segments": segments})
        except Exception as error:
            notes.append("Line pattern %s not exported: %s" % (element_id(element), error))
    return found


def fill_patterns(document, notes):
    """Each fill pattern as its families of lines: angle in degrees, the rest in mm (on paper, or in the model)."""
    found = []
    for element in DB.FilteredElementCollector(document).OfClass(DB.FillPatternElement):
        try:
            pattern = element.GetFillPattern()
            lines = []
            for grid in pattern.GetFillGrids():
                # Revit steps to the next line on either side: a negative step with its shift is the same lines as the
                # positive step with the shift turned, which is how BIMCraft says them.
                turn = -1 if grid.Offset < 0 else 1
                line = {"angle": tidy(grid.Angle * 57.29577951308232), "spacing": tidy(turn * grid.Offset * 304.8),
                        "origin": [tidy(grid.Origin.U * 304.8), tidy(grid.Origin.V * 304.8)],
                        "shift": tidy(turn * grid.Shift * 304.8)}
                # Revit lists a dash, then a space, and so on: BIMCraft writes a space as a negative length.
                dashes = [tidy(length * 304.8) * (1 if index % 2 == 0 else -1) for index, length in enumerate(grid.GetSegments())]
                if dashes:
                    line["dashes"] = dashes
                lines.append(line)
            target = "model" if text(pattern.Target) == "Model" else "drafting"
            found.append({"name": text(pattern.Name), "target": target, "solid": bool(pattern.IsSolidFill), "lines": lines})
        except Exception as error:
            notes.append("Fill pattern %s not exported: %s" % (element_id(element), error))
    return found


def text_types(document, notes):
    """Each text note type: its font, its size on paper in mm, its style, its colour and its border."""
    P = DB.BuiltInParameter

    def read(element, builtin, how):
        parameter = element.get_Parameter(builtin)
        if parameter is None or not parameter.HasValue:
            return None
        return how(parameter)

    found = []
    for element in DB.FilteredElementCollector(document).OfClass(DB.TextNoteType):
        try:
            item = {"name": element_name(element)}
            item["font"] = read(element, P.TEXT_FONT, lambda p: text(p.AsString()))
            size = read(element, P.TEXT_SIZE, lambda p: p.AsDouble())
            item["size"] = tidy(size * 304.8) if size is not None else None
            item["bold"] = read(element, P.TEXT_STYLE_BOLD, lambda p: p.AsInteger() == 1)
            item["italic"] = read(element, P.TEXT_STYLE_ITALIC, lambda p: p.AsInteger() == 1)
            item["underline"] = read(element, P.TEXT_STYLE_UNDERLINE, lambda p: p.AsInteger() == 1)
            item["width_factor"] = read(element, P.TEXT_WIDTH_SCALE, lambda p: tidy(p.AsDouble()))
            colour = read(element, P.LINE_COLOR, lambda p: p.AsInteger())
            if colour is not None:
                # Revit holds a colour as one whole number, blue highest.
                item["color"] = [colour & 255, (colour >> 8) & 255, (colour >> 16) & 255]
            item["border"] = read(element, P.TEXT_BOX_VISIBILITY, lambda p: p.AsInteger() == 1)
            offset = read(element, P.LEADER_OFFSET_SHEET, lambda p: p.AsDouble())
            item["border_offset"] = tidy(offset * 304.8) if offset is not None else None
            found.append(dict((key, value) for key, value in item.items() if value is not None))
        except Exception as error:
            notes.append("Text type %s not exported: %s" % (element_id(element), error))
    return found


def colour(value):
    """A Revit colour as [red, green, blue], or None where it holds none."""
    try:
        if value is not None and value.IsValid:
            return [int(value.Red), int(value.Green), int(value.Blue)]
    except Exception:
        pass
    return None


def pen(category, style):
    """A category's line weight where it is seen or cut, 1 to 16, or None where it has none."""
    try:
        weight = category.GetLineWeight(style)
        return int(weight) if weight is not None else None
    except Exception:
        return None


def line_styles(document, notes):
    """Each line style, a subcategory of Lines: its pen, its colour and its line pattern's name."""
    found = []
    lines = document.Settings.Categories.get_Item(DB.BuiltInCategory.OST_Lines)
    for style in lines.SubCategories:
        try:
            item = {"name": text(style.Name), "pen": pen(style, DB.GraphicsStyleType.Projection), "color": colour(style.LineColor)}
            pattern = document.GetElement(style.GetLinePatternId(DB.GraphicsStyleType.Projection))
            if isinstance(pattern, DB.LinePatternElement):
                item["pattern"] = text(pattern.GetLinePattern().Name)
            found.append(dict((key, value) for key, value in item.items() if value is not None))
        except Exception as error:
            notes.append("A line style was not exported: %s" % error)
    return found


def object_styles(document, notes):
    """Each model category's pens, seen and cut, under its public BuiltInCategory name."""
    found = []
    for category in document.Settings.Categories:
        try:
            if category.CategoryType != DB.CategoryType.Model:
                continue
            item = {"category": text(category.BuiltInCategory),
                    "projection": pen(category, DB.GraphicsStyleType.Projection), "cut": pen(category, DB.GraphicsStyleType.Cut)}
            if item["projection"] is not None or item["cut"] is not None:
                found.append(item)
        except Exception as error:
            notes.append("An object style was not exported: %s" % error)
    return found


def dimension_types(document, notes):
    """Each dimension type's text and lines, lengths in mm on paper. Its tick mark is not read."""
    P = DB.BuiltInParameter

    def read(element, builtin, how):
        parameter = element.get_Parameter(builtin)
        if parameter is None or not parameter.HasValue:
            return None
        return how(parameter)

    def length(element, builtin):
        value = read(element, builtin, lambda p: p.AsDouble())
        return tidy(value * 304.8) if value is not None else None

    found = []
    for element in DB.FilteredElementCollector(document).OfClass(DB.DimensionType):
        try:
            name = element_name(element)
            if not name.strip():
                continue
            item = {"name": name, "font": read(element, P.TEXT_FONT, lambda p: text(p.AsString())),
                    "text_size": length(element, P.TEXT_SIZE),
                    "bold": read(element, P.TEXT_STYLE_BOLD, lambda p: p.AsInteger() == 1),
                    "italic": read(element, P.TEXT_STYLE_ITALIC, lambda p: p.AsInteger() == 1),
                    "underline": read(element, P.TEXT_STYLE_UNDERLINE, lambda p: p.AsInteger() == 1),
                    "width_factor": read(element, P.TEXT_WIDTH_SCALE, lambda p: tidy(p.AsDouble())),
                    "line_pen": read(element, P.LINE_PEN, lambda p: p.AsInteger()),
                    "text_offset": length(element, P.TEXT_DIST_TO_LINE),
                    "witness_gap": length(element, P.WITNS_LINE_GAP_TO_ELT),
                    "witness_extension": length(element, P.WITNS_LINE_EXTENSION),
                    "line_extension": length(element, P.DIM_LINE_EXTENSION)}
            value = read(element, P.LINE_COLOR, lambda p: p.AsInteger())
            if value is not None:
                item["color"] = [value & 255, (value >> 8) & 255, (value >> 16) & 255]
            found.append(dict((key, value) for key, value in item.items() if value is not None))
        except Exception as error:
            notes.append("Dimension type %s not exported: %s" % (element_id(element), error))
    return found


def project_parameters(document, notes):
    """Each project parameter bound to categories: its kind, a shared one's GUID, instance or type, group, categories."""
    found = []
    iterator = document.ParameterBindings.ForwardIterator()
    iterator.Reset()
    while iterator.MoveNext():
        try:
            definition, binding = iterator.Key, iterator.Current
            item = {"name": text(definition.Name), "kind": definition_kind(definition) or "other",
                    "scope": "type" if isinstance(binding, DB.TypeBinding) else "instance",
                    "group": definition_group(definition), "categories": []}
            element = document.GetElement(definition.Id)
            if isinstance(element, DB.SharedParameterElement):
                item["guid"] = text(element.GuidValue)
            for category in binding.Categories:
                try:
                    item["categories"].append(text(category.BuiltInCategory))
                except Exception:
                    pass
            found.append(dict((key, value) for key, value in item.items() if value is not None))
        except Exception as error:
            notes.append("A project parameter was not exported: %s" % error)
    return found


def id_value(element_id_):
    """An ElementId's number: `Value` from Revit 2024, `IntegerValue` before."""
    try:
        return element_id_.Value
    except AttributeError:
        return element_id_.IntegerValue


def category_of(document, category_id):
    """The public BuiltInCategory name of the category an id names, or None."""
    try:
        category = DB.Category.GetCategory(document, category_id)
        return text(category.BuiltInCategory) if category is not None else None
    except Exception:
        return None


# A rule's evaluator, a public class of the API, as BIMCraft names the comparison; and what each is when inverted.
OPERATORS = {"FilterStringEquals": "equals", "FilterStringContains": "contains", "FilterStringBeginsWith": "begins-with",
             "FilterStringEndsWith": "ends-with", "FilterStringGreater": "greater",
             "FilterStringGreaterOrEqual": "greater-or-equal", "FilterStringLess": "less",
             "FilterStringLessOrEqual": "less-or-equal", "FilterNumericEquals": "equals",
             "FilterNumericGreater": "greater", "FilterNumericGreaterOrEqual": "greater-or-equal",
             "FilterNumericLess": "less", "FilterNumericLessOrEqual": "less-or-equal"}
INVERTED = {"equals": "not-equals", "contains": "not-contains", "begins-with": "not-begins-with",
            "ends-with": "not-ends-with", "greater": "less-or-equal", "greater-or-equal": "less",
            "less": "greater-or-equal", "less-or-equal": "greater", "has-value": "has-no-value",
            "has-no-value": "has-value"}


def rule_field(document, parameter_id):
    """The name a rule's parameter is shown by, its definition where it is a project or shared parameter, and its
    public BuiltInParameter name where it is a built-in one."""
    element = document.GetElement(parameter_id)
    if element is not None:
        try:
            return element_name(element), element.GetDefinition(), None
        except Exception:
            return element_name(element), None, None
    import System
    builtin = System.Enum.ToObject(DB.BuiltInParameter, id_value(parameter_id))
    # The label is in Revit's language; the enumeration's own name is the same in every one.
    return text(DB.LabelUtils.GetLabelFor(builtin)), None, text(builtin)


def filter_rule(document, rule):
    """One rule as {field, op, value}; `internal` marks a number still in Revit's internal units."""
    inverse = False
    if isinstance(rule, DB.FilterInverseRule):
        inverse = True
        rule = rule.GetInnerRule()
    field, definition, builtin = rule_field(document, rule.GetRuleParameter())
    item = {"field": field}
    if builtin:
        item["builtin"] = builtin
    if isinstance(rule, DB.HasValueFilterRule):
        item["op"] = "has-value"
    elif isinstance(rule, DB.HasNoValueFilterRule):
        item["op"] = "has-no-value"
    else:
        item["op"] = OPERATORS[type(rule.GetEvaluator()).__name__]
        if isinstance(rule, DB.FilterStringRule):
            item["value"] = text(rule.RuleString)
        elif isinstance(rule, DB.FilterDoubleRule):
            found = definition_kind(definition) if definition is not None else None
            if found in FACTORS:
                item["value"] = tidy(rule.RuleValue * FACTORS[found])
            elif found == "number":
                item["value"] = rule.RuleValue
            else:
                # A built-in parameter's, or a data type of its own: its unit is not known here.
                item["value"] = rule.RuleValue
                item["internal"] = True
        elif isinstance(rule, DB.FilterIntegerRule):
            item["value"] = int(rule.RuleValue)
        elif isinstance(rule, DB.FilterElementIdRule):
            # A type, a level, a material: its name.
            named = document.GetElement(rule.RuleValue)
            item["value"] = element_name(named) if named is not None else ""
    if inverse:
        item["op"] = INVERTED[item["op"]]
    return item


def filter_rules(document, element_filter):
    """A filter's rules and how they are joined: `and`, `or`, or `complex` for a tree BIMCraft has no way to say."""
    if element_filter is None:
        return "and", []
    if isinstance(element_filter, DB.ElementParameterFilter):
        return "and", [filter_rule(document, rule) for rule in element_filter.GetRules()]
    if isinstance(element_filter, DB.ElementLogicalFilter):
        parts = [filter_rules(document, part) for part in element_filter.GetFilters()]
        rules = [rule for _, found in parts for rule in found]
        if any(logic != "and" for logic, _ in parts):
            return "complex", rules
        if isinstance(element_filter, DB.LogicalOrFilter) and len(parts) > 1:
            return "or", rules
        return "and", rules
    return "complex", []


def view_filters(document, notes):
    """Each view filter: its categories, its rules and how they are joined."""
    found = []
    for element in DB.FilteredElementCollector(document).OfClass(DB.ParameterFilterElement):
        try:
            categories = [category_of(document, category_id) for category_id in element.GetCategories()]
            logic, rules = filter_rules(document, element.GetElementFilter())
            found.append({"name": element_name(element), "categories": [c for c in categories if c], "logic": logic, "rules": rules})
        except Exception as error:
            notes.append("View filter %s not exported: %s" % (element_id(element), error))
    return found


VIEW_KINDS = {"FloorPlan": "plan", "CeilingPlan": "plan", "EngineeringPlan": "plan", "AreaPlan": "plan",
              "Section": "section", "Elevation": "section", "Detail": "section", "ThreeD": "3d"}
VIEW_STYLES = {"Wireframe": "wireframe", "HLR": "hidden_line", "Shading": "shaded", "ShadingWithEdges": "shaded",
               "Realistic": "shaded", "RealisticWithEdges": "shaded", "FlatColors": "shaded"}


def view_templates(document, notes):
    """Each view template: the kind of view it is for, scale, detail, style, hidden categories, filters, phase, and what it controls."""
    P = DB.BuiltInParameter
    controlled = [(P.VIS_GRAPHICS_MODEL, "categories"), (P.VIS_GRAPHICS_FILTERS, "filters"), (P.PLAN_VIEW_RANGE, "range"),
                  (P.VIEW_PHASE, "phase"), (P.VIEW_SCALE, "scale"), (P.VIEW_DETAIL_LEVEL, "detail"),
                  (P.MODEL_GRAPHICS_STYLE, "style")]
    categories = list(document.Settings.Categories)
    found = []
    for view in DB.FilteredElementCollector(document).OfClass(DB.View):
        try:
            if not view.IsTemplate:
                continue
            kind = text(view.ViewType)
            item = {"name": element_name(view), "kind": VIEW_KINDS.get(kind, kind.lower()), "hide": [], "filters": []}
            try:
                if view.Scale > 0:
                    item["scale"] = int(view.Scale)
            except Exception:
                pass
            detail = text(view.DetailLevel).lower()
            if detail in ("coarse", "medium", "fine"):
                item["detail"] = detail
            style = VIEW_STYLES.get(text(view.DisplayStyle))
            if style:
                item["visual_style"] = style
            for category in categories:
                try:
                    if view.GetCategoryHidden(category.Id):
                        item["hide"].append(text(category.BuiltInCategory))
                except Exception:
                    pass
            try:
                for filter_id in view.GetFilters():
                    item["filters"].append({"filter": element_name(document.GetElement(filter_id)),
                                            "hidden": not view.GetFilterVisibility(filter_id)})
            except Exception:
                pass
            try:
                phase = document.GetElement(view.get_Parameter(P.VIEW_PHASE).AsElementId())
                if phase is not None:
                    item["phase"] = element_name(phase)
            except Exception:
                pass
            try:
                free = set(id_value(parameter_id) for parameter_id in view.GetNonControlledTemplateParameterIds())
                item["controls"] = [name for builtin, name in controlled if int(builtin) not in free]
            except Exception:
                pass
            found.append(item)
        except Exception as error:
            notes.append("View template %s not exported: %s" % (element_id(view), error))
    return found


def title_block(family, opened):
    """A title block family's lines and plain texts, mm on paper from its lower left corner. Its labels are not read."""
    lines, texts, xs, ys = [], [], [], []
    for curve in DB.FilteredElementCollector(opened).OfClass(DB.CurveElement):
        try:
            points = [[point.X * 304.8, point.Y * 304.8] for point in curve.GeometryCurve.Tessellate()]
            if len(points) >= 2:
                lines.append(points)
                xs.extend(point[0] for point in points)
                ys.extend(point[1] for point in points)
        except Exception:
            pass
    for note in DB.FilteredElementCollector(opened).OfClass(DB.TextNote):
        try:
            item = {"at": [note.Coord.X * 304.8, note.Coord.Y * 304.8], "text": text(note.Text).strip()}
            size = note.Symbol.get_Parameter(DB.BuiltInParameter.TEXT_SIZE)
            if size is not None and size.HasValue:
                item["size"] = tidy(size.AsDouble() * 304.8)
            item["align"] = {"Center": "center", "Right": "right"}.get(text(note.HorizontalAlignment), "left")
            if item["text"]:
                texts.append(item)
        except Exception:
            pass
    if not xs:
        raise ValueError("it has no lines")
    left, bottom = min(xs), min(ys)
    return {"name": text(family.Name), "width": tidy(max(xs) - left), "height": tidy(max(ys) - bottom),
            "lines": [{"points": [[tidy(x - left), tidy(y - bottom)] for x, y in points]} for points in lines],
            "texts": [dict(item, at=[tidy(item["at"][0] - left), tidy(item["at"][1] - bottom)]) for item in texts]}


def title_blocks(document, notes):
    """Each loaded title block family, read in its own document and closed unsaved."""
    found = []
    for family in DB.FilteredElementCollector(document).OfClass(DB.Family):
        opened = None
        try:
            if family.FamilyCategory is None or text(family.FamilyCategory.BuiltInCategory) != "OST_TitleBlocks":
                continue
            if not family.IsEditable:
                notes.append("Title block %s is not editable; not exported" % family.Name)
                continue
            opened = document.EditFamily(family)
            found.append(title_block(family, opened))
        except RuntimeError:
            raise
        except Exception as error:
            notes.append("Title block %s not exported: %s" % (element_id(family), error))
        finally:
            if opened is not None:
                opened.Close(False)
    return found


def sheets(document, notes):
    """Each sheet: its number, its name, and its size and title block from the title block placed on it."""
    P = DB.BuiltInParameter
    found = []
    for sheet in DB.FilteredElementCollector(document).OfClass(DB.ViewSheet):
        try:
            if sheet.IsPlaceholder or sheet.IsTemplate:
                continue
            item = {"number": text(sheet.SheetNumber), "name": element_name(sheet)}
            placed = DB.FilteredElementCollector(document, sheet.Id).OfCategory(DB.BuiltInCategory.OST_TitleBlocks)
            for block in placed.WhereElementIsNotElementType():
                width, height = block.get_Parameter(P.SHEET_WIDTH), block.get_Parameter(P.SHEET_HEIGHT)
                if width is not None and height is not None and width.HasValue and height.HasValue:
                    item["width"], item["height"] = tidy(width.AsDouble() * 304.8), tidy(height.AsDouble() * 304.8)
                try:
                    item["title_block"] = text(block.Symbol.FamilyName)
                except Exception:
                    pass
                break
            found.append(item)
        except Exception as error:
            notes.append("Sheet %s not exported: %s" % (element_id(sheet), error))
    return found


def read_document(document, limit=None, progress=None):
    """What the bridge exports of a document: the family it is, or a project's (or a template's) levels, phases, line and
    fill patterns, line styles, text and dimension types, object styles, project parameters, view filters, view templates,
    title blocks and sheets, and its loaded editable families
    (the first `limit` of them, when given), each opened with EditFamily and closed unsaved. `progress(text)` hears of each."""
    snapshot = {"title": text(document.Title), "families": [], "levels": [], "notes": []}
    if document.IsFamilyDocument:
        snapshot["families"].append(read_family(document))
        return snapshot
    snapshot["levels"] = [{"name": element_name(level), "elevation": tidy(level.ProjectElevation * 304.8)}
                          for level in DB.FilteredElementCollector(document).OfClass(DB.Level)]
    # The project's own settings, which make it a template: each read apart, so one that fails leaves the others.
    for key, reader in [("line_patterns", line_patterns), ("line_styles", line_styles), ("fill_patterns", fill_patterns),
                        ("text_types", text_types), ("dimension_types", dimension_types), ("object_styles", object_styles),
                        ("parameters", project_parameters), ("filters", view_filters),
                        ("view_templates", view_templates), ("title_blocks", title_blocks), ("sheets", sheets)]:
        try:
            snapshot[key] = reader(document, snapshot["notes"])
        except Exception as error:
            snapshot["notes"].append("%s not exported: %s" % (key, error))
    try:
        snapshot["phases"] = phases(document)
    except Exception as error:
        snapshot["notes"].append("phases not exported: %s" % error)
    if progress is not None:
        progress("settings: " + ", ".join("%s %s" % (len(snapshot.get(key, [])), key) for key in (
            "levels", "phases", "line_patterns", "line_styles", "fill_patterns", "text_types", "dimension_types",
            "object_styles", "parameters", "filters", "view_templates", "title_blocks", "sheets")))
    families = list(DB.FilteredElementCollector(document).OfClass(DB.Family))
    for family in families:
        if not family.IsEditable or family.IsInPlace:
            snapshot["notes"].append("Family %s (%s) is not editable or is in-place; not exported" % (family.Name, element_id(family)))
            continue
        if limit is not None and len(snapshot["families"]) >= limit:
            snapshot["notes"].append("Only the first %s editable families of %s were exported" % (limit, len(families)))
            break
        opened = None
        try:
            opened = document.EditFamily(family)
            # The project knows the family by its name; its own document may hold none.
            exported = read_family(opened, text(family.Name))
            snapshot["families"].append(exported)
            if progress is not None:
                progress("%s: %s parameters, %s types" % (exported["name"], len(exported["parameters"]), len(exported["types"])))
        except RuntimeError:
            raise
        except Exception as error:
            snapshot["notes"].append("Family %s (%s) not exported: %s" % (family.Name, element_id(family), error))
            if progress is not None:
                progress("%s not exported: %s" % (family.Name, error))
        finally:
            if opened is not None:
                opened.Close(False)
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
    report = bridge.write_bundle(read_document(document), destination)
    forms.alert("Exported %s families; %s failures. Read report.json.\n%s" %
                (len(report["exported"]), len(report["failed"]), destination))


if __name__ == "__main__":
    export_active()
