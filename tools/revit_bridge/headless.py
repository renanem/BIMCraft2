# -*- coding: utf-8 -*-
"""Run the bridge inside Revit without its dialogs: `pyrevit run headless.py --revit=2027`.

`pyrevit run` starts a Revit of its own, runs this script and closes it. What to export is a job file, since no argument
reaches the script: `bimcraft-bridge-job.json` in the user's temporary folder,

    {"documents": ["C:\\\\path\\\\Template.rte", "C:\\\\path\\\\Project.rvt", "C:\\\\path\\\\Family.rfa"],
     "out": "C:\\\\path\\\\new-or-existing-folder", "limit": null}

Each document is opened in memory (a workshared one detached from its central model, so nothing reaches it), read by
script.py exactly as the button reads it, written as a bundle under `out` in a folder of its own, and closed unsaved.
`limit` is the most loaded families read of a project; null reads them all. Everything is logged to `out/bridge.log` as
it happens, with the time, so a run that stops says where.
"""
import imp
import io
import json
import os
import re
import tempfile
import time
import traceback

HERE = os.path.dirname(os.path.abspath(__file__))
JOB = os.path.join(tempfile.gettempdir(), "bimcraft-bridge-job.json")
started = time.time()
log_path = [None]


def say(text):
    if log_path[0] is None:
        return
    with io.open(log_path[0], "a", encoding="utf-8") as handle:
        handle.write(u"%7.1fs %s\n" % (time.time() - started, text))


def run():
    from pyrevit import HOST_APP, DB
    with io.open(JOB, encoding="utf-8") as handle:
        job = json.load(handle)
    out = job["out"]
    if not os.path.isdir(out):
        os.makedirs(out)
    log_path[0] = os.path.join(out, "bridge.log")
    say(u"Revit %s; job %s" % (HOST_APP.version, JOB))
    exporter = imp.load_source("bimcraft_bridge_script", os.path.join(HERE, "script.py"))
    import bridge
    for path in job.get("documents", []):
        document = None
        try:
            options = DB.OpenOptions()
            if DB.BasicFileInfo.Extract(path).IsWorkshared:
                options.DetachFromCentralOption = DB.DetachFromCentralOption.DetachAndPreserveWorksets
            document = HOST_APP.app.OpenDocumentFile(DB.ModelPathUtils.ConvertUserVisiblePathToModelPath(path), options)
            say(u"opened %s" % path)
            snapshot = exporter.read_document(document, job.get("limit"), say)
            # A bundle is never written over: each document has a folder of its own, numbered where one is there.
            stem = re.sub(r"[^\w.-]", "_", os.path.basename(path), flags=re.UNICODE)
            target, number = os.path.join(out, stem), 1
            while os.path.exists(target):
                number += 1
                target = os.path.join(out, "%s-%s" % (stem, number))
            report = bridge.write_bundle(snapshot, target)
            say(u"wrote %s: %s families exported, %s failed, %s notes" % (
                target, len(report["exported"]), len(report["failed"]), len(report["notes"])))
        except Exception:
            say(u"FAILED %s\n%s" % (path, traceback.format_exc()))
        finally:
            if document is not None:
                document.Close(False)
    say(u"done")


try:
    run()
except Exception:
    # No log folder yet, or the job could not be read: said beside the job file.
    with io.open(JOB + ".error.txt", "w", encoding="utf-8") as handle:
        handle.write(u"%s" % traceback.format_exc())
