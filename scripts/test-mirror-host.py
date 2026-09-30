"""Manual Windows desktop test: mirror capture, both clients, reconnect and Stop.

Requires an unlocked desktop and the bundled FFmpeg in PATH. Captures the actual
primary screen over loopback only; video stays in memory and is never saved.
Does not start a display broker, modify the firewall or change display topology.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import time

VERSION = 3
HIDDEN = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}


def message(kind, body=b""):
    payload = struct.pack(">HB", VERSION, kind) + body
    return struct.pack(">I", len(payload)) + payload


def exact(sock, count):
    result = bytearray()
    while len(result) < count:
        chunk = sock.recv(count - len(result))
        if not chunk:
            raise EOFError("host closed the connection")
        result.extend(chunk)
    return result


def read(sock):
    count, = struct.unpack(">I", exact(sock, 4))
    assert 3 <= count <= 65536, count
    payload = exact(sock, count)
    version, kind = struct.unpack(">HB", payload[:3])
    assert version == VERSION
    return kind, payload[3:]


def handshake(sock, legacy, bounds):
    sock.settimeout(15)
    sock.sendall(message(1, struct.pack(">H", 11) + b"mirror-test"))
    assert read(sock)[0] == 1
    sock.sendall(message(2, struct.pack(">HHHB", *bounds, 60, 2 if legacy else 4 | 128)))
    kind, body = read(sock)
    assert kind == 3
    session, width, height, fps, codec = struct.unpack(">QHHHB", body)
    assert (fps, codec) == (60, 2 if legacy else 3)
    assert 2 <= width <= bounds[0] and 2 <= height <= bounds[1], (width, height)
    if legacy:
        assert width <= 1280 and height <= 800, (width, height)
    assert width % 2 == height % 2 == 0, (width, height)
    probes = 0
    while True:
        kind, body = read(sock)
        if kind == 4:
            break
        if kind == 11:
            probes += 1
        else:
            assert kind == 6
            sock.sendall(message(7, body))
    assert probes == (0 if legacy else 64), probes
    return session, (width, height)


def receive(sock, session, seconds):
    started = time.monotonic()
    encoded = bytearray()
    number = index = 0
    metadata = None
    while time.monotonic() - started < seconds or index:
        kind, body = read(sock)
        assert kind == 10, (kind, body[:100])
        sid, frame, timestamp, fragment, count, keyframe = struct.unpack(">QQQHHB", body[:29])
        assert (sid, frame, fragment) == (session, number, index)
        assert 0 <= fragment < count
        if not index:
            metadata = (timestamp, count, keyframe)
        assert metadata == (timestamp, count, keyframe)
        encoded.extend(body[29:])
        assert len(encoded) < 64 * 1024 * 1024, "unbounded capture"
        index += 1
        if index == count:
            number += 1
            index = 0
    assert number > 0
    return encoded


def probe(encoded, legacy, ffprobe, expected_size):
    result = subprocess.run([
        ffprobe, "-v", "error", "-f", "mpegts" if legacy else "h264", "-i", "pipe:0",
        "-count_frames", "-show_entries", "stream=codec_name,width,height,nb_read_frames",
        "-of", "json",
    ], input=encoded, capture_output=True, timeout=20, check=True, **HIDDEN)
    assert not result.stderr, result.stderr.decode(errors="replace")
    video = json.loads(result.stdout)["streams"][0]
    assert (video["codec_name"], video["width"], video["height"]) == ("h264", *expected_size), video
    # WGC can emit only changed desktop images. A static source does not promise
    # 30 frames in this interval; this is a decoding/negotiation test, not an FPS benchmark.
    assert int(video["nb_read_frames"]) >= 1, video
    return video


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", required=True)
    parser.add_argument("--ffprobe", default="ffprobe")
    args = parser.parse_args()
    assert os.name == "nt", "requires Windows graphics capture"
    host_path = str(Path(args.host).resolve())
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        address = reservation.getsockname()
    with tempfile.TemporaryDirectory(prefix="windowdeck-mirror-") as temp:
        folder = Path(temp)
        stop = folder / "stop"
        env = dict(os.environ, WINDOWDECK_STOP_FILE=str(stop),
                   WINDOWDECK_DISPLAY_EXE=str(folder / "no-display-helper.exe"),
                   WINDOWDECK_H264_ENCODER="libx264")
        with (folder / "host.out").open("wb") as output, (folder / "host.log").open("wb") as log:
            host = subprocess.Popen([host_path, "--mirror", f"{address[0]}:{address[1]}"],
                                    env=env, stdout=output, stderr=log, **HIDDEN)
            try:
                deadline = time.monotonic() + 10
                while "windowdeck_state=listening" not in (folder / "host.out").read_text():
                    assert host.poll() is None, "host failed to start"
                    assert time.monotonic() < deadline, "host startup timed out"
                    time.sleep(.05)
                cases = ((False, (1280, 800)), (True, (65535, 65535)),
                         (False, (1920, 1080)), (False, (2560, 1440)), (False, (1280, 800)))
                for attempt, (legacy, bounds) in enumerate(cases):
                    with socket.create_connection(address, timeout=10) as sock:
                        session, video_size = handshake(sock, legacy, bounds)
                        encoded = receive(sock, session, 12 if attempt == 0 else 2)
                        if attempt == len(cases) - 1:
                            stop.touch()
                            deadline = time.monotonic() + 5
                            while True:
                                try:
                                    if read(sock)[0] == 5:
                                        break
                                except EOFError:
                                    break
                                assert time.monotonic() < deadline, "Stop did not finish capture"
                    # Decode after closing the connection so this work cannot stall the host.
                    print(json.dumps({"attempt": attempt, "transport": "mpegts" if legacy else "h264_frames",
                                      **probe(encoded, legacy, args.ffprobe, video_size)}), flush=True)
                assert host.wait(timeout=8) == 0
                states = (folder / "host.out").read_text()
                logs = (folder / "host.log").read_text(encoding="utf-8")
                assert states.count("windowdeck_state=streaming") == len(cases), states
                assert "windowdeck_state=stopped" in states, states
                assert logs.count("event=mirror_capture_selected") == len(cases), logs
                assert logs.count('scale_filter="bicubic"') == len(cases), logs
                for forbidden in ("event=driver_capture_selected", "event=virtual_capture_selected",
                                  "encoder_stalled", "monitor_removed_or_changed", "connection_unstable"):
                    assert forbidden not in logs, logs
                print("PASS: mirror without display helper; framed/MPEG-TS decode; reconnect; Stop.")
            except BaseException:
                print((folder / "host.log").read_text(encoding="utf-8", errors="replace"))
                raise
            finally:
                stop.touch()
                try:
                    host.wait(timeout=12)
                except subprocess.TimeoutExpired:
                    host.kill()
                    host.wait()


if __name__ == "__main__":
    main()
