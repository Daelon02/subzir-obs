from pathlib import Path
import hashlib, json, urllib.request, tempfile
from concurrent.futures import ThreadPoolExecutor
REPO = "quickmt/quickmt-en-uk"
REV = "a73137905668a904476c3458f7556b8139fd92fc"
FILES = ("model.bin", "config.json", "src.spm.model", "tgt.spm.model", "source_vocabulary.json", "target_vocabulary.json", "quickmt-config.yaml", "README.md")
def main():
    directory = Path(__file__).resolve().parent / "quickmt-en-uk"
    if directory.exists():
        print(directory)
        return
    with urllib.request.urlopen(f"https://huggingface.co/api/models/{REPO}/revision/{REV}?blobs=true", timeout=30) as r:
        metadata = {f["rfilename"]: f for f in json.load(r)["siblings"]}
    with tempfile.TemporaryDirectory(dir=directory.parent) as tmp:
        staged = Path(tmp) / "model"
        staged.mkdir()
        def fetch(name):
            file = staged / name
            urllib.request.urlretrieve(f"https://huggingface.co/{REPO}/resolve/{REV}/{name}", file)
            info = metadata[name]
            if file.stat().st_size != info["size"]:
                raise IOError(f"Incomplete {name}")
            if "lfs" in info:
                with file.open("rb") as stream:
                    if hashlib.file_digest(stream, "sha256").hexdigest() != info["lfs"]["sha256"]:
                        raise IOError(f"Checksum mismatch for {name}")
        with ThreadPoolExecutor(max_workers=3) as pool:
            list(pool.map(fetch, FILES))
        staged.rename(directory)
    print(directory)
if __name__=="__main__":
    main()
