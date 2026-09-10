"""Headless native client integration: fragmented video, lost connection, restart, Stop.

Uses generated pixels only. No monitor, broker, desktop capture or LAN required.
"""
import argparse
import os
import re
import socket
import struct
import subprocess
import threading
import time

VERSION = 3


def message(kind, body=b""):
    payload = struct.pack(">HB", VERSION, kind) + body
    return struct.pack(">I", len(payload)) + payload


def exact(sock, count):
    result = b""
    while len(result) < count:
        chunk = sock.recv(count - len(result))
        if not chunk:
            raise RuntimeError("client closed during handshake")
        result += chunk
    return result


def read(sock, kind):
    count = struct.unpack(">I", exact(sock, 4))[0]
    assert 3 <= count <= 65536
    payload = exact(sock, count)
    assert struct.unpack(">HB", payload[:3]) == (VERSION, kind)
    return payload[3:]


def handshake(sock, session):
    sock.settimeout(10)
    read(sock, 1)
    sock.sendall(message(1, struct.pack(">H", 4) + b"test"))
    assert read(sock, 2)[-1] & 4, "native access-unit capability missing"
    sock.sendall(message(3, struct.pack(">QHHHB", session, 1280, 800, 60, 3)))
    sock.sendall(message(4))


def send_frame(sock, session, number, payload, partial=False):
    split = len(payload) // 2
    for index, fragment in enumerate((payload[:split], payload[split:])):
        body = struct.pack(">QQQHHB", session, number, 100000 + number * 16667, index, 2, 1)
        sock.sendall(message(10, body + fragment))
        if partial:
            return


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--client", required=True)
    parser.add_argument("--ffmpeg", default="ffmpeg")
    args = parser.parse_args()
    hidden = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}
    encoded = subprocess.check_output([
        args.ffmpeg, "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=1280x800:rate=60",
        "-frames:v", "12", "-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency",
        "-g", "1", "-x264-params", "aud=1:repeat-headers=1", "-f", "h264", "pipe:1",
    ], **hidden)
    boundaries = [m.start() for m in re.finditer(b"\x00\x00(?:\x00)?\x01", encoded)
                  if encoded[m.end()] & 31 == 9]
    assert len(boundaries) == 12, "expected twelve complete H.264 access units"
    frames = [encoded[a:b] for a, b in zip(boundaries, boundaries[1:] + [len(encoded)])]
    failures = []
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        listener.settimeout(15)

        def serve():
            try:
                for session in (10, 20):
                    sock, _ = listener.accept()
                    with sock:
                        handshake(sock, session)
                        for number, frame in enumerate(frames):
                            send_frame(sock, session, number, frame)
                            time.sleep(1 / 60)
                        if session == 10:
                            send_frame(sock, session, 12, frames[0], partial=True)
                            # A TCP EOF inside the next unit must flush and reconnect.
                        else:
                            sock.sendall(message(5))
            except Exception as error:
                failures.append(error)

        server = threading.Thread(target=serve, daemon=True)
        server.start()
        env = dict(os.environ, SDL_VIDEODRIVER="dummy")
        result = subprocess.run([args.client, f"127.0.0.1:{listener.getsockname()[1]}", "--native"],
                                env=env, capture_output=True, text=True, timeout=25, **hidden)
        server.join(timeout=2)
    assert not failures, failures
    assert not server.is_alive(), "test server did not finish"
    assert result.returncode == 0, result.stderr
    assert "native_reconnected" in result.stderr, result.stderr
    assert "native_decode_reset" not in result.stderr, result.stderr
    assert "native_decoder active=" in result.stderr, result.stderr
    print("PASS: native decoding, fragmented access units, partial-frame disconnect, reconnect and explicit Stop")


if __name__ == "__main__":
    main()
