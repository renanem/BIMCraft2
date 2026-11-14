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
    return {"name": "Socket", "category": "outlet", "types": [
        {"name": "Small", "triangles": [[[0, 0, 0], [100, 0, 0], [0, 100, 100]]]},
        {"name": "Large", "triangles": [[[0, 0, 0], [200, 0, 0], [0, 200, 200]]]},
    ]}


class BridgeTests(unittest.TestCase):
    @unittest.skipUnless(os.environ.get("BIMCRAFT_CLI"), "set BIMCRAFT_CLI to test the native consumer")
    def test_native_cli_loads_places_and_reopens_both_types(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bridge.write_bundle({"title": "Synthetic", "families": [family()]}, root / "exchange")
            native = root / "converted.bimcraft"
            result = subprocess.run([os.environ["BIMCRAFT_CLI"], "run", "--script", str(root / "exchange" / "project.txt"),
                                     "--cmd", 'component {"family":"Socket","type":"Small","at":[1000,1000]}',
                                     "--cmd", 'component {"family":"Socket","type":"Large","at":[2000,1000]}',
                                     "--save", str(native)], capture_output=True, text=True, check=True)
            self.assertTrue(native.exists(), result.stdout)
            reopened = subprocess.run([os.environ["BIMCRAFT_CLI"], "run", str(native), "--cmd", 'family {"name":"Socket"}'],
                                      capture_output=True, text=True, check=True)
            reply = json.loads(reopened.stdout.splitlines()[-1])
            self.assertEqual(reply["values"]["Small"]["Imported Type"], 0)
            self.assertEqual(reply["values"]["Large"]["Imported Type"], 1)

    def test_types_keep_separate_geometry_and_are_native_family_files(self):
        converted = bridge.convert_family(family())
        self.assertEqual(set(converted["types"]), {"Small", "Large"})
        self.assertEqual(converted["category"], "outlet")
        self.assertNotEqual(converted["forms"][0]["visible"], converted["forms"][1]["visible"])
        self.assertEqual(converted["forms"][1]["mesh"][0][1], [200, 0, 0])

    def test_hosted_family_is_explicitly_generic_until_hosts_are_converted(self):
        source = family()
        source["category"] = "door"
        self.assertEqual(bridge.convert_family(source)["category"], "generic")

    def test_refuses_nonfinite_coordinates_empty_geometry_and_duplicate_types(self):
        for number in [math.nan, math.inf, -math.inf, 1e12]:
            source = family()
            source["types"][0]["triangles"][0][0][0] = number
            with self.assertRaises(ValueError):
                bridge.convert_family(source)
        source = family()
        source["types"][0]["triangles"] = []
        with self.assertRaises(ValueError):
            bridge.convert_family(source)
        source = family()
        source["types"][1]["name"] = "Small"
        with self.assertRaises(ValueError):
            bridge.convert_family(source)

    def test_bundle_is_self_contained_and_reports_loss_and_failures(self):
        snapshot = {"title": "Office", "families": [family(), {"name": "Broken"}],
                    "levels": [{"name": "Ground", "elevation": 0}, {"name": "Roof", "elevation": 4000}],
                    "notes": ["A nested family could not be read"]}
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "exchange"
            bridge.write_bundle(snapshot, output)
            commands = (output / "library.txt").read_text(encoding="utf-8").splitlines()
            self.assertTrue(any(line.startswith("family ") for line in commands))
            self.assertNotIn('"path":', "\n".join(commands))
            report = json.loads((output / "report.json").read_text(encoding="utf-8"))
            self.assertEqual(report["converted"], ["Socket"])
            self.assertEqual(len(report["failed"]), 1)
            self.assertTrue(report["limitations"])
            self.assertIn("A nested family could not be read", report["notes"])
            with self.assertRaises(FileExistsError):
                bridge.write_bundle(snapshot, output)


if __name__ == "__main__":
    unittest.main()
