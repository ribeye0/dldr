#!/usr/bin/env python3
"""Exercise final AppImage bytes with an empty profile and generated silent media."""
import argparse
import base64
import ctypes
import json
import os
from pathlib import Path
import socket
import sqlite3
import zipfile
import subprocess
import sys
import tempfile
import time
import wave

import requests


def media_probe(appdir, media):
    library = appdir / "usr/lib/libmpv.so.2"
    mpv = ctypes.CDLL(str(library))
    mpv.mpv_client_api_version.restype = ctypes.c_ulong
    assert mpv.mpv_client_api_version() >> 16 == 2
    mpv.mpv_create.restype = ctypes.c_void_p
    mpv.mpv_set_option_string.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p]
    mpv.mpv_initialize.argtypes = [ctypes.c_void_p]
    mpv.mpv_command.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_char_p)]
    mpv.mpv_get_property.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int, ctypes.c_void_p]
    mpv.mpv_get_property_string.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    mpv.mpv_get_property_string.restype = ctypes.c_void_p
    mpv.mpv_free.argtypes = [ctypes.c_void_p]
    mpv.mpv_terminate_destroy.argtypes = [ctypes.c_void_p]
    handle = mpv.mpv_create()
    assert handle
    try:
        for key, value in {"config": "no", "load-scripts": "no", "ytdl": "no", "sub-auto": "no", "audio-file-auto": "no", "access-references": "no", "terminal": "no", "ao": "null", "vo": "null", "video": "no", "pause": "yes"}.items():
            assert mpv.mpv_set_option_string(handle, key.encode(), value.encode()) >= 0
        assert mpv.mpv_initialize(handle) >= 0
        for path in sorted(media.glob("track.*")):
            command = (ctypes.c_char_p * 3)(b"loadfile", os.fsencode(path), None)
            assert mpv.mpv_command(handle, command) >= 0
            for _ in range(150):
                pointer = mpv.mpv_get_property_string(handle, b"path")
                current = ctypes.string_at(pointer).decode() if pointer else ""
                if pointer:
                    mpv.mpv_free(pointer)
                duration = ctypes.c_double()
                if current == str(path) and mpv.mpv_get_property(handle, b"duration", 5, ctypes.byref(duration)) >= 0 and duration.value >= 2.9:
                    break
                time.sleep(.05)
            else:
                raise AssertionError(f"Packaged decoder failed: {path}")
        libraries = [line.split()[-1] for line in Path("/proc/self/maps").read_text().splitlines()
                     if any(name in line for name in ["libmpv.so", "libavcodec.so", "libavformat.so", "libswresample.so"])]
        assert libraries and all(str(appdir) in path for path in libraries), libraries
        print(json.dumps({"packagedCodecs": [p.suffix for p in media.glob("track.*")], "libraries": sorted(set(libraries))}))
    finally:
        mpv.mpv_terminate_destroy(handle)


def native_probe(appdir, temporary, qa):
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0)); port = probe.getsockname()[1]
    stubs = temporary / "bin"; stubs.mkdir()
    url_log = temporary / "urls.txt"
    opener = stubs / "xdg-open"
    opener.write_text('#!/bin/sh\nprintf "%s\\n" "$1" >> "$DLDR_QA_URL_LOG"\n')
    opener.chmod(0o755)
    downloads = temporary / "中文 日本語 Downloads"
    downloads.mkdir()
    config = temporary / "config"
    config.mkdir()
    (config / "user-dirs.dirs").write_text(f'XDG_DOWNLOAD_DIR="{downloads}"\n')
    env = os.environ | {"PATH": str(stubs) + ":" + os.environ["PATH"], "DLDR_QA_URL_LOG": str(url_log),
                        "XDG_DATA_HOME": str(temporary / "data"), "XDG_CONFIG_HOME": str(temporary / "config"),
                        "XDG_CACHE_HOME": str(temporary / "cache"), "TAURI_WEBVIEW_AUTOMATION": "true",
                        "WEBKIT_DISABLE_DMABUF_RENDERER": "1", "GDK_BACKEND": "x11", "LC_ALL": "zh_TW.UTF-8"}
    if os.geteuid() == 0:
        # Only the disposable rootless-build namespace runs as container uid 0.
        env["WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS"] = "1"
    log = (qa / "webdriver.log").open("w")
    driver = subprocess.Popen(["WebKitWebDriver", f"--port={port}"], env=env, stdout=log, stderr=log)
    base, session = f"http://127.0.0.1:{port}", None
    try:
        for _ in range(100):
            try:
                if requests.get(base + "/status", timeout=1).ok:
                    break
            except requests.RequestException:
                pass
            time.sleep(.1)
        result = requests.post(base + "/session", json={"capabilities": {"alwaysMatch": {"webkitgtk:browserOptions": {"binary": str(appdir / "AppRun")}}}}, timeout=60).json()["value"]
        assert "sessionId" in result, result
        session = result["sessionId"]

        def call(method, path, args=None):
            value = requests.request(method, base + "/session/" + session + path, json=args, timeout=45).json()["value"]
            assert not (isinstance(value, dict) and "error" in value), value
            return value

        def js(source):
            return call("POST", "/execute/sync", {"script": source, "args": []})

        def wait(source):
            for _ in range(150):
                if js(source):
                    return
                time.sleep(.1)
            raise AssertionError(source)

        call("POST", "/window/rect", {"width": 1280, "height": 900})
        wait("return !!document.querySelector('aside nav button[data-view=settings]')")
        assert js("return typeof HTMLElement.prototype.showPopover === 'function'"), "Native WebKit must support unclipped notifications"
        font = call("POST", "/execute/async", {"script": '''
            const done = arguments[arguments.length - 1];
            Promise.all([400, 600, 700, 900].map(weight =>
                document.fonts.load(`${weight} 16px "DLDR Noto Sans CJK"`, '繁體中文简体中文日本語𠮷髙﨑')
                    .then(faces => faces.map(face => face.status))
            )).then(done, error => done({error: String(error)}));
        ''', "args": []})
        assert font == [["loaded"]] * 4, font
        font_license = call("POST", "/execute/async", {"script": '''
            const done = arguments[arguments.length - 1];
            fetch('/fonts/OFL.txt').then(response => response.text()).then(done, error => done(String(error)));
        ''', "args": []})
        assert "Copyright © 2014-2021 Adobe" in font_license and "SIL OPEN FONT LICENSE Version 1.1" in font_license
        wait("return !!document.querySelector('.first-run #setup-login')")
        profile = config / "dldr"
        assert (profile / "dldr.sqlite").is_file()
        assert (profile / "webview").is_dir()
        assert (profile / "logs").is_dir()
        assert not (temporary / "data/io.github.ribeye0.dldr").exists()  # Profile data belongs in config/dldr.
        (qa / "native-first-run.png").write_bytes(base64.b64decode(call("GET", "/screenshot")))

        def invoke(command, args=None):
            script = f"const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke({json.dumps(command)},{json.dumps(args or {})}).then(value=>done({{value}}),error=>done({{failure:String(error)}}))"
            return call("POST", "/execute/async", {"script": script, "args": []})

        defaults = invoke("get_setup_folders")["value"]
        library = downloads / "dldr"
        assert Path(defaults["libraryRoot"]) == library, defaults
        assert not library.exists(), "Reading defaults must not create directories"
        before = invoke("get_settings")["value"]
        assert before["autoExtract"] is True
        rejected = invoke("configure_download_folders", {"libraryRoot": "relative", "downloadRoot": None, "uiLocale": "en"})
        assert "failure" in rejected, rejected
        assert invoke("get_settings")["value"] == before
        configured = invoke("configure_download_folders", {"libraryRoot": str(library), "downloadRoot": None, "uiLocale": "en"})
        assert configured["value"]["libraryRoot"] == str(library), configured
        assert Path(configured["value"]["downloadRoot"]) == library / ".staging"
        assert (library / ".staging").is_dir()
        assert not list(library.glob(".dldr-write-check-*"))

        # Real native player in the GTK process: this catches LC_NUMERIC errors
        # that standalone libmpv/codec tests cannot detect.
        local_work = library / "RJ99999999"
        local_work.mkdir()
        audio = temporary / "silent.wav"
        with wave.open(str(audio), "wb") as track:
            track.setnchannels(1); track.setsampwidth(2); track.setframerate(8000)
            track.writeframes(b"\0\0" * 8000 * 12)
        archive = local_work / "作品%|.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zipped:
            zipped.write(audio, "音声/01.wav")
            zipped.writestr("音声/01.wav.vtt", "WEBVTT\n\n00:01.000 --> 00:03.000\n測試字幕\n")
        with sqlite3.connect(profile / "dldr.sqlite") as db:
            db.execute("INSERT INTO works (work_id,title_json,raw_json,last_detail_sync_at) VALUES (?, ?, ?, '2026-01-01')",
                       ("RJ99999999", '{"ja_JP":"Synthetic archive test"}',
                        json.dumps({"genre_ids": [48, 302, 529, 166, 478]})))
            db.execute("INSERT INTO work_downloads (work_id,status,local_path,unpack_policy,updated_at) VALUES (?, 'downloaded', ?, 'keep_archives', '2026-01-01')",
                       ("RJ99999999", str(local_work)))
        opened = invoke("open_player", {"request": {"workId": "RJ99999999"}})
        tracks = opened["value"]["media"]["tracks"]
        assert len(tracks) == 1 and tracks[0]["name"] == "01.wav", opened
        played = invoke("control_player", {"request": {"workId": "RJ99999999", "control": {"action": "play", "trackId": tracks[0]["id"]}}})
        assert "failure" not in played, played
        for _ in range(100):
            playing = invoke("player_snapshot")["value"]
            assert playing["error"] is None, playing
            if playing["duration"] > 11.9 and playing["position"] > .1:
                break
            time.sleep(.1)
        else:
            raise AssertionError(playing)
        cues = invoke("player_subtitle", {"request": {"workId": "RJ99999999", "subtitleId": playing["subtitleId"]}})
        assert cues["value"][0]["text"] == "測試字幕", cues
        paused = invoke("control_player", {"request": {"workId": "RJ99999999", "control": {"action": "pause", "paused": True}}})
        assert paused["value"]["paused"]
        stopped = invoke("control_player", {"request": {"workId": "RJ99999999", "control": {"action": "stop"}}})
        assert "failure" not in stopped, stopped
        assert list(local_work.iterdir()) == [archive], "Playback must not extract audio"

        # Store a synthetic account through the real form, stopping before sign-in.
        js("for(const [id,value] of [['setup-login','test@example.invalid'],['setup-password','synthetic-password']]){const e=document.getElementById(id);e.value=value;e.dispatchEvent(new Event('input',{bubbles:true}))}")
        wait("return !document.querySelector('.first-run button[type=submit]').disabled")
        js("document.querySelector('.first-run form').requestSubmit()")
        wait("return !!document.querySelector('.first-run .setup-account')")
        assert len(invoke("list_accounts")["value"]) == 1
        assert (profile / "credentials/vault.json").is_file()
        # The full sync/retry flow is covered by the production-route fixture.
        # Close only this test dialog to inspect unrelated settings without DLsite.
        js("document.querySelector('dialog:has(.first-run)').close()")
        js("document.querySelector('aside nav button[data-view=settings]').click()")
        wait("return !!document.querySelector('#ui-theme')")
        for theme in ["dark", "light", "system"]:
            js(f"const s=document.querySelector('#ui-theme');s.value='{theme}';s.dispatchEvent(new Event('change',{{bubbles:true}}))")
            expected = theme if theme != "system" else js("return matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light'")
            wait(f"return document.documentElement.dataset.theme==='{expected}'")
            (qa / f"native-{theme}.png").write_bytes(base64.b64decode(call("GET", "/screenshot")))
        # Change the archive preference through the actual settings form.
        js("document.querySelector('.settings-layout > .page-header .tabs').querySelectorAll('button')[1].click()")
        wait("return !!document.querySelector('#archive-handling')")
        assert js("return document.querySelector('#archive-handling').value") == "extract"
        for choice, expected in [("keep", False), ("extract", True)]:
            js(f"const s=document.querySelector('#archive-handling');s.value='{choice}';s.dispatchEvent(new Event('change',{{bubbles:true}}));s.closest('form').requestSubmit()")
            for _ in range(100):
                if invoke("get_settings")["value"]["autoExtract"] is expected:
                    break
                time.sleep(.1)
            else:
                raise AssertionError("Archive preference was not saved")
            wait("return !document.querySelector('#archive-handling').disabled")
        js("document.querySelector('.settings-layout > .page-header .tabs').querySelectorAll('button')[0].click()")
        wait("return !!document.querySelector('#ui-locale')")
        for language in ["en", "zh-TW", "zh-CN", "ja"]:
            js(f"const s=document.querySelector('#ui-locale');s.value='{language}';s.dispatchEvent(new Event('change',{{bubbles:true}}))")
            wait(f"return document.querySelector('#ui-locale').value==='{language}'")
            (qa / f"native-{language}.png").write_bytes(base64.b64decode(call("GET", "/screenshot")))
            js("document.querySelector('aside nav button[data-view=downloaded]').click()")
            wait("return document.querySelectorAll('.product-card .product-tag').length===5")
            expected_genres = {
                "en": ["Cuckoldry (Netorare)", "Cuckoldry (Netori)", "Cuckoldry (Netorase)", "Short Hair", "Short Hair (Male)"],
                "zh-TW": ["NTR", "寢取/NTR", "故意被NTR/綠奴", "短髮", "短髮"],
                "zh-CN": ["被NTR(苦主视角)", "NTR(黄毛视角)", "故意被NTR/绿奴", "短发", "短发"],
                "ja": ["寝取られ", "寝取り", "寝取らせ", "ショートカット", "短髪"],
            }[language]
            actual_genres = js("return [...document.querySelectorAll('.product-card .product-tag')].map(node=>node.textContent.trim())")
            assert sorted(actual_genres) == sorted(expected_genres), (language, actual_genres)
            detail = invoke("get_product_detail", {"request": {"workId": "RJ99999999"}})["value"]
            assert sorted(tag["genreId"] for tag in detail["tags"]) == [48, 166, 302, 478, 529], detail
            js("document.querySelector('.product-card .card-title').click()")
            wait("return !!document.querySelector('dialog[open] #detail-tags')")
            assert sorted(js("return [...document.querySelectorAll('dialog[open] .tag-overview .chips button')].map(node=>node.textContent.trim())")) == sorted(expected_genres)
            js("document.querySelector('dialog[open] header button').click();document.querySelector('aside nav button[data-view=settings]').click()")
            wait("return !!document.querySelector('#ui-locale')")
        # A rejected edit of the synthetic account leaves its dialog open.
        # This exercises the packaged toast/top layer without changing real data.
        js("document.querySelector('.settings-layout > .page-header .tabs').querySelectorAll('button')[2].click()")
        wait("return !!document.querySelector('.account-row')")
        js("document.querySelector('.account-row button:last-child').click()")
        wait("return !!document.querySelector('dialog[open] #account-label')")
        js("const input=document.querySelector('#account-label');input.value='';input.dispatchEvent(new Event('input',{bubbles:true}));input.closest('form').requestSubmit()")
        wait("return !!document.querySelector('.toast-stack:popover-open .toast')")
        notification = js("""
            const node=document.querySelector('.toast-stack'),dialog=node.closest('dialog');
            dialog.scrollTop=dialog.scrollHeight;
            const r=node.getBoundingClientRect(),close=node.querySelector('.toast-close'),c=close.getBoundingClientRect();
            return {visible:r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight,
                belowHeader:r.top>dialog.querySelector('header').getBoundingClientRect().bottom,
                interactive:document.elementFromPoint(c.x+c.width/2,c.y+c.height/2)?.closest('.toast-close')===close};
        """)
        assert notification == {"visible": True, "belowHeader": True, "interactive": True}, notification
        js("const dialog=document.querySelector('#account-label').closest('dialog');document.querySelector('.toast-close').click();dialog.querySelector('header button').click();document.querySelector('.settings-layout > .page-header .tabs').querySelectorAll('button')[0].click()")
        wait("return !!document.querySelector('#ui-locale')")
        # Exercise the real Svelte slider/controller against the native player,
        # not only direct IPC. Every destination below needs one interaction.
        js("document.querySelector('aside nav button[data-view=downloaded]').click()")
        wait("return !!document.querySelector('.card-play-target')")
        js("document.querySelector('.card-play-target').click()")
        wait("return !!document.querySelector('.player-panel .track-list button')")
        js("document.querySelector('.player-panel .track-list button').click()")
        wait("return !!document.querySelector('.seek input') && !document.querySelector('.seek input').disabled")
        for target in [8.0, 2.0, 6.0]:
            js(f"const s=document.querySelector('.seek input');s.value='{target}';s.dispatchEvent(new Event('input',{{bubbles:true}}));s.dispatchEvent(new Event('change',{{bubbles:true}}))")
            for _ in range(100):
                current = invoke("player_snapshot")["value"]
                assert current["error"] is None, current
                if target <= current["position"] < target + 1.0:
                    break
                time.sleep(.05)
            else:
                raise AssertionError(current)
        js("for(const value of ['2','8','4']){const s=document.querySelector('.seek input');s.value=value;s.dispatchEvent(new Event('input',{bubbles:true}));s.dispatchEvent(new Event('change',{bubbles:true}))}")
        wait("return Number(document.querySelector('.seek input').value)>=4 && Number(document.querySelector('.seek input').value)<5 && !document.querySelector('.player-panel .primary-control').disabled")
        wait("return !!document.querySelector('.cue-time')")
        metrics = js("return {title:getComputedStyle(document.querySelector('.panel-header h2')).fontSize,icon:document.querySelector('.header-actions svg').getBoundingClientRect().width,cue:document.querySelector('.cue-time') && getComputedStyle(document.querySelector('.cue-time')).fontSize}")
        assert metrics == {"title":"16px", "icon":24, "cue":"14px"}, metrics
        (qa / "native-player.png").write_bytes(base64.b64decode(call("GET", "/screenshot")))
        invoke("control_player", {"request": {"workId": "RJ99999999", "control": {"action": "stop"}}})
        js("document.querySelector('.header-actions button:last-child').click();document.querySelector('aside nav button[data-view=settings]').click()")
        wait("return !!document.querySelector('#ui-locale')")
        # Invalid local media input exercises typed errors across a real blocking
        # worker. The user's actual profile and purchased files are never opened.
        rejected_subtitle = invoke("player_subtitle", {"request": {"workId":"RJ99999999", "subtitleId":"../PRIVATE-SUBTITLE"}})
        assert "failure" in rejected_subtitle, rejected_subtitle
        logs = invoke("list_audit_events", {"request": {"limit":500}})["value"]
        cause = next(event for event in logs if event["stage"] == "cause" and event.get("errorCode") == "player")
        assert cause["operation"] == "player_subtitle" and cause.get("operationId"), cause
        assert cause.get("errorMessage") and cause.get("hint"), cause
        assert cause["details"].get("playerErrorKind") in ["invalid_path", "invalid_subtitle", "invalid_control"], cause
        assert "PRIVATE-SUBTITLE" not in json.dumps(logs), logs
        js("document.querySelector('.settings-layout > .page-header .tabs').querySelectorAll('button')[3].click()")
        wait("return !!document.querySelector('.logs-layout .log-record')")
        assert not js("return document.querySelector('.logs-layout').textContent.includes('Application logs')"), "Logs must be one timeline"
        wait("return [...document.querySelectorAll('.log-row')].some(node=>node.textContent.includes('player_subtitle'))")
        js("[...document.querySelectorAll('.log-row')].find(node=>node.textContent.includes('player_subtitle')).click()")
        wait("return ['Suggested action','建議處理方式','建议处理方式','対処方法'].some(label=>document.querySelector('.logs-layout').textContent.includes(label))")
        (qa / "native-logs.png").write_bytes(base64.b64decode(call("GET", "/screenshot")))
        js("const input=document.querySelector('.logs-layout input:not([type=checkbox])');input.value='字幕';input.dispatchEvent(new Event('input',{bubbles:true}))")
        wait("return [...document.querySelectorAll('.logs-layout .log-row')].some(node=>node.textContent.includes('player_subtitle'))")
        assert js("return [...document.querySelectorAll('.logs-layout .log-row')].every(node=>node.textContent.includes('字幕'))"), "Search must match translated operation names"

        js("document.querySelector('.settings-layout > .page-header .tabs').querySelectorAll('button')[0].click()")
        wait("return !!document.querySelector('#ui-locale')")
        info = call("POST", "/execute/async", {"script": "const done=arguments[arguments.length-1];Promise.all([window.__TAURI_INTERNALS__.invoke('plugin:app|version'),window.__TAURI_INTERNALS__.invoke('plugin:app|identifier'),window.__TAURI_INTERNALS__.invoke('set_native_header_theme',{dark:false})]).then(v=>done(v),e=>done({failure:String(e)}))", "args": []})
        version = json.loads((Path(__file__).resolve().parents[2] / "package.json").read_text())["version"]
        assert isinstance(info, list) and info[0] == version and info[1] == "io.github.ribeye0.dldr", info
        js("document.querySelector('.settings-layout > .page-header .tabs').querySelectorAll('button')[4].click()")
        wait("return document.querySelector('.settings-layout').textContent.includes('GitHub')")
        js("document.querySelector('.settings-layout .dldr\\\\:my-5 button').click()")
        for _ in range(100):
            if url_log.exists():
                break
            time.sleep(.1)
        assert url_log.read_text().strip() == "https://github.com/ribeye0/dldr/releases"
        return {"themes": ["light", "dark", "system"], "languages": ["en", "zh-TW", "zh-CN", "ja"], "firstRun": {"accountForm": True, "defaultFolders": True, "rejectsRelativePathsBeforeSaving": True, "createsWritableUnicodeFolders": True}, "embeddedFrontend": True, "nativeIpc": True, "officialGenreLabelsById": True, "readableUnifiedLogs": True, "blockingFailureCorrelation": True, "localizedLogSearch": True, "unclippedModalNotifications": notification, "nativeArchivePlayback": {"gtkNumericLocale": "C", "compressedWav": True, "automaticVtt": True, "archivesKept": True, "settingsSaved": True, "nativeSliderSeeks": True, "latestSeekWins": True, "largerPlayerControls": metrics}, "manualUpdateDestination": "ribeye0/dldr/releases", "version": info[0], "identifier": info[1], "profile": "empty temporary XDG directories"}
    finally:
        if session:
            try:
                requests.delete(base + "/session/" + session, timeout=10)
            except requests.RequestException:
                pass
        driver.terminate()
        try:
            driver.wait(5)
        except subprocess.TimeoutExpired:
            driver.kill(); driver.wait()
        log.close()


def main(output):
    output = output.resolve()
    subprocess.run(["sha256sum", "--check", "SHA256SUMS.txt"], cwd=output, check=True)
    images = list(output.glob("*.AppImage")); assert len(images) == 1, images
    qa = output.parent / "qa"; qa.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="dldr-packaged-qa-") as folder:
        temporary = Path(folder)
        subprocess.run([str(images[0]), "--appimage-extract"], cwd=temporary, check=True, stdout=subprocess.DEVNULL)
        appdir = temporary / "squashfs-root"
        assert not list((appdir / "usr/lib").glob("libwayland-client.so.*"))
        assert list(appdir.rglob("libgstlibav.so")), "Packaged WebKit FFmpeg plugin is missing"
        media = temporary / "synthetic"; media.mkdir()
        with wave.open(str(media / "track.wav"), "wb") as track:
            track.setnchannels(1); track.setsampwidth(2); track.setframerate(16000); track.writeframes(b"\0\0" * 48000)
        for extension in ["mp3", "flac", "m4a", "ogg"]:
            subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-i", str(media / "track.wav"), str(media / f"track.{extension}")], check=True)
        env = os.environ | {"LD_LIBRARY_PATH": ":".join([str(appdir / "usr/lib"), str(appdir / "usr/lib/x86_64-linux-gnu")])}
        result = subprocess.check_output([sys.executable, str(Path(__file__).resolve()), "--media", str(appdir), str(media)], env=env, text=True)
        cli_dir = temporary / "cli"; cli_dir.mkdir()
        cli_archives = list(output.glob("dldr_*_linux_x86_64.tar.gz")); assert len(cli_archives) == 1
        subprocess.run(["tar", "-xzf", str(cli_archives[0]), "-C", str(cli_dir)], check=True)
        version = json.loads((Path(__file__).resolve().parents[2] / "package.json").read_text())["version"]
        assert subprocess.check_output([str(cli_dir / "dldr"), "--version"], text=True).strip() == f"dldr {version}"
        login_profile = temporary / "no-terminal-profile"
        login = subprocess.run([str(cli_dir / "dldr"), "--data-dir", str(login_profile), "login"], input="", capture_output=True, text=True)
        assert login.returncode != 0 and "terminal" in login.stderr.lower(), login.stderr
        assert not login_profile.exists()
        native = native_probe(appdir, temporary, qa)
        report = {"media": json.loads(result.splitlines()[-1]), "native": native, "cli": {"versionMatches": True, "redirectedLoginRejectedBeforeWrites": True}, "realAccountUsed": False,
                  "remaining": ["authenticated official playback", "simultaneous purchased-work GUI download acceptance", "additional desktop distributions"]}
        (qa / "acceptance.json").write_text(json.dumps(report, indent=2) + "\n")
        (output / "acceptance.json").write_text(json.dumps(report, indent=2) + "\n")
        import hashlib
        (output / "SHA256SUMS.txt").write_text("".join(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in sorted(output.iterdir()) if p.is_file() and p.name != "SHA256SUMS.txt"))
        print(json.dumps(report))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--media", action="store_true")
    parser.add_argument("paths", nargs="+", type=Path)
    args = parser.parse_args()
    if args.media:
        media_probe(*args.paths)
    else:
        main(*args.paths)
