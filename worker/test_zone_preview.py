import io
import json
import unittest
import tempfile
from pathlib import Path
from unittest.mock import patch
from urllib.error import URLError

import hy_worker


class ProtocolStream(io.StringIO):
    def reconfigure(self, **kwargs):
        pass


class ZonePreviewTests(unittest.TestCase):
    def run_worker(self, requests, completions, *, model="hy", style="normal"):
        replies = iter(completions)

        def open_request(request, timeout):
            if request.data is None:
                return io.StringIO('{"status":"ok"}')
            reply = next(replies)
            if isinstance(reply, Exception):
                raise reply
            if json.loads(request.data)["stream"]:
                chunks = [{"choices": [{"delta": {"content": part}, "finish_reason": None}]} for part in reply]
                chunks.append({"choices": [{"delta": {}, "finish_reason": "stop"}]})
                return io.BytesIO(("".join("data: "+json.dumps(item)+"\n\n" for item in chunks)
                                   +"data: [DONE]\n\n").encode())
            return io.StringIO(json.dumps({"choices": [
                {"message": {"content": reply}, "finish_reason": "stop"}
            ]}))

        incoming = ProtocolStream("\n".join(map(json.dumps, requests)))
        outgoing = ProtocolStream()
        with patch.dict(hy_worker.os.environ, SUB_TARGET="uk", SUB_MODEL=model, SUB_DIALOGUE_STYLE=style,
                        SUB_HY_URL="http://worker.invalid", SUB_HY_KEY="test"), \
                patch.object(hy_worker.sys, "stdin", incoming), \
                patch.object(hy_worker.sys, "stdout", outgoing), \
                patch.object(hy_worker, "build_opener") as opener:
            opener.return_value.open.side_effect = open_request
            hy_worker.main()
        self.assertIsNone(next(replies, None))
        return [json.loads(line) for line in outgoing.getvalue().splitlines()[1:]]

    def test_only_matching_confirmed_text_context_and_zone_reuse_the_draft(self):
        line = dict(text="Are we square?", previous="Count it.", zone=0)
        preview = dict(line, zone_preview=True)
        confirm = dict(line, accept_preview=True)
        requests = [preview, dict(confirm, zone=1), confirm, preview,
                    dict(confirm, previous="Let's play."), preview, dict(confirm, text="Go.")]
        self.assertEqual(self.run_worker(requests,
                         ["ready", "draft A", "zone B", "draft A", "new context", "draft A", "new line"]),
                         [None, "zone B", "draft A", None, "new context", None, "new line"])

    def test_sentence_preview_emits_once_and_skips_a_repeated_history_prefix(self):
        prior = dict(zone=0,text="Stay here.",previous="")
        line = dict(zone=0,text="Take the keys. I'll drive.",previous=prior["text"])
        draft = dict(line,zone_preview=True,stream_sentence=True)
        confirm = dict(line,accept_preview=True,stream_sentence=True)
        a, b, c = "Залишайся тут.", "Візьми ключі.", "Я поведу."
        for chunks, retries, packets in [
            ([b+" ",c], [], [dict(line,sentence=b)]),
            ([a+" ",b+" "+c], [b+" "+c], []),
        ]:
            with self.subTest(chunks=chunks):
                self.assertEqual(self.run_worker([prior,draft,confirm], ["ready",a,chunks,*retries],
                                 model="Hy-MT2-7B-Q4_K_M.gguf",style="concise"),
                                 [a,*packets,None,b+" "+c])

    def test_failed_preview_is_retried_after_confirmation(self):
        line = dict(text="Go.", previous="", zone=0)
        self.assertEqual(self.run_worker([dict(line, zone_preview=True), dict(line, accept_preview=True)],
                         ["ready", URLError("temporary failure"), "Рушай."]), [None, "Рушай."])

    def test_normal_dialogue_accepts_a_split_but_retries_a_leaked_translation(self):
        prior = dict(text="Stay in the car.", previous="", zone=0)
        line = dict(text="Take the keys, I'll drive.", previous=prior["text"], zone=0)
        draft = dict(line, zone_preview=True)
        confirm = dict(line, accept_preview=True)
        split = "Візьми ключі. Я поведу."
        previous_uk = "Залишайся в машині."
        for answers in ([split], [previous_uk + " " + split, split]):
            with self.subTest(first=answers[0]):
                self.assertEqual(self.run_worker(
                    [prior, draft, confirm], ["ready", previous_uk, *answers],
                    model="Hy-MT2-7B-Q4_K_M.gguf"
                ), [previous_uk, None, split])

    def test_normal_and_concise_keep_their_line_break_handling(self):
        line = dict(text="Take the keys, I'll drive.", previous="Stay here.", zone=0)
        split = "Візьми ключі.\nЯ поведу."
        for style, answers, expected in [
            ("normal", [split, "Візьми ключі, я поведу."], "Візьми ключі, я поведу."),
            ("concise", [split], "Візьми ключі. Я поведу."),
        ]:
            with self.subTest(style=style):
                self.assertEqual(self.run_worker(
                    [line], ["ready", *answers], model="Hy-MT2-7B-Q4_K_M.gguf", style=style
                ), [expected])

    def test_batch_restores_row_order_without_consuming_dialogue_preview(self):
        line = dict(text="Go.", previous="", zone=0)
        requests = [dict(line, zone_preview=True), {"hud_labels": ["LOOK", "BACK"]},
                    dict(line, accept_preview=True)]
        self.assertEqual(self.run_worker(requests, ["ready", "Рушай.", "2. Назад\n1. Огляд"]),
                         [None, ["Огляд", "Назад"], "Рушай."])

    def test_malformed_label_ids_discard_the_batch_and_allow_individual_translation(self):
        group = {"hud_labels": ["LOOK", "BACK"]}
        single = dict(text="LOOK", previous="", zone=1, hud=True)
        for answer in ("1. Огляд", "1. Огляд\n1. Назад", "1. Огляд\n3. Назад", "1. Огляд\n2. "):
            with self.subTest(answer=answer):
                self.assertEqual(self.run_worker([group, single], ["ready", answer, "Огляд"]),
                                 [None, "Огляд"])

    def test_restarted_worker_completes_only_the_missing_hud_rows(self):
        group = dict(zone=2, hud_labels=["LOOK","BACK"], contexts=["controls","other"])
        objective = dict(zone=1, text="Find the boat.", previous="", hud=True)
        with tempfile.TemporaryDirectory() as folder, patch.dict(hy_worker.os.environ,
                SUB_RUNTIME=folder, SUB_HUD_CACHE="1"):
            Path(folder,"zones.json").write_text("[]")
            self.assertEqual(self.run_worker([objective, group],
                             ["ready","Знайди човен.","1. Огляд\n2. Назад"]),
                             ["Знайди човен.",["Огляд","Назад"]])
            changed = dict(group, hud_labels=["LOOK","READ"])
            self.assertEqual(self.run_worker([group, objective, changed], ["ready","Читати"]),
                             [["Огляд","Назад"], "Знайди човен.", ["Огляд","Читати"]])


if __name__ == "__main__":
    unittest.main()
