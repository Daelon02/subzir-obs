import json
import os
import sys
from pathlib import Path

import ctranslate2
import sentencepiece as spm

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
        tokens = src.encode(text, out_type=str)[:254] + ["</s>"]
        result = model.translate_batch([tokens], beam_size=beam, max_decoding_length=96)
        return tgt.decode(result[0].hypotheses[0]).strip()

    translate("Ready.")
    print(json.dumps({"device": "CPU", "model": "QuickMT EN-UK",
                      "precision": model.compute_type, "beam": beam}), flush=True)
    for line in sys.stdin:
        print(json.dumps(translate(json.loads(line)), ensure_ascii=False), flush=True)

if __name__ == "__main__":
    main()
