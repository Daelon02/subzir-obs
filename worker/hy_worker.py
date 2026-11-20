"""Translate using the llama.cpp instance owned by the Rust engine."""
import json
import os
import re
import sys
import time
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, ProxyHandler, build_opener

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

    recent_by_zone = {}

    def translate(text, previous, hud=False, zone=None, legend=False, nearby="", panel=False):
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
        known = recent_by_zone.setdefault(zone, {})
        if not previous:
            known.clear()
        recent = {line: known[line] for line in previous.splitlines() if line in known}
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
                contextual = complete(conversation)
                if concise_dialogue and not echoes_history(contextual, text, previous, recent):
                    result = " ".join(contextual.splitlines())
                elif (not concise_dialogue and "\n" not in contextual
                      and sentence_count(contextual) <= sentence_count(text)
                      and not echoes_history(contextual, text, previous, recent)):
                    result = contextual
            except HTTPError as error:
                if error.code != 400:
                    raise
            except ValueError:
                pass
            if result is None:
                result = retry_dialogue(complete, text, previous, language, recent)
        if result is None:
            result = complete([current])
        # Keep older translations available if Rust selects their source again.
        known.pop(text, None)
        known[text] = result
        if len(known) > MAX_RECENT_TRANSLATIONS:
            del known[next(iter(known))]
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
                result = translate(obj["text"], obj["previous"], obj.get("hud", False),
                                   obj.get("zone"), obj.get("legend", False), obj.get("nearby", ""),
                                   obj.get("panel", False))
            except ValueError:
                if not obj.get("draft"):
                    raise
                result = None
        print(json.dumps(result,ensure_ascii=False),flush=True)

def sentence_count(text):
    return max(1, sum(bool(part.strip()) for part in re.split(r"[.!?…。！？؟]+", text)))


if __name__ == "__main__":
    main()
