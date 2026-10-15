#!/usr/bin/env python3
"""An example Argus extension: Docker containers as a view, a key that
restarts the selected one after asking, and a command that picks a compose
service to start.

Declare it in projects.toml and the daemon runs it, handing it the URL and
token it publishes with:

    [[extension]]
    name = "docker"
    command = ["python3", "/path/to/argus/scripts/docker-extension.py"]

On Windows the command is usually ["python", ...]. It uses only the
standard library and the `docker` CLI. The view shows in the desktop
window's views panel, beside the views agents show.
"""

import json
import os
import subprocess
import threading
import time
import urllib.error
import urllib.request

BASE = os.environ["ARGUS_HOOK_URL"]
TOKEN = os.environ["ARGUS_HOOK_TOKEN"]
POLL_SECONDS = 5

TONES = {"running": "good", "restarting": "warn", "paused": "info", "exited": "bad", "dead": "bad"}


def send(method, endpoint, body=None):
    """One request to this extension's endpoint; the answer's JSON, if any."""
    data = None if body is None else (body if isinstance(body, bytes) else json.dumps(body).encode())
    request = urllib.request.Request(f"{BASE}/{endpoint}", data=data, method=method)
    request.add_header("Authorization", f"Bearer {TOKEN}")
    try:
        with urllib.request.urlopen(request, timeout=10) as answer:
            text = answer.read().decode()
            return json.loads(text) if text.strip() else None
    except urllib.error.HTTPError as refused:
        print(f"{method} {endpoint} refused: {refused.code} {refused.read().decode()}", flush=True)
        return None


def notice(text):
    send("POST", "notice", text.encode())


def docker(*args, cwd=None):
    """What a docker command printed, or None when it failed."""
    try:
        done = subprocess.run(["docker", *args], capture_output=True, text=True, cwd=cwd, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as failed:
        print(f"docker {' '.join(args)}: {failed}", flush=True)
        return None
    if done.returncode != 0:
        print(f"docker {' '.join(args)}: {done.stderr.strip()}", flush=True)
        return None
    return done.stdout


def text(value):
    return {"kind": "text", "text": value}


def containers():
    out = docker("ps", "--all", "--format", "{{json .}}")
    if out is None:
        return None
    return [json.loads(line) for line in out.splitlines() if line.strip()]


def view(listed, title="Containers"):
    rows = [
        {
            "key": c["Names"],
            "cells": [text(c["Names"]), text(c["Image"]), text(c["Status"]), text(c.get("Ports") or "")],
            "status": {"label": c["State"], "tone": TONES.get(c["State"], "neutral")},
        }
        for c in listed
    ]
    return {
        "name": "containers",
        "title": title,
        "columns": [{"title": "Name"}, {"title": "Image"}, {"title": "Status"}, {"title": "Ports"}],
        "rows": rows[:200],
        "actions": [
            {"id": "restart", "label": "Restart", "key": "r", "confirm": "Restart this container?"},
            {"id": "stop", "label": "Stop", "key": "s", "confirm": "Stop this container?"},
            {"id": "logs", "label": "Logs", "key": "o"},
            {"id": "refresh", "label": "Refresh", "view_wide": True},
        ],
    }


published = None


def publish(force=False):
    """Puts the view again when what docker says changed: the window shows
    only what this says, so the view moves when this republishes."""
    global published
    listed = containers()
    model = view(listed or [], "Containers" if listed is not None else "Containers (docker is not answering)")
    if force or model != published:
        published = model
        send("PUT", "view", model)


def act(event):
    name, action = event.get("row"), event["action"]
    if action == "refresh":
        publish(force=True)
    elif action in ("restart", "stop"):
        if docker(action, name) is not None:
            notice(f"{name}: {action}ped" if action == "stop" else f"{name}: restarted")
        publish()
    elif action == "logs":
        send("POST", "pane", {"run": f"docker logs -f {name}"})


def services(context):
    """The compose services in the checkout the person was at."""
    checkout = (context or {}).get("checkout") or {}
    folder = checkout.get("path")
    out = docker("compose", "config", "--services", cwd=folder) if folder else None
    return folder, [s for s in (out or "").splitlines() if s.strip()]


asked = {}


def command(event):
    if event["command"] == "refresh":
        publish(force=True)
        return
    folder, found = services(event.get("context"))
    if not found:
        notice("no compose services in this checkout")
        return
    answer = send("POST", "dialog", {
        "title": "Start a compose service",
        "answers": event["id"],
        "kind": {"pick": {"message": f"In {folder}", "items": [{"key": s, "label": s} for s in found]}},
    })
    if answer:
        asked[answer["dialog"]] = folder


def answered(event):
    folder = asked.pop(event["dialog"], None)
    picked = (event.get("answer") or {}).get("picked")
    if folder and picked and docker("compose", "up", "-d", picked, cwd=folder) is not None:
        notice(f"{picked} started")
        publish()


def listen():
    """Reads what the person does, opening the stream again whenever it
    closes. A daemon gone for good is not coming back on this port and
    token, so after a while without one this exits; the next daemon starts
    the extension afresh."""
    failures = 0
    while failures < 30:
        request = urllib.request.Request(f"{BASE}/events")
        request.add_header("Authorization", f"Bearer {TOKEN}")
        try:
            with urllib.request.urlopen(request) as stream:
                failures = 0
                for raw in stream:
                    line = raw.decode().strip()
                    if not line.startswith("data: "):
                        continue
                    event = json.loads(line[len("data: "):])
                    kind = event["event"]
                    if kind == "action":
                        act(event)
                    elif kind == "command":
                        command(event)
                    elif kind == "answer":
                        answered(event)
                    elif kind == "cancel":
                        asked.pop(event["dialog"], None)
        except (urllib.error.URLError, OSError) as failed:
            failures += 1
            print(f"events: {failed}", flush=True)
        time.sleep(1)
    os._exit(1)


def main():
    refused = send("PUT", "commands", [
        {"name": "up", "title": "Start a compose service…", "key": {"key": "s", "alt": True}, "scope": "checkout"},
        {"name": "refresh", "title": "Refresh the containers"},
    ])
    for each in (refused or {}).get("refused", []):
        print(f"no key for {each['name']}: {each['reason']}", flush=True)
    threading.Thread(target=listen, daemon=True).start()
    while True:
        publish()
        time.sleep(POLL_SECONDS)


if __name__ == "__main__":
    main()
