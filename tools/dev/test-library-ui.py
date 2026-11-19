#!/usr/bin/env python3
"""WebKit UI regressions against Storybook fixtures; no real account or media.

Start Storybook, then run with a working DISPLAY (or under Xvfb).
Requires requests, WebKitWebDriver and WebKitGTK MiniBrowser.
"""
import argparse
import json
import socket
import subprocess
import tempfile
import time

import requests


def check_ui(storybook, driver_url, session):
    base = f"{driver_url}/session/{session}"

    def call(method, path, args=None):
        value = requests.request(method, base + path, json=args, timeout=30).json()["value"]
        if isinstance(value, dict) and "error" in value:
            raise AssertionError(value)
        return value

    def js(code):
        return call("POST", "/execute/sync", {"script": code, "args": []})

    def wait(code):
        for _ in range(150):
            result = js(code)
            if result:
                return result
            time.sleep(.1)
        raise AssertionError(js("return document.body.innerText.slice(0, 3000)"))

    def button(label, scope="document"):
        element = js(f"return [...{scope}.querySelectorAll('button')].find(b=>b.textContent.trim()==={json.dumps(label)})")
        assert element, label
        # Native clicks enforce modal input blocking; JS .click() bypasses it.
        call("POST", f"/element/{element['element-6066-11e4-a52e-4f735466cecf']}/click", {})

    def close_header(scope):
        element = js(f"return {scope}.querySelector(':scope > header button')")
        call("POST", f"/element/{element['element-6066-11e4-a52e-4f735466cecf']}/click", {})

    def open_story(story, extra=""):
        call("POST", "/url", {"url": f"{storybook}/iframe.html?id={story}&viewMode=story{extra}"})

    call("POST", "/window/rect", {"width": 1280, "height": 900})
    open_story("library-product-detail-dialog--downloaded-owned")
    wait("return !!document.querySelector('dialog[open] .copy-title')")
    js("document.querySelector('.copy-title').click()")
    wait("return !!document.querySelector('.toast-stack:popover-open .toast')")

    def toast_metrics():
        return js("""
          const stack=document.querySelector('.toast-stack'), dialog=stack.closest('dialog');
          const rect=stack.getBoundingClientRect(), header=dialog.querySelector(':scope > header').getBoundingClientRect();
          const close=stack.querySelector('.toast-close'), r=close.getBoundingClientRect();
          return {height:rect.height, top:rect.top, headerBottom:header.bottom,
            rows:[...stack.querySelectorAll('.toast')].map(n=>n.getBoundingClientRect().height),
            interactive:document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)?.closest('.toast-close')===close};
        """)

    metrics = toast_metrics()
    assert metrics["rows"][0] < 100 and metrics["height"] < 150, metrics
    assert metrics["top"] >= metrics["headerBottom"] and metrics["interactive"], metrics
    # Multiple rows, dialog scrolling and mobile sizing must stay compact.
    js("document.querySelector('.copy-title').click(); document.querySelector('.copy-title').click()")
    wait("return document.querySelectorAll('.toast').length === 3")
    assert max(toast_metrics()["rows"]) < 100
    js("document.querySelector('dialog').scrollTop=10000")
    call("POST", "/window/rect", {"width": 390, "height": 844})
    wait("return document.querySelector('.toast-stack').getBoundingClientRect().width < innerWidth")
    assert max(toast_metrics()["rows"]) < 160, toast_metrics()
    js("document.querySelectorAll('.toast-close').forEach(b=>b.click())")
    wait("return !document.querySelector('.toast-stack')")

    call("POST", "/window/rect", {"width": 1280, "height": 900})
    open_story("app-production-workspace--interactive", "&localFiles")
    wait("return [...document.querySelectorAll('button')].some(b=>b.textContent.trim()==='Downloaded')")
    button("Downloaded")
    wait("return !!document.querySelector('.downloaded-player-hint') && document.querySelectorAll('.selection-checkbox').length > 2")
    js("document.querySelector('.selection-checkbox').click()")
    first_page = js("return document.querySelector('.product-card .selection-checkbox').getAttribute('aria-label')")
    button("Next", "document.querySelector('.list-header .pagination')")
    wait(f"return document.querySelector('.product-card .selection-checkbox')?.getAttribute('aria-label') !== {json.dumps(first_page)}")
    js("document.querySelectorAll('.selection-checkbox')[0].click(); document.querySelectorAll('.selection-checkbox')[1].click()")
    wait("return document.querySelector('.selection-bar').textContent.includes('3 selected')")
    js("""
      const invoke=window.__TAURI_INTERNALS__.invoke;
      window.__localFilesQa={calls:[],hold:false,failOnce:false};
      window.__TAURI_INTERNALS__.invoke=async function(command,args,...rest) {
        const qa=window.__localFilesQa;
        if(command==='delete_work_download') {
          qa.calls.push(args.request.workId);
          if(qa.hold) await new Promise(resolve=>{qa.release=resolve});
          if(qa.failOnce) {qa.failOnce=false; throw new Error('Folder is busy (simulated)')}
        }
        return invoke.call(this,command,args,...rest);
      };
    """)
    confirmation = "document.querySelector('dialog[aria-label^=\"Delete local files\"]')"
    # Direct deletion used to mount a disabled review above its confirmation.
    for dismiss in ["cancel", "close"]:
        button("Delete selected files", "document.querySelector('.selection-bar')")
        wait("return document.querySelectorAll('dialog[open]').length === 1")
        assert js(f"return !!{confirmation}")
        if dismiss == "cancel":
            button("Cancel", confirmation)
        else:
            close_header(confirmation)
        wait("return !document.querySelector('dialog[open]')")
        assert js("return window.__localFilesQa.calls.length === 0")
        assert js("return !document.querySelector('.selection-bar button:last-child').disabled")

    button("Export selected folders", "document.querySelector('.selection-bar')")
    wait("return document.querySelector('dialog[open] progress')?.value === 3")
    assert js("return document.querySelector('dialog[open]').textContent.includes('3 succeeded, 0 failed')")
    button("Export selected folders", "document.querySelector('dialog[open]')")
    wait("return document.querySelector('dialog[open]').textContent.includes('0 succeeded, 3 failed')")
    assert js("return document.querySelectorAll('dialog[open] li strong').length >= 3")
    button("Delete selected files", "document.querySelector('dialog[open]')")
    wait("return document.querySelectorAll('dialog[open]').length === 2")
    button("Cancel", "document.querySelector('dialog[aria-label^=\"Delete local files\"]')")
    wait("return document.querySelectorAll('dialog[open]').length === 1")
    assert js("return document.querySelector('.selection-bar').textContent.includes('3 selected')")
    button("Close", "document.querySelector('dialog[open]')")
    js("window.__localFilesQa.hold=true; window.__localFilesQa.failOnce=true")
    button("Delete selected files", "document.querySelector('.selection-bar')")
    wait(f"return !!{confirmation}")
    button("Delete selected files", confirmation)
    wait("return document.querySelector('dialog[open] progress')?.value === 0 && !!window.__localFilesQa.release")
    # A slow/hung native operation must never trap the user in its progress UI.
    close_header("document.querySelector('dialog[open]')")
    wait("return !document.querySelector('dialog[open]')")
    button("Review selected", "document.querySelector('.selection-bar')")
    wait("return document.querySelector('dialog[open] progress')?.value === 0")
    button("Close", "document.querySelector('dialog[open]')")
    button("Settings")
    button("Downloaded")
    button("Review selected", "document.querySelector('.selection-bar')")
    wait("return document.querySelector('dialog[open] progress')?.value === 0")
    assert js("return window.__localFilesQa.calls.length === 1"), "Duplicate batches must not start"
    js("window.__localFilesQa.hold=false; window.__localFilesQa.release()")
    wait("return document.querySelector('dialog[open]').textContent.includes('2 succeeded, 1 failed')")
    assert js("return document.querySelector('dialog[open]').textContent.includes('Folder is busy (simulated)')")
    wait("return document.querySelector('.selection-bar').textContent.includes('1 selected')")
    button("Delete selected files", "document.querySelector('dialog[open]')")
    wait(f"return !!{confirmation}")
    button("Delete selected files", confirmation)
    wait("return document.querySelector('dialog[open]').textContent.includes('1 succeeded, 0 failed')")
    wait("return !document.querySelector('.selection-bar')")
    close_header("document.querySelector('dialog[open]')")
    button("Settings")
    print(json.dumps({"toast": metrics, "crossPageSelection": True, "export": True,
                      "collisionReporting": True, "deleteCancellation": True, "bulkDelete": True,
                      "directDeleteConfirmation": True, "closeDuringDelete": True,
                      "partialFailureRetry": True, "nativePointerClicks": True}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--storybook", default="http://127.0.0.1:6011")
    parser.add_argument("--browser", default="/usr/lib/webkitgtk-6.0/MiniBrowser")
    args = parser.parse_args()
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    base = f"http://127.0.0.1:{port}"
    with tempfile.TemporaryFile() as log:
        driver = subprocess.Popen(["WebKitWebDriver", f"--port={port}"], stdout=log, stderr=log)
        session = None
        try:
            for _ in range(100):
                try:
                    if requests.get(base + "/status", timeout=1).ok:
                        break
                except requests.RequestException:
                    pass
                time.sleep(.1)
            result = requests.post(base + "/session", json={"capabilities": {"alwaysMatch": {
                "webkitgtk:browserOptions": {"binary": args.browser, "args": ["--automation"]}
            }}}, timeout=30).json()["value"]
            session = result["sessionId"]
            check_ui(args.storybook.rstrip("/"), base, session)
        finally:
            if session:
                requests.delete(f"{base}/session/{session}", timeout=10)
            driver.terminate()
            driver.wait(timeout=10)


if __name__ == "__main__":
    main()
