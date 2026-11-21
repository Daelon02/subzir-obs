"""Optional local store of confirmed HUD translations, shared by both workers."""
import hashlib
import json
import os
import sqlite3
from pathlib import Path

MAX_ENTRIES = 8192


class HudCache:
    def __init__(self, model, weights=()):
        self.db = None
        runtime = os.environ.get("SUB_RUNTIME")
        if not runtime or os.environ.get("SUB_HUD_CACHE") == "0":
            return
        root = Path(runtime)
        try:
            layout = json.loads((root / "zones.json").read_text("utf-8-sig"))
            profile = [[z.get(k) for k in ("Name", "Left", "Top", "W", "H", "Hud", "Legend")]
                       for z in layout]
            stamps = []
            for asset in weights:
                if asset.is_file():
                    info = asset.stat()
                    stamps.append([asset.name, info.st_size, info.st_mtime_ns])
            identity = json.dumps([1, model, stamps, profile], ensure_ascii=False).encode("utf8")
            self.scope = hashlib.sha256(identity).hexdigest()
            self.game = os.environ.get("SUB_GAME_PROFILE", "")
            self.scene_file = root / "hud-scene.txt"
            self.db = sqlite3.connect(root / "hud-cache.sqlite3", timeout=0.15)
            self.db.execute("CREATE TABLE IF NOT EXISTS hud (k TEXT UNIQUE NOT NULL, value TEXT NOT NULL)")
        except (sqlite3.Error, OSError, ValueError, AttributeError, TypeError):
            self.close()

    def close(self):
        if self.db is not None:
            self.db.close()
            self.db = None

    def key(self, target, zone, kind, text, nearby=""):
        if self.db is not None:
            game = self.game
            if not game:
                try:
                    game = self.scene_file.read_text("utf8")
                except (OSError, UnicodeError):
                    pass
            # Map controls keep their meaning when a neighbouring action is disabled.
            context = "" if kind == "map" else nearby
            return json.dumps([self.scope, game, target, zone, kind, text, context], ensure_ascii=False)

    def get(self, key):
        if self.db is not None and key is not None:
            try:
                row = self.db.execute("SELECT value FROM hud WHERE k=?", (key,)).fetchone()
                return row[0] if row and isinstance(row[0], str) and row[0].strip() else None
            except sqlite3.Error:
                return None

    def save(self, entries):
        if self.db is None:
            return
        try:
            with self.db:
                self.db.executemany("INSERT OR REPLACE INTO hud(k,value) VALUES (?,?)", entries)
                self.db.execute("DELETE FROM hud WHERE rowid <= "
                                "(SELECT rowid FROM hud ORDER BY rowid DESC LIMIT 1 OFFSET ?)", (MAX_ENTRIES,))
        except sqlite3.Error:
            pass

    def single(self, key, translate):
        value = self.get(key)
        if value is None:
            value = translate()
            if value:
                self.save([(key, value)])
        return value
