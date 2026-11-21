import json
import os
import re
import sys
import time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from hud_cache import HudCache

import ctranslate2
import sentencepiece as spm

# Match complete utterances. "The lights come on" and "there was no sweat
# on his face" need their ordinary meanings.
IDIOMS = {
    "come on": "Ну ж бо",
    "oh come on": "Та ну",
    "come on man": "Та ну, друже",
    "come on in": "Заходь",
    "let's bounce": "Звалюймо",
    "let's roll": "Поїхали",
    "we're cooked": "Нам кінець",
    "you good": "Ти як",
    "you trippin'": "Ти гониш",
    "you trippin": "Ти гониш",
    "cut me some slack": "Не тисни на мене",
    "quit messing with me": "Годі з мене знущатися",
    "no cap": "Без жартів",
    "dunno": "Та не знаю",
    "i dunno": "Та не знаю",
    "beats me": "Гадки не маю",
    "no way": "Та ну",
    "keep it low-key": "Тільки без зайвого шуму",
    "i got your back": "Я тебе прикрию",
    "nice ride": "Класна тачка",
    "that ride is sick": "Оце крута тачка",
    "our ride is here": "Наша тачка вже тут",
    "my bad": "Моя помилка",
    "no sweat": "Без проблем",
    "you gotta be kidding me": "Та ти знущаєшся",
    "you've gotta be kidding me": "Та ти знущаєшся",
    "gimme a hand": "Допоможи мені",
    "lemme see": "Дай глянути",
    "gotcha": "Ага, ясно",
    "ha gotcha": "Ага, попався",
    "gotcha covered": "Я тебе прикрию",
    "i'm beat": "Я без сил",
    "it's a piece of cake": "Це раз плюнути",
    "piece of cake": "Раз плюнути",
    "that tears it": "Ну все, годі",
    "that just bloody tears it": "Ну все, досить із мене",
    "well that just bloody tears it": "Ну все, досить із мене",
}
APOSTROPHES = str.maketrans({"’": "'", "‘": "'", "`": "'", "´": "'"})
COME_ON = re.compile(r"\bc\s*'?\s*mon\b", re.IGNORECASE)
SPEAKER = re.compile(r"^([A-Za-z][A-Za-z .'-]{0,32}:)\s+")
URGING = re.compile(r"^come on[,—–]\s*(?=\w)", re.IGNORECASE)
HESITATION = re.compile(r"^(?:i\s+)?dunno\s*[,—–]\s*(?=\w)", re.IGNORECASE)
REQUEST = re.compile(r"^(gimme|lemme|imma)\b", re.IGNORECASE)
REQUESTS = {"gimme": "Give me", "lemme": "Let me", "imma": "I'm going to"}
CONTEXT_SECONDS = 8
FUNNY_PERSON = re.compile(r"\bfunny(?=\s+(?:guys?|man|woman|person|dude)\b)", re.IGNORECASE)
HUMOUR = re.compile(
    r"\b(?:jokes?|punchlines?|comedian|comedy|hilarious|laugh(?:ed|ing|ter)?|in stitches|cracked me up)\b", re.I
)
DISTRUST = re.compile(
    r"\b(?:suspicious|creepy|shady|gives? me the creeps|something(?:'s| is) off|"
    r"(?:don't|do not|can't|cannot) trust (?:him|her|them)|(?:give|gave|use|used) (?:us |me )?a (?:fake|false) name|"
    r"(?:following|followed|watching|watched) (?:me|us) (?:all night|again|home)|"
    r"(?:his|her|their) story keeps changing)\b", re.I
)
NEGATED_DISTRUST = re.compile(r"\b(?:not|never|isn't|wasn't)\s+(?:(?:at all|that|very)\s+)?(?:suspicious|creepy|shady)\b", re.I)


VOCATIVE = re.compile(r",\s*(bro|man|dude)$", re.IGNORECASE)
ADDRESS = {"bro": "брате", "man": "друже", "dude": "чувак"}

# Disambiguate by neighbouring words before handing longer sentences to MT.
# For example, "we gotta bounce the ball" keeps its literal verb.
CONTEXT_RULES = [
    (re.compile(pattern, re.IGNORECASE), replacement)
    for pattern, replacement in (
        (r"\b(I|we|you|they) gotta bounce(?=\s+(?:before|now|soon|or|if|from here)\b|[,.!?…]|$)", r"\1 need to leave"),
        (r"\bwe're cooked(?=\s+(?:if|unless|when)\b)", "we are in serious trouble"),
        (r"\bfive[- ][o0](?:'s|\s+(?:is|are))(?=\s+(?:here|outside|coming|behind|after)\b)", "the cops are"),
        (r"\bwhere(?:'s| is) (my|your|our|his|her|their) ride\b(?![- ]or[- ]die)", r"where is \1 car"),
        (r"\b(our|my|your|their) ride(?=\s+(?:just arrived|pulled up|is parked)\b)", r"\1 car"),
        (r"\b(my|your|his|her|our|their) ride[- ]or[- ]die\b", r"\1 most loyal friend"),
        (r"\boutta gas\b", "out of fuel"),
        (r"\b(i|we|they|you) dunno\b", r"\1 do not know"),
        (r"^dunno\b", "I don't know"),
    )
]


class DialogueContext:
    """One adjacent English line per worker session; only the current line is rendered."""
    def __init__(self):
        self.previous = ""
        self.last_seen = None
        self.last_key = ""
        self.last_input = ""
        self.resolved = ""

    def resolve(self, text, now):
        # Keep the same interpretation for retries with different OCR punctuation.
        key = " ".join(re.findall(r"\w+", text.casefold()))
        fresh = self.last_seen is not None and now - self.last_seen <= CONTEXT_SECONDS
        if fresh and self.last_key == key and self.last_input == text:
            return self.resolved
        if not fresh or key != self.last_key:
            self.previous = self.last_input if fresh else ""
            self.last_seen = now
        self.last_key = key
        self.last_input = text
        resolved = text
        if FUNNY_PERSON.search(text):
            # Explicit cues in the current line take priority over a previous turn.
            current = text.translate(APOSTROPHES)
            prior = self.previous.translate(APOSTROPHES)
            evidence = current if HUMOUR.search(current) or DISTRUST.search(current) else prior
            if DISTRUST.search(evidence) and not (HUMOUR.search(evidence) or NEGATED_DISTRUST.search(evidence)):
                resolved = FUNNY_PERSON.sub("strange", text)
        self.resolved = resolved
        return resolved


def idiom_for(words):
    return IDIOMS.get(" ".join(words.lower().replace(",", " ").split()))


def subtitle_parts(sentence):
    """Yield (needs_model, text) pairs in display order."""
    normalized = COME_ON.sub("come on", sentence.translate(APOSTROPHES))
    # Windows OCR sometimes reads a lowercase L as an uppercase I.
    normalized = normalized.replace("Iow-key", "low-key").replace("lay Iow", "lay low")
    speaker = SPEAKER.match(normalized)
    body = normalized[speaker.end():] if speaker else normalized
    stem = body.rstrip(".!?…")
    ending = body[len(stem):]
    idiom = idiom_for(stem)
    address = VOCATIVE.search(stem)
    if not idiom and address:
        base = idiom_for(stem[:address.start()])
        if base:
            idiom = f"{base}, {ADDRESS[address[1].lower()]}"
    urging = URGING.match(body)
    hesitation = HESITATION.match(body)
    lead = hesitation or urging
    if idiom or lead:
        if speaker:
            yield True, speaker[1]
        if idiom:
            yield False, idiom + ending
        else:
            yield False, "Та не знаю." if hesitation else "Ну ж бо!"
            rest = body[lead.end():]
            yield from subtitle_parts(rest[:1].upper() + rest[1:])
        return

    capitalize = body[:1].isupper()
    body = REQUEST.sub(lambda match: REQUESTS[match[1].lower()], body)
    for pattern, replacement in CONTEXT_RULES:
        body = pattern.sub(replacement, body)
    if capitalize:
        body = body[:1].upper() + body[1:]
    if speaker and body != normalized[speaker.end():]:
        yield True, speaker[1]
        yield True, body
    else:
        yield True, (normalized[:speaker.end()] if speaker else "") + body


def main():
    root = Path(__file__).resolve().parent / "quickmt-en-uk"
    sys.stdin.reconfigure(encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8")
    os.chdir(root)
    src = spm.SentencePieceProcessor(model_proto=Path("src.spm.model").read_bytes())
    tgt = spm.SentencePieceProcessor(model_proto=Path("tgt.spm.model").read_bytes())
    beam = int(os.environ.get("SUB_BEAM", "3"))
    if beam not in (1, 3, 5):
        raise ValueError("Use beam 1, 3, or 5")
    device = "cpu"
    if device not in ("cuda", "cpu"):
        raise ValueError("Use cuda or cpu")
    compute = "float16" if device == "cuda" else "int8"
    model = ctranslate2.Translator(".", device=device, compute_type=compute, intra_threads=4)

    def translate(text):
        # QuickMT can drop the second sentence after a question or exclamation.
        sentences = re.split(r"(?<=[!?])\s+(?=[A-Z])", text)
        parts = [part for s in sentences for part in subtitle_parts(s)]
        tokens = [src.encode(s, out_type=str)[:254] + ["</s>"] for use_model, s in parts if use_model]
        results = iter(model.translate_batch(tokens, beam_size=beam, max_decoding_length=96) if tokens else [])
        return " ".join(
            tgt.decode(next(results).hypotheses[0]).strip() if use_model else s
            for use_model, s in parts
        )

    app = Path(__file__).resolve().parent
    settings = Path(os.environ.get("SUB_RUNTIME", str(app.parent / "runtime"))) / "languages.json"
    catalog_path = app / "languages.json"
    if not catalog_path.exists():
        catalog_path = app.parent / "launcher" / "languages.json"
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    multi = None
    tokenizer = None

    def target_language():
        override = os.environ.get("SUB_TARGET")
        if override is not None:
            if override not in catalog:
                raise ValueError("Unsupported SUB_TARGET: " + override)
            return override
        try:
            code = json.loads(settings.read_text(encoding="utf-8-sig")).get("target")
            return code if isinstance(code, str) and code in catalog else "uk"
        except (FileNotFoundError, ValueError, AttributeError):
            return "uk"

    selected_model = os.environ.get("SUB_MODEL", "auto")
    if selected_model not in ("auto", "quickmt", "m2m100"):
        raise ValueError("Unknown translation model")

    def render(original, resolved, code):
        nonlocal multi, tokenizer
        if selected_model == "quickmt" and code != "uk":
            raise ValueError("QuickMT EN-UK requires Ukrainian output")
        if code == "en":
            return original
        if code == "uk" and selected_model != "m2m100":
            return translate(resolved)
        if multi is None:
            directory = app / "m2m100"
            tokenizer = spm.SentencePieceProcessor(model_proto=(directory / "sentencepiece.bpe.model").read_bytes())
            multi = ctranslate2.Translator(str(directory), device="cpu", compute_type="int8", intra_threads=4)
        tokens = ["__en__"] + tokenizer.encode(resolved, out_type=str)[:254] + ["</s>"]
        result = multi.translate_batch([tokens], target_prefix=[["__" + code + "__"]],
                                       beam_size=beam, max_decoding_length=96)
        return tokenizer.decode(result[0].hypotheses[0][1:]).strip()

    render("Ready.", "Ready.", target_language())
    hud_cache = HudCache([selected_model, beam], [root/"model.bin", app/"m2m100"/"model.bin"])
    print(json.dumps({"device": "CPU", "model": selected_model,
                      "precision": model.compute_type, "beam": beam}), flush=True)
    dialogue = DialogueContext()
    zones = {}
    try:
        for line in sys.stdin:
            request = json.loads(line)
            if isinstance(request, dict):
                original = request["text"]
                if request.get("hud") or request.get("legend") or request.get("panel"):
                    code = target_language()
                    kind = "map" if request.get("legend") else "panel" if request.get("panel") else "hud"
                    key = hud_cache.key(code, request["zone"], kind, original, request.get("nearby", ""))
                    print(json.dumps(hud_cache.single(key, lambda: render(original, original, code)), ensure_ascii=False), flush=True)
                    continue
                key = request["zone"]
                if key not in zones:
                    zones[key] = DialogueContext()
                context = zones[key]
            else:
                original, context = request, dialogue
            source = context.resolve(original, time.monotonic())
            print(json.dumps(render(original, source, target_language()), ensure_ascii=False), flush=True)
    finally:
        hud_cache.close()

if __name__ == "__main__":
    main()
