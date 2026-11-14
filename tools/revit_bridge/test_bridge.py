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


class BridgeTests(unittest.TestCase):
    @unittest.skipUnless(os.environ.get("BIMCRAFT_CLI"), "set BIMCRAFT_CLI to test the native consumer")
    def test_native_cli_loads_places_and_reopens_both_types(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = bridge.write_bundle({"title": "Synthetic", "families": [family()]}, root / "exchange")
            exported = root / "exchange" / report["files"][0]
            native = root / "converted.bimc"
            result = subprocess.run([os.environ["BIMCRAFT_CLI"], "run", "--script", str(root / "exchange" / "project.txt"),
                                     str(exported),
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

    def test_bundle_holds_a_file_for_each_family_the_levels_and_a_report_of_loss_and_failures(self):
        snapshot = {"title": "Office", "families": [family(), {"name": "Broken"}, "odd"],
                    "levels": [{"name": "Ground", "elevation": 0}, {"name": "Roof", "elevation": 4000}],
                    "notes": ["A nested family could not be read"]}
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "exchange"
            bridge.write_bundle(snapshot, output)
            files = sorted(p.name for p in (output / "families").iterdir())
            self.assertEqual(files, ["0000_Socket.rfa.json"])
            written = json.loads((output / "families" / files[0]).read_text(encoding="utf-8"))
            self.assertEqual(written, bridge.export_family(family()))
            project = (output / "project.txt").read_text(encoding="utf-8").splitlines()
            self.assertTrue(project[0].startswith("new "))
            self.assertEqual(len(project), 3)
            self.assertNotIn("family ", "\n".join(project))
            report = json.loads((output / "report.json").read_text(encoding="utf-8"))
            self.assertEqual(report["exported"], ["Socket"])
            self.assertEqual(report["files"], ["families/0000_Socket.rfa.json"])
            self.assertEqual(len(report["failed"]), 2)
            self.assertTrue(report["limitations"])
            self.assertIn("A nested family could not be read", report["notes"])
            with self.assertRaises(FileExistsError):
                bridge.write_bundle(snapshot, output)


if __name__ == "__main__":
    unittest.main()
