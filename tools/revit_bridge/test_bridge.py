"""Synthetic fixtures only; no Revit installation or Autodesk assets required."""
import json
import math
import os
from pathlib import Path
import tempfile
import subprocess
import unittest

import bridge


def family():
    return {"name": "Socket", "category": "OST_ElectricalFixtures",
            "options": {"shared": True, "always_vertical": "yes"},
            "parameters": [
                {"name": "Plate Width", "kind": "length", "group": "dimensions"},
                {"name": "Half", "kind": "length", "formula": "Plate Width / 2"},
                {"name": "Mounting Height", "kind": "length", "instance": True},
                {"name": "Rating", "kind": "other", "unit": "A", "shared": "0f3c9a52-7d41-4e7a-9b1e-2c5d8e6f7a10"},
            ],
            "types": [
                {"name": "Small", "values": {"Plate Width": 80.0, "Half": 40.0, "Mounting Height": 300.0, "Rating": 10},
                 "triangles": [[[0, 0, 0], [100, 0, 0], [0, 100, 100]]]},
                {"name": "Large", "values": {"Plate Width": 120.0, "Half": 60.0, "Mounting Height": 300.0, "Rating": 20},
                 "triangles": [[[0, 0, 0], [200, 0, 0], [0, 200, 200]]]},
            ]}


def office():
    return {"title": "Office", "families": [family()],
            "levels": [{"name": "Ground", "elevation": 0}, {"name": "Roof", "elevation": 4000}],
            "phases": ["Survey", "Fit Out"],
            "line_patterns": [{"name": "Office Chain", "segments": [{"dash": 12.0}, {"space": 2.0}, "dot", {"space": 2.0}]}],
            "fill_patterns": [{"name": "Office Tile", "target": "model", "solid": False,
                               "lines": [{"angle": 90.0, "spacing": 600.0, "origin": [0.0, 0.0], "shift": 0.0, "dashes": [300.0, -300.0]}]}],
            "text_types": [{"name": "Office 2mm", "font": "Arial", "size": 2.0, "bold": True, "color": [200, 30, 30]}],
            "line_styles": [{"name": "Office Demolished", "pen": 3, "color": [255, 0, 0], "pattern": "Office Chain"}],
            "dimension_types": [{"name": "Office Dim", "font": "Arial", "text_size": 2.0, "line_pen": 2}],
            "object_styles": [{"category": "OST_Walls", "projection": 2, "cut": 5}, {"category": "OST_Views", "projection": 1}],
            "parameters": [{"name": "Fire Rating", "kind": "text", "guid": "7b1d2c3e-4f50-4a61-8b72-9c83ad94be05",
                            "scope": "type", "categories": ["OST_Walls", "OST_Doors"]}],
            "notes": ["A nested family could not be read"]}


class BridgeTests(unittest.TestCase):
    @unittest.skipUnless(os.environ.get("BIMCRAFT_CLI"), "set BIMCRAFT_CLI to test the native consumer")
    def test_native_cli_loads_places_and_reopens_both_types(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = bridge.write_bundle({"title": "Synthetic", "families": [family()]}, root / "exchange")
            exported = root / "exchange" / report["files"][0]
            native = root / "converted.bimc"
            result = subprocess.run([os.environ["BIMCRAFT_CLI"], "run", str(exported),
                                     "--cmd", 'component {"family":"Socket","type":"Small","at":[1000,1000]}',
                                     "--cmd", 'component {"family":"Socket","type":"Large","at":[2000,1000]}',
                                     "--save", str(native)], capture_output=True, text=True, encoding="utf-8", check=True)
            self.assertTrue(native.exists(), result.stdout)
            loaded = json.loads([line for line in result.stdout.splitlines() if '"family"' in line][0])
            self.assertEqual((loaded["family"], loaded["category"], loaded["types"]), ("Socket", "outlet", 2))
            reopened = subprocess.run([os.environ["BIMCRAFT_CLI"], "run", str(native), "--cmd", 'family {"name":"Socket"}'],
                                      capture_output=True, text=True, encoding="utf-8", check=True)
            reply = json.loads(reopened.stdout.splitlines()[-1])
            self.assertEqual(reply["values"]["Small"]["Imported Type"], 0)
            self.assertEqual(reply["values"]["Large"]["Imported Type"], 1)
            self.assertEqual(reply["values"]["Large"]["Half"], 60)
            parameters = dict((p["name"], p) for p in reply["family"]["parameters"])
            self.assertTrue(parameters["Mounting Height"]["instance"])
            self.assertEqual(parameters["Half"]["formula"], "Plate Width / 2")
            self.assertEqual(parameters["Rating"]["shared"], "0f3c9a52-7d41-4e7a-9b1e-2c5d8e6f7a10")

    def test_a_family_is_exported_with_its_parameters_types_values_and_geometry(self):
        exported = bridge.export_family(family())
        self.assertEqual((exported["format"], exported["version"]), ("bimcraft-revit-family", 1))
        self.assertEqual(exported["category"], "OST_ElectricalFixtures")
        self.assertEqual(exported["options"], {"shared": True})
        self.assertEqual([p["name"] for p in exported["parameters"]], ["Plate Width", "Half", "Mounting Height", "Rating"])
        self.assertEqual(exported["parameters"][1], {"name": "Half", "kind": "length", "formula": "Plate Width / 2"})
        self.assertEqual(exported["parameters"][2], {"name": "Mounting Height", "kind": "length", "instance": True})
        self.assertEqual(exported["parameters"][3]["unit"], "A")
        self.assertEqual([t["name"] for t in exported["types"]], ["Small", "Large"])
        self.assertEqual(exported["types"][1]["values"]["Plate Width"], 120.0)
        self.assertEqual(exported["types"][1]["triangles"][0][1], [200, 0, 0])

    def test_a_family_without_geometry_or_parameters_is_still_a_family(self):
        source = {"name": "Label", "types": [{"name": "Default"}]}
        exported = bridge.export_family(source)
        self.assertEqual(exported["types"], [{"name": "Default", "values": {}, "triangles": []}])
        self.assertEqual(exported["parameters"], [])

    def test_refuses_nonfinite_numbers_unknown_kinds_and_duplicates(self):
        for number in [math.nan, math.inf, -math.inf, 1e12]:
            source = family()
            source["types"][0]["triangles"][0][0][0] = number
            with self.assertRaises(ValueError):
                bridge.export_family(source)
        for number in [math.nan, math.inf]:
            source = family()
            source["types"][0]["values"]["Plate Width"] = number
            with self.assertRaises(ValueError):
                bridge.export_family(source)
        source = family()
        source["parameters"][0]["kind"] = "flavour"
        with self.assertRaises(ValueError):
            bridge.export_family(source)
        source = family()
        source["parameters"][1]["name"] = "Plate Width"
        with self.assertRaises(ValueError):
            bridge.export_family(source)
        source = family()
        source["types"][1]["name"] = "Small"
        with self.assertRaises(ValueError):
            bridge.export_family(source)
        source = family()
        source["types"] = []
        with self.assertRaises(ValueError):
            bridge.export_family(source)

    @unittest.skipUnless(os.environ.get("BIMCRAFT_CLI"), "set BIMCRAFT_CLI to test the native consumer")
    def test_native_cli_opens_the_template_as_a_project_and_saves_it_as_a_bimt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = bridge.write_bundle(office(), root / "exchange")
            saved = root / "Office.bimt"
            result = subprocess.run([os.environ["BIMCRAFT_CLI"], "run", str(root / "exchange" / report["template"]),
                                     "--cmd", "line_pattern {}", "--cmd", "phase {}", "--save", str(saved)],
                                    capture_output=True, text=True, encoding="utf-8", check=True)
            self.assertTrue(saved.exists(), result.stdout)
            opened = json.loads(result.stdout.splitlines()[0])
            self.assertEqual(opened["name"], "Office")
            self.assertTrue(opened["skipped"][0].startswith("Came in from the bridge's export: 2 levels, 2 phases, 1 line pattern"))
            self.assertIn("Office Chain", result.stdout)
            self.assertIn("Fit Out", result.stdout)
            self.assertIn("1 line style", opened["skipped"][0])
            self.assertIn("1 dimension type, 1 object style, 1 parameter and 1 family", opened["skipped"][0])
            started = subprocess.run([os.environ["BIMCRAFT_CLI"], "run", "--cmd", 'new {"template": %s}' % json.dumps(str(saved)),
                                      "--cmd", 'component {"family":"Socket","type":"Large","at":[0,0]}', "--cmd", "text_type {}"],
                                     capture_output=True, text=True, encoding="utf-8", check=True)
            self.assertIn("Office 2mm", started.stdout)

    def test_a_template_is_exported_with_its_settings_and_the_names_of_its_families_files(self):
        template = bridge.export_template(office(), ["families/0000_Socket.rfa.json"])
        self.assertEqual((template["format"], template["version"], template["name"]), ("bimcraft-revit-template", 1, "Office"))
        self.assertEqual(template["levels"], [{"name": "Ground", "elevation": 0}, {"name": "Roof", "elevation": 4000}])
        self.assertEqual(template["phases"], ["Survey", "Fit Out"])
        self.assertEqual(template["line_patterns"][0]["segments"], [{"dash": 12.0}, {"space": 2.0}, "dot", {"space": 2.0}])
        self.assertEqual(template["fill_patterns"][0]["lines"][0]["dashes"], [300.0, -300.0])
        self.assertEqual(template["text_types"][0]["color"], [200, 30, 30])
        self.assertEqual(template["families"], ["families/0000_Socket.rfa.json"])
        self.assertEqual(template["line_styles"][0]["pattern"], "Office Chain")
        self.assertEqual(template["dimension_types"][0]["line_pen"], 2)
        self.assertEqual([style["category"] for style in template["object_styles"]], ["OST_Walls", "OST_Views"])
        self.assertEqual(template["parameters"][0]["guid"], "7b1d2c3e-4f50-4a61-8b72-9c83ad94be05")
        # What no file should hold is dropped with a note, and the rest stays.
        odd = office()
        odd["levels"].append({"name": "Sky", "elevation": math.inf})
        odd["line_patterns"].append({"name": "", "segments": []})
        odd["fill_patterns"].append({"name": "Bad", "lines": [{"angle": math.nan}]})
        odd["phases"].append(7)
        odd["text_types"] = "none"
        template = bridge.export_template(odd, [])
        self.assertEqual(len(template["levels"]), 2)
        self.assertEqual(len(template["line_patterns"]), 1)
        self.assertEqual(len(template["fill_patterns"]), 1)
        self.assertEqual(template["phases"], ["Survey", "Fit Out"])
        self.assertEqual(template["text_types"], [])
        self.assertEqual(len(template["notes"]), 4)

    def test_bundle_holds_a_file_for_each_family_the_template_and_a_report_of_loss_and_failures(self):
        snapshot = office()
        snapshot["families"] = [family(), {"name": "Broken"}, "odd"]
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "exchange"
            bridge.write_bundle(snapshot, output)
            files = sorted(p.name for p in (output / "families").iterdir())
            self.assertEqual(files, ["0000_Socket.rfa.json"])
            written = json.loads((output / "families" / files[0]).read_text(encoding="utf-8"))
            self.assertEqual(written, bridge.export_family(family()))
            report = json.loads((output / "report.json").read_text(encoding="utf-8"))
            self.assertEqual(report["template"], "Office.rte.json")
            template = json.loads((output / "Office.rte.json").read_text(encoding="utf-8"))
            self.assertEqual(template["families"], ["families/0000_Socket.rfa.json"])
            self.assertEqual(len(template["levels"]), 2)
            self.assertFalse((output / "project.txt").exists())
            self.assertEqual(report["exported"], ["Socket"])
            self.assertEqual(report["files"], ["families/0000_Socket.rfa.json"])
            self.assertEqual(len(report["failed"]), 2)
            self.assertTrue(report["limitations"])
            self.assertIn("A nested family could not be read", report["notes"])
            with self.assertRaises(FileExistsError):
                bridge.write_bundle(snapshot, output)


if __name__ == "__main__":
    unittest.main()
