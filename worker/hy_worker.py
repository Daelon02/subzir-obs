"""Translate using the llama.cpp instance owned by the Rust engine."""
import json
import os
import re
import sys
import time
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, ProxyHandler, build_opener

def main():
    sys.stdin.reconfigure(encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8")
    languages = json.loads(Path(__file__).with_name("hy-languages.json").read_text("utf8"))
    target = os.environ["SUB_TARGET"]
    language = languages[target]
    model = os.environ.get("SUB_MODEL","hy").removeprefix("gguf:")
    label = "Hy-MT2-1.8B Q6_K" if model == "hy" else model
    instruction = (
        f"Translate this line of game dialogue into natural, conversational {language}. Preserve the intended meaning and tone. Output only the translated line."
        if target == "uk" and model.casefold() == "hy-mt2-7b-q4_k_m.gguf"
        else f"Translate the following segment into {language}, without additional explanation."
    )
    endpoint = os.environ["SUB_HY_URL"]
    auth = {"Authorization": "Bearer " + os.environ["SUB_HY_KEY"], "Content-Type": "application/json"}
    client = build_opener(ProxyHandler({}))
    deadline = time.monotonic() + 60
    while True:
        try:
            with client.open(Request(endpoint + "/health", headers=auth),timeout=2) as response:
                if json.load(response).get("status") == "ok":
                    break
        except (URLError, TimeoutError):
            pass
        if time.monotonic() >= deadline:
            raise RuntimeError(f"{label} failed to start. Check the CUDA runtime, weights and GPU memory.")
        time.sleep(0.15)

    def complete(messages):
        payload = {"messages":messages,"temperature":0,
                   "max_tokens":128,"stream":False,"cache_prompt":True}
        request = Request(endpoint + "/v1/chat/completions", data=json.dumps(payload).encode(), headers=auth)
        with client.open(request,timeout=45) as res:
            choice = json.load(res)["choices"][0]
        result = choice["message"]["content"].strip()
        if not result or choice["finish_reason"] == "length":
            raise ValueError(f"{label} did not finish the subtitle")
        return result

    def translate(text, previous, hud=False):
        if hud:
            prompt = (f"Localize the following video game interface text into {language}. "
                      "For a menu action use a concise button label. Keep instructions clear. "
                      "Output only the translation.\n\n" + text)
            return complete([{"role":"user","content":prompt}])
        current = {"role":"user","content":f"{instruction}\n\n{text}"}
        if previous:
            earlier = " ".join(previous.splitlines())
            conversation = [{"role":"user","content":f"Previous dialogue: {earlier}"},
                            {"role":"assistant","content":"OK."}, current]
            try:
                result = complete(conversation)
                # Some models repeat a background sentence or question in the caption.
                # Retry the current line alone rather than trimming away dialogue.
                if sentence_count(result) <= sentence_count(text) and "\n" not in result:
                    return result
            except HTTPError as error:
                if error.code != 400:
                    raise
            except ValueError:
                pass
        return complete([current])

    translate("Ready.", "")
    print(json.dumps({"model":label,"target":language,"device":"CUDA"}),flush=True)
    previous, last = "", 0
    for line in sys.stdin:
        obj = json.loads(line)
        if isinstance(obj, str):
            now = time.monotonic()
            result = translate(obj, previous if now-last <= 8 else "")
            previous, last = obj, now
        else:
            try:
                result = translate(obj["text"], obj["previous"], obj.get("hud", False))
            except ValueError:
                if not obj.get("draft"):
                    raise
                result = None
        print(json.dumps(result,ensure_ascii=False),flush=True)

def sentence_count(text):
    return max(1, sum(bool(part.strip()) for part in re.split(r"[.!?…。！？؟]+", text)))


if __name__ == "__main__":
    main()
