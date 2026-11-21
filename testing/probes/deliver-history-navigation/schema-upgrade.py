"""Run the baseline and candidate Windows nodes on private, disposable databases."""
import argparse
import hashlib
import json
import socket
import sqlite3
import subprocess
import tempfile
import time
from contextlib import closing
from pathlib import Path

parser = argparse.ArgumentParser()
for name in ("baseline", "candidate", "openssl"):
    parser.add_argument(name, type=Path)
args = parser.parse_args()


def boot(root, exe):
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    config = root / "node.toml"
    settings = dict(data_dir=(root / "data").as_posix(), listen=f"127.0.0.1:{port}",
                    canonical_origin=f"https://localhost:{port}",
                    tls_key=(root.parent / "key.pem").as_posix(), tls_cert=(root.parent / "cert.pem").as_posix())
    config.write_text("\n".join(f"{k} = {json.dumps(v)}" for k, v in settings.items()))
    command = [exe, "--config", config]
    with subprocess.Popen(command + ["daemon", "start"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) as node:
        try:
            for _ in range(150):
                if node.poll() is not None:
                    raise RuntimeError(f"node exited with {node.returncode}")
                result = subprocess.run(command + ["device", "list"], capture_output=True, timeout=10)
                if result.returncode == 0:
                    break
                time.sleep(0.1)
            else:
                raise TimeoutError("owner device list readiness")
            subprocess.run(command + ["daemon", "stop"], capture_output=True, check=True, timeout=10)
            assert node.wait(timeout=30) == 0
        finally:
            if node.poll() is None:
                node.kill()
                node.wait()


def schema(root):
    with closing(sqlite3.connect(root / "data/node.sqlite3")) as db:
        return db.execute("SELECT type,name,sql FROM sqlite_master ORDER BY name").fetchall()


def content(db):
    meta = db.execute("SELECT native_session_id,record_count,byte_size,content_digest,last_updated_at_ms FROM transcript_sessions ORDER BY native_session_id").fetchall()
    digest = hashlib.sha256()
    for (blob,) in db.execute("SELECT raw_json FROM transcript_records ORDER BY native_session_id,ordinal"):
        digest.update(blob)
    return meta, digest.digest()


with tempfile.TemporaryDirectory(prefix="hn-schema-") as tmp:
    root = Path(tmp)
    subprocess.run([args.openssl, "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                    "-subj", "/CN=localhost", "-keyout", root / "key.pem", "-out", root / "cert.pem"],
                   capture_output=True, check=True)
    old, new = root / "baseline", root / "candidate"
    for area, binary in ((old, args.baseline), (new, args.candidate)):
        area.mkdir()
        boot(area, binary)
    rows = schema(old)
    assert rows == schema(new) and not any("transcript_" in str(row) for row in rows)
    print(f"Identical node.sqlite3 schemas ({len(rows)} objects).", flush=True)
    cache = old / "data/transcript-cache.sqlite3"
    with closing(sqlite3.connect(cache)) as db:
        blob = b'{"text":"' + b"x" * (1024 * 1024 - 11) + b'"}'
        for name, count in [("a", 1), ("b", 2), ("large", 200)]:
            db.execute("INSERT INTO transcript_sessions VALUES(zeroblob(16),?,1,1,zeroblob(32),?,NULL,'unknown','normal',?,?)",
                       (name, int(time.time()*1000), count, count * len(blob)))
            db.executemany("INSERT INTO transcript_records VALUES(zeroblob(16),?,1,?,?,?)",
                           ((name, i+1, i, blob) for i in range(count)))
        db.commit()
        before = content(db)
    boot(old, args.candidate)
    with closing(sqlite3.connect(cache)) as db:
        assert content(db) == before
        assert db.execute("SELECT schema_version FROM transcript_cache_meta").fetchone() == (2,)
        assert db.execute("SELECT COUNT(DISTINCT generation_token) FROM transcript_sessions WHERE length(generation_token)=16 AND turn_state='pending' AND turn_count=0").fetchone() == (3,)
        assert db.execute("SELECT COUNT(*) FROM transcript_turns").fetchone() == (0,)
        assert all(r[6] == "CASCADE" for r in db.execute("PRAGMA foreign_key_list(transcript_turns)"))
    print("Three retained sessions migrated with original metadata and raw bytes intact; all remain pending.", flush=True)
    def digest_file():
        with cache.open("rb") as handle:
            return hashlib.file_digest(handle, "sha256").digest()
    migrated = digest_file()
    boot(old, args.baseline)
    assert digest_file() == migrated
    with closing(sqlite3.connect(cache)) as db:
        db.execute("UPDATE transcript_cache_meta SET schema_version=3")
        db.commit()
    future = digest_file()
    boot(old, args.candidate)
    assert digest_file() == future
    print("Baseline on v2 and candidate on v3 served owner device list and preserved cache file SHA-256.")
