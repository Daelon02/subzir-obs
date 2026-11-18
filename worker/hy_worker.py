"""Translate using the llama.cpp instance owned by the Rust engine."""
import json
import os
import sys
import time
from pathlib import Path
from urllib.error import URLError
from urllib.request import Request, ProxyHandler, build_opener

def main():
    sys.stdin.reconfigure(encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8")
    languages = json.loads(Path(__file__).with_name("hy-languages.json").read_text("utf8"))
    language = languages[os.environ["SUB_TARGET"]]
    model = os.environ.get("SUB_MODEL","hy").removeprefix("gguf:")
    label = "Hy-MT2-1.8B Q6_K" if model == "hy" else model
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

    def translate(text, previous):
        if previous:
            prompt = (f"Translate the following English video game subtitle into natural {language}. "
                      f"Previous line for context: “{previous}”. Translate only this line:\n\n{text}")
        else:
            prompt = f"Translate the following segment into {language}, without additional explanation.\n\n{text}"
        payload = {"messages":[{"role":"user","content":prompt}],"temperature":0,
                   "max_tokens":128,"stream":False,"cache_prompt":True}
        request = Request(endpoint + "/v1/chat/completions", data=json.dumps(payload).encode(), headers=auth)
        with client.open(request,timeout=45) as res:
            choice = json.load(res)["choices"][0]
        result = choice["message"]["content"].strip()
        if not result or choice["finish_reason"] == "length":
            raise ValueError(f"{label} did not finish the subtitle")
        return result

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
                result = translate(obj["text"], obj["previous"])
            except ValueError:
                if not obj.get("draft"):
                    raise
                result = None
        print(json.dumps(result,ensure_ascii=False),flush=True)

if __name__ == "__main__":
    main()
