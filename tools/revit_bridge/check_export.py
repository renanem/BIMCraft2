"""Load what the bridge exported into BIMCraft and say what came in and what was left out (standard library only).

    python check_export.py EXPORT_FOLDER [--cli PATH\\to\\bimcraft-cli.exe]

EXPORT_FOLDER is a bundle the bridge wrote, or a folder of bundles (headless.py's `out`). Every template file
(`*.rte.json`) is opened as a project and saved as a `.bimt` beside it, and a new project is started from that `.bimt`;
a bundle of one family has its family loaded and a component of its first type placed. Nothing but those `.bimt` files
and `check.json` is written. The summary is printed and kept in `check.json`: for each file what came in, every note of
what was left out, and whether BIMCraft took it at all.
"""
from __future__ import print_function

import argparse
import io
import json
import os
import subprocess
import sys


def run(cli, arguments):
    """bimcraft-cli's replies, one JSON object a line, or the error it stopped on."""
    done = subprocess.run([cli] + arguments, capture_output=True, text=True, encoding="utf-8")
    replies = []
    for line in done.stdout.splitlines():
        try:
            replies.append(json.loads(line))
        except ValueError:
            pass
    return replies, (done.stderr.strip() or None) if done.returncode != 0 else None


def bundles(folder):
    """The bundles under `folder`: each folder that holds a report.json."""
    found = []
    for root, _, files in os.walk(folder):
        if "report.json" in files:
            found.append(root)
    return sorted(found)


def check_bundle(cli, bundle):
    with io.open(os.path.join(bundle, "report.json"), encoding="utf-8") as handle:
        report = json.load(handle)
    result = {"bundle": bundle, "source": report.get("source"), "bridge_failed": report.get("failed", []),
              "bridge_notes": report.get("notes", []), "families_exported": len(report.get("exported", []))}
    template = os.path.join(bundle, report.get("template", ""))
    if report.get("template") and os.path.isfile(template):
        saved = template[:-len(".rte.json")] + ".bimt"
        if os.path.exists(saved):
            os.remove(saved)
        replies, error = run(cli, ["run", template, "--save", saved])
        opened = replies[0] if replies else {}
        result["template"] = {"file": template, "error": error, "came_in": (opened.get("skipped") or [None])[0],
                              "notes": (opened.get("skipped") or [])[1:], "elements": opened.get("elements")}
        if error is None:
            # The template saved is what a new project starts from.
            started, error = run(cli, ["run", "--cmd", "new " + json.dumps({"template": saved}), "--cmd", "inspect {}"])
            result["template"]["starts_a_project"] = error is None
            result["template"]["start_error"] = error
    # A bundle of one family: the family itself, placed.
    files = report.get("files", [])
    if len(files) == 1:
        path = os.path.join(bundle, files[0])
        replies, error = run(cli, ["run", path])
        loaded = replies[0] if replies else {}
        family = {"file": path, "error": error, "name": loaded.get("family"), "category": loaded.get("category"),
                  "types": loaded.get("types"), "parameters": loaded.get("parameters"), "forms": loaded.get("forms"),
                  "notes": loaded.get("skipped", [])}
        if error is None and loaded.get("family"):
            listed, _ = run(cli, ["run", path, "--cmd", "family " + json.dumps({"name": loaded["family"]})])
            types = sorted((listed[-1].get("values") or {}).keys()) if listed else []
            if types:
                place = "component " + json.dumps({"family": loaded["family"], "type": types[0], "at": [0, 0]})
                _, error = run(cli, ["run", path, "--cmd", place])
                family["places"] = error is None
                family["place_error"] = error
        result["family"] = family
    return result


def summary(results):
    lines = []
    for result in results:
        lines.append("== %s (%s)" % (os.path.basename(result["bundle"]), result.get("source")))
        if result["bridge_failed"]:
            lines.append("  the bridge refused %s families: %s" % (
                len(result["bridge_failed"]), "; ".join("%s (%s)" % (f.get("family"), f.get("reason")) for f in result["bridge_failed"][:5])))
        template = result.get("template")
        if template:
            if template["error"]:
                lines.append("  TEMPLATE NOT OPENED: %s" % template["error"])
            else:
                lines.append("  " + (template["came_in"] or "the template came in"))
                lines.append("  starts a new project: %s" % ("yes" if template.get("starts_a_project") else "NO: %s" % template.get("start_error")))
                for note in template["notes"]:
                    lines.append("    - " + note[:300])
        family = result.get("family")
        if family:
            if family["error"]:
                lines.append("  FAMILY NOT LOADED: %s" % family["error"])
            else:
                lines.append("  family %s: category %s, %s types, %s parameters, %s forms; places: %s" % (
                    family["name"], family["category"], family["types"], family["parameters"], family["forms"],
                    "yes" if family.get("places") else "NO: %s" % family.get("place_error")))
                for note in family["notes"][1:]:
                    lines.append("    - " + note[:300])
    return "\n".join(lines)


def main(arguments):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("folder")
    parser.add_argument("--cli", default=os.environ.get("BIMCRAFT_CLI", "bimcraft-cli"))
    options = parser.parse_args(arguments)
    # A path to the program is made whole, so it is found whatever the folder a check is run from.
    if os.path.isfile(options.cli):
        options.cli = os.path.abspath(options.cli)
    elif os.sep in options.cli or "/" in options.cli:
        print("There is no bimcraft-cli at %s: build it with `cargo build -p bimcraft-cli`" % options.cli)
        return 1
    found = bundles(options.folder)
    if not found:
        print("No bundle (a folder with report.json) under %s" % options.folder)
        return 1
    results = [check_bundle(options.cli, bundle) for bundle in found]
    with io.open(os.path.join(options.folder, "check.json"), "w", encoding="utf-8") as handle:
        handle.write(json.dumps(results, ensure_ascii=False, indent=2) + "\n")
    print(summary(results))
    failed = [r for r in results if (r.get("template") or {}).get("error") or (r.get("family") or {}).get("error")]
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
