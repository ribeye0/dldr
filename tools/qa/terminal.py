#!/usr/bin/env python3
"""Isolated Linux PTY smoke checks; all network requests use an unreachable proxy."""
import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import sqlite3
import subprocess
import sys
import tempfile
import termios
import time

binary = str(Path(sys.argv[1]).resolve())


def scenario(action):
    with tempfile.TemporaryDirectory(prefix='dldr-terminal-') as directory:
        profile = Path(directory) / 'profile'
        master, slave = pty.openpty()
        original = termios.tcgetattr(slave)

        def controlling_terminal():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        environment = os.environ.copy()
        for key in ('HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'http_proxy', 'https_proxy', 'all_proxy'):
            environment[key] = 'http://127.0.0.1:1'
        environment.update(NO_PROXY='', no_proxy='')
        child = subprocess.Popen(
            [binary, '--data-dir', str(profile), 'login', '--no-sync'],
            stdin=slave, stdout=slave, stderr=slave, env=environment,
            preexec_fn=controlling_terminal,
        )
        transcript = bytearray()
        deadline = time.monotonic() + 15

        def until(text):
            while text.encode() not in transcript:
                assert time.monotonic() < deadline, 'Prompt timed out'
                ready, _, _ = select.select([master], [], [], 0.1)
                if ready:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError as error:
                        if error.errno == errno.EIO:
                            break
                        raise
                    if not chunk:
                        break
                    transcript.extend(chunk)
            assert text.encode() in transcript, 'Expected prompt missing'

        try:
            until('DLsite login ID/email')
            if action == 'escape':
                os.write(master, b'\x1b')
            elif action == 'sigterm':
                child.send_signal(signal.SIGTERM)
            else:
                os.write(master, b'fixture-private@example.invalid\r')
                until('Password (input hidden')
                if action == 'ctrl-c':
                    os.write(master, b'\x03')
                else:
                    os.write(master, b'fixture-password-with spaces!\r')
            while child.poll() is None:
                assert time.monotonic() < deadline, 'CLI did not exit'
                ready, _, _ = select.select([master], [], [], 0.1)
                if ready:
                    transcript.extend(os.read(master, 65536))
            # Drain any buffered final error without printing credentials/transcripts.
            while select.select([master], [], [], 0)[0]:
                transcript.extend(os.read(master, 65536))
            assert child.returncode == (1 if action == 'failure' else 130), 'Wrong exit code'
            current = termios.tcgetattr(slave)
            assert current == original, 'Terminal settings were not restored'
            assert b'fixture-private' not in transcript, 'Login identity echoed'
            assert b'fixture-password' not in transcript, 'Password echoed'
            database = sqlite3.connect(profile / 'dldr.sqlite')
            assert database.execute('SELECT COUNT(*) FROM accounts').fetchone()[0] == 0
            database.close()
            vault = json.loads((profile / 'credentials/vault.json').read_text())
            assert 'fixture-password' not in json.dumps(vault), 'Failed login persisted credentials'
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()
            os.close(master)
            os.close(slave)
    print(f'PASS: {action}; hidden input, restored terminal, no saved account/secrets')


for case in ('escape', 'ctrl-c', 'sigterm', 'failure'):
    scenario(case)
