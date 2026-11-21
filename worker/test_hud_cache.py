import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from hud_cache import HudCache
from hy_worker import translate_labels


class HudTests(unittest.TestCase):
    def test_restart_and_cache_boundaries(self):
        with tempfile.TemporaryDirectory() as folder, patch.dict(os.environ, SUB_RUNTIME=folder,
                SUB_GAME_PROFILE="game-a", SUB_HUD_CACHE="1"):
            Path(folder, "zones.json").write_text(json.dumps([{"Name":"Panel"}]))
            def key(cache, language="uk", nearby="USE"):
                return cache.key(language, 1, "panel", "OPEN", nearby)
            first = HudCache("model-a")
            map_key = first.key("uk", 1, "map", "EXIT", "ADD MARKER")
            first.save([(map_key, "Вийти")])
            self.assertEqual(first.get(first.key("uk", 1, "map", "EXIT", "RECENTER")), "Вийти")
            first.save([(key(first), "Відкрити")])
            first.db.close()
            second = HudCache("model-a")
            self.assertEqual(second.single(key(second), lambda: self.fail("Unexpected inference")), "Відкрити")
            self.assertIsNone(second.get(key(second, "fr")))
            self.assertIsNone(second.get(key(second, nearby="ROAD")))
            second.db.close()
            for model, game in [("model-b", "game-a"), ("model-a", "game-b")]:
                with patch.dict(os.environ, SUB_GAME_PROFILE=game):
                    other = HudCache(model)
                    self.assertIsNone(other.get(key(other)))
                    other.db.close()

    def test_stream_preserves_ids_and_rejects_a_late_duplicate(self):
        seen = []
        def parse(lines):
            return translate_labels(None, ["LOOK", "BACK", "TAKE"], "Ukrainian", False,
                                    lambda _: iter(lines), lambda i,s: seen.append((i,s)))
        self.assertEqual(parse(["2. Назад", "1. Огляд", "3. Взяти"]), ["Огляд", "Назад", "Взяти"])
        self.assertEqual(seen, [(1,"Назад"),(0,"Огляд")])
        self.assertIsNone(parse(["1. Огляд", "1. Назад", "3. Взяти"]))

    def test_obs_scene_switch_and_unwritable_cache(self):
        with tempfile.TemporaryDirectory() as folder, patch.dict(os.environ, SUB_RUNTIME=folder,
                SUB_GAME_PROFILE="", SUB_HUD_CACHE="1"):
            root = Path(folder)
            (root / "zones.json").write_text("[]")
            scene = root / "hud-scene.txt"
            scene.write_text("Game A")
            cache = HudCache("model")
            key = lambda: cache.key("uk", 1, "panel", "BACK")
            cache.save([(key(), "Назад")])
            scene.write_text("Game B")
            self.assertIsNone(cache.get(key()))
            scene.write_text("Game A")
            self.assertEqual(cache.get(key()), "Назад")
            cache.db.close()
            (root / "hud-cache.sqlite3").unlink()
            (root / "hud-cache.sqlite3").mkdir()
            fallback = HudCache("model")
            self.assertEqual(fallback.single(None, lambda: "Назад"), "Назад")


if __name__ == "__main__":
    unittest.main()
