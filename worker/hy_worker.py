"""Translate using the llama.cpp instance owned by the Rust engine."""
import json
import os
import re
import sys
import time
from http.client import HTTPException
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, ProxyHandler, build_opener
sys.path.insert(0, str(Path(__file__).resolve().parent))
from hud_cache import HudCache

MIN_ECHO_LENGTH = 8
MAX_RECENT_TRANSLATIONS = 6
SUBJECTLESS_STATEMENT = re.compile(r"(?:wanna|gotta|gonna)\b[^.?!]*\.\s*$", re.I)
SPEAKER_LINE = re.compile(r"([^\W\d_][\w .’'-]{0,35}):\s+(.+)", re.UNICODE)
SUBJECT_PERSON = {"she":"third-person feminine singular", "he":"third-person masculine singular",
                  "we":"first-person plural", "i":"first-person singular", "you":"second person"}


def dialogue_source(text):
    speaker = SPEAKER_LINE.fullmatch(text)
    body = speaker.group(2) if speaker else text
    notes = []
    if speaker:
        first = re.match(r"[a-z]+", body, re.I)
        person = SUBJECT_PERSON.get(first.group().lower()) if first else None
        if person:
            notes.append(f"Keep the explicit subject and the {person} point of view.")
        else:
            notes.append(f"The speaker is {speaker.group(1)}. Translate only their spoken words. "
                         "Preserve the original point of view, including an implied subject.")
    if SUBJECTLESS_STATEMENT.fullmatch(body):
        notes.append("Keep statements declarative. If the speaker and a companion must act together, "
                     "explicitly say we. For individual actions and predictions, infer the subject from context.")
    return body, " ".join(notes)


def words(text):
    return " ".join(re.findall(r"[^\W_]+", text.casefold()))


def echoes_history(result, text, previous, translations):
    parts = [words(part) for part in re.split(r"[.!?…。！？؟\n]+", result)]
    parts = [part for part in parts if part]
    if len(parts) < 2:
        return False
    current = f" {words(text)} "
    for source in previous.splitlines():
        if f" {words(source)} " in current:
            continue
        for prior in (source, translations.get(source, "")):
            normalized = words(prior)
            if len(normalized) >= MIN_ECHO_LENGTH and normalized in parts:
                return True
    return False


def retry_dialogue(complete, text, previous, language, known):
    body, notes = dialogue_source(text)
    prompt = (f"Translate only the final line of game dialogue into natural {language}. "
              "Use the earlier lines to interpret the speaker's meaning. Output only the translation."
              f" {notes}\n\nEarlier lines:\n{previous}\n\nFinal line:\n{body}")
    try:
        result = complete([{"role": "user", "content": prompt}])
    except HTTPError as error:
        if error.code != 400:
            raise
        return None
    except ValueError:
        return None
    if echoes_history(result, text, previous, known):
        return None
    return " ".join(result.splitlines())


def translate_labels(complete, labels, language, map_controls, stream=None, progress=None):
    kind = "map controls" if map_controls else "HUD labels"
    instruction = (f"Translate these game {kind} into {language}. "
                   "Use concise menu labels and infinitives for actions. Keep each number with its translation. "
                   "Output only the numbered lines.")
    lines = [f"{i}. {label}" for i, label in enumerate(labels, 1)]
    translations = {}
    messages = [{"role": "user", "content": "\n".join([instruction, "", *lines])}]
    def accept(line):
        match = re.fullmatch(r"\s*([1-9])\s*[.):]\s*(\S.*?)\s*", line)
        if not match:
            return False
        index = int(match[1])
        if index > len(labels) or index in translations:
            return False
        translations[index] = match[2]
        if progress and len(translations) < len(labels):
            progress(index - 1, match[2])
        return True
    try:
        lines = stream(messages) if stream else complete(messages).strip().splitlines()
        for line in lines:
            if not accept(line):
                return None
    except (ValueError, OSError, HTTPException, KeyError, IndexError):
        return None
    if len(translations) == len(labels):
        return [translations[i] for i in range(1, len(labels) + 1)]
    return None


def main():
    sys.stdin.reconfigure(encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8")
    languages = json.loads(Path(__file__).with_name("hy-languages.json").read_text("utf8"))
    target = os.environ["SUB_TARGET"]
    language = languages[target]
    model = os.environ.get("SUB_MODEL","hy").removeprefix("gguf:")
    label = "Hy-MT2-1.8B Q6_K" if model == "hy" else model
    ukrainian_7b = target == "uk" and model.casefold() == "hy-mt2-7b-q4_k_m.gguf"
    concise_dialogue = ukrainian_7b and os.environ.get("SUB_DIALOGUE_STYLE") == "concise"
    instruction = (
        f"Translate this line of game dialogue into natural, conversational {language}. Preserve the intended meaning and tone. Output only the translated line."
        if ukrainian_7b
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

    def open_completion(messages, stream):
        payload = {"messages":messages,"temperature":0,
                   "max_tokens":128,"stream":stream,"cache_prompt":True}
        request = Request(endpoint + "/v1/chat/completions", data=json.dumps(payload).encode(), headers=auth)
        return client.open(request,timeout=45)

    def stream_lines(messages):
        pending = ""
        stopped = False
        with open_completion(messages, True) as response:
            for raw in response:
                if not raw.startswith(b"data:"):
                    continue
                data = raw[5:].strip()
                if data == b"[DONE]":
                    break
                choice = json.loads(data)["choices"][0]
                pending += choice["delta"].get("content") or ""
                while "\n" in pending:
                    line, pending = pending.split("\n", 1)
                    yield line
                stopped = stopped or choice.get("finish_reason") == "stop"
        if not stopped:
            raise ValueError("Incomplete HUD stream")
        if pending.strip():
            yield pending

    def complete(messages):
        with open_completion(messages, False) as res:
            choice = json.load(res)["choices"][0]
        result = choice["message"]["content"].strip()
        if not result or choice["finish_reason"] == "length":
            raise ValueError(f"{label} did not finish the subtitle")
        return result

    def stream_completion(messages, text, previous, recent, zone):
        contents = ""
        finish = None
        checked = False
        with open_completion(messages, True) as response:
            for raw in response:
                if not raw.startswith(b"data:"):
                    continue
                data = raw[5:].strip()
                if data == b"[DONE]":
                    break
                packet = json.loads(data)
                choice = packet["choices"][0]
                contents += choice["delta"].get("content") or ""
                finish = choice.get("finish_reason") or finish
                if checked:
                    continue
                boundary = re.search(r'(?<=[.!?])\s+(?=[А-ЯІЇЄҐA-Z«“—])',contents)
                if boundary:
                    checked = True
                    prefix = contents[:boundary.start()].strip()
                    prior = {words(piece) for turn in previous.splitlines() for known in (turn, recent.get(turn, ""))
                             for piece in re.split(r"[.!?…。！？؟\n]+",known)}
                    if words(prefix) not in prior and len(words(prefix)) >= 5:
                        print(json.dumps(dict(zone=zone,text=text,previous=previous,sentence=prefix),
                                         ensure_ascii=False),flush=True)
        if finish != "stop" or not contents.strip():
            raise ValueError("Incomplete dialogue")
        return contents.strip()

    recent_by_zone = {}
    app = Path(__file__).resolve().parent
    weights = app/"hy-runtime"/"Hy-MT2-1.8B-Q6_K.gguf" if model == "hy" else app/"models"/model
    cache = HudCache(model, [weights])

    def remember(zone, text, previous, result):
        known = recent_by_zone.setdefault(zone, {})
        if not previous:
            known.clear()
        known.pop(text, None)
        known[text] = result
        if len(known) > MAX_RECENT_TRANSLATIONS:
            del known[next(iter(known))]

    def translate(text, previous, hud=False, zone=None, legend=False, nearby="", panel=False, preview=False, stream_sentence=False):
        if panel and text.isupper() and len(text.split()) <= 5:
            controls = ", ".join(label for label in nearby.splitlines() if label.isupper())
            prompt = (f"Other actions available to the player here: {controls}. Translate only the following "
                      f"in-world player interaction into natural {language}. Use a short verb in the infinitive "
                      "for actions and a noun for people or things. No explanations.\n\n" + text)
            return complete([{"role":"user","content":prompt}])
        if legend:
            others = ", ".join(nearby.splitlines())
            prompt = (f"You are localizing a game's interactive map. Write the shortest clear {language} "
                      f"interface label for the selected control. Other controls: {others}. "
                      f"Output only the localized label.\n\n{text}")
            return complete([{"role": "user", "content": prompt}])
        if hud:
            prompt = (f"Localize the following video game interface text into {language}. "
                      "For a menu action use a concise button label. Keep instructions clear. "
                      "Output only the translation.\n\n" + text)
            return complete([{"role":"user","content":prompt}])
        body, notes = dialogue_source(text)
        voice = " " + notes if notes else ""
        current = {"role":"user","content":f"{instruction}{voice}\n\n{body}"}
        # Limit comparisons to the context selected by this zone's Rust stabilizer.
        known = recent_by_zone.get(zone, {}) if previous else {}
        recent = {line: known[line] for line in previous.splitlines() if line in known}
        def dialogue_completion(messages):
            if stream_sentence and ukrainian_7b:
                return stream_completion(messages, text, previous, recent, zone)
            return complete(messages)

        result = None
        if previous:
            if concise_dialogue:
                background = f"Previous dialogue:\n{previous}"
                prompt = f"Translate only this subtitle into concise, natural {language}. Keep the meaning and tone."
                contextual_line = {"role": "user", "content": f"{prompt}{voice}\n\n{body}"}
            else:
                background = "Previous dialogue: " + " ".join(previous.splitlines())
                contextual_line = current
            conversation = [
                {"role": "user", "content": background},
                {"role": "assistant", "content": "OK."},
                contextual_line,
            ]
            try:
                contextual = dialogue_completion(conversation)
                if (concise_dialogue or "\n" not in contextual) and not echoes_history(
                    contextual, text, previous, recent
                ):
                    result = " ".join(contextual.splitlines()) if concise_dialogue else contextual
            except HTTPError as error:
                if error.code != 400:
                    raise
            except ValueError:
                pass
            if result is None:
                result = retry_dialogue(complete, text, previous, language, recent)
        if result is None:
            result = complete([current]) if previous else dialogue_completion([current])
        # Keep older translations available if Rust selects their source again.
        if not preview:
            remember(zone, text, previous, result)
        return result

    translate("Ready.", "")
    print(json.dumps({"model":label,"target":language,"device":"CUDA","hud_batch":True}),flush=True)
    previous, last = "", 0
    previews = {}
    try:
        for line in sys.stdin:
            obj = json.loads(line)
            if isinstance(obj, str):
                now = time.monotonic()
                result = translate(obj, previous if now-last <= 8 else "")
                previous, last = obj, now
            elif "hud_labels" in obj:
                labels = obj["hud_labels"]
                kind = "map" if obj.get("map") else "panel"
                near = obj.get("contexts", [""] * len(labels))
                keys = [cache.key(target, obj.get("zone"), kind, s, ctx) for s, ctx in zip(labels, near)]
                result = [cache.get(k) for k in keys]
                missing = [i for i, value in enumerate(result) if value is None]
                if missing:
                    live = obj.get("stream") and os.environ.get("SUB_HUD_STREAM") != "0"
                    def emit(i, text):
                        print(json.dumps({"row":i,"text":text},ensure_ascii=False),flush=True)
                    if live:
                        for i, value in enumerate(result):
                            if value is not None:
                                emit(i, value)
                    if len(missing) == 1:
                        i = missing[0]
                        values = [translate(labels[i], "", True, obj.get("zone"), kind=="map", near[i], kind=="panel")]
                    else:
                        values = translate_labels(complete, [labels[i] for i in missing], language, kind=="map",
                                                  stream_lines if live else None,
                                                  (lambda n,s: emit(missing[n],s)) if live else None)
                    if values:
                        for i, value in zip(missing, values):
                            result[i] = value
                        cache.save([(keys[i], result[i]) for i in missing])
                    else:
                        result = None
            elif obj.get("hud"):
                kind = "map" if obj.get("legend") else "panel" if obj.get("panel") else "hud"
                key = cache.key(target, obj.get("zone"), kind, obj["text"], obj.get("nearby", ""))
                result = cache.single(key, lambda: translate(obj["text"], "", True, obj.get("zone"),
                                      obj.get("legend", False), obj.get("nearby", ""), obj.get("panel", False)))
            else:
                zone = obj.get("zone")
                key = (obj["text"], obj["previous"])
                preview = obj.get("zone_preview", False)
                saved = previews.pop(zone, None)
                try:
                    if obj.get("accept_preview") and saved and saved[0] == key:
                        result = saved[1]
                        remember(zone, *key, result)
                    else:
                        result = translate(*key, obj.get("hud", False), zone, obj.get("legend", False),
                                           obj.get("nearby", ""), obj.get("panel", False), preview,
                                           obj.get("stream_sentence", False))
                    if preview:
                        previews[zone] = (key, result)
                        result = None
                except ValueError:
                    if not (preview or obj.get("draft")):
                        raise
                    result = None
                except (URLError, TimeoutError):
                    if not preview:
                        raise
                    result = None
            print(json.dumps(result,ensure_ascii=False),flush=True)
    finally:
        cache.close()

if __name__ == "__main__":
    main()
