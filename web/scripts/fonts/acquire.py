"""Reproducibly acquire real, licensed Fontsource WOFF2 faces for the editor.
Run from any directory. Existing metadata locks/assets are reused; pass --refresh
explicitly to resolve newer package versions. No browser runtime CDN dependency.
"""
import argparse, concurrent.futures, hashlib, json, pathlib, subprocess, re

ROOT = pathlib.Path(__file__).resolve().parents[2]
PUBLIC = ROOT / 'apps/editor/public/fonts'
LOCK = pathlib.Path(__file__).with_name('catalog.lock.json')
DEFAULT = '''noto-sans-sc noto-serif-sc noto-sans-tc noto-serif-tc zcool-xiaowei zcool-qingke-huangyou zcool-kuaile ma-shan-zheng long-cang zhi-mang-xing liu-jian-mao-cao lxgw-wenkai-tc inter roboto open-sans lato montserrat nunito nunito-sans raleway poppins work-sans source-sans-3 public-sans dm-sans manrope barlow rubik ubuntu fira-sans oswald archivo pt-sans noto-sans noto-serif source-serif-4 merriweather lora playfair-display libre-baskerville libre-caslon-text eb-garamond cormorant-garamond crimson-pro bitter pt-serif roboto-slab zilla-slab arvo spectral jetbrains-mono fira-code source-code-pro ibm-plex-mono roboto-mono space-mono inconsolata ubuntu-mono noto-sans-mono ibm-plex-sans ibm-plex-serif bebas-neue anton dancing-script pacifico caveat permanent-marker great-vibes amatic-sc patrick-hand noto-sans-jp noto-serif-jp noto-sans-kr noto-serif-kr noto-naskh-arabic noto-sans-arabic noto-sans-devanagari noto-serif-devanagari noto-sans-thai noto-serif-thai noto-sans-hebrew chocolate-classical-sans huninn iansui atkinson-hyperlegible atkinson-hyperlegible-next lexend plus-jakarta-sans outfit figtree onest geist geist-mono bricolage-grotesque instrument-sans instrument-serif fraunces bodoni-moda domine cardo vollkorn alegreya alegreya-sans assistant heebo cairo tajawal amiri sarabun kanit prompt baloo-2 hind tiro-devanagari-hindi'''.split()

def fetch(url, path):
    if path.exists() and path.stat().st_size: return
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_suffix(path.suffix + '.tmp')
    subprocess.run(['curl', '-sSfL', '--retry', '5', '--retry-all-errors', '--max-time', '180', url, '-o', str(temp)], check=True)
    temp.replace(path)

def family(entry):
    id, meta = entry
    directory = PUBLIC / id
    base = f'https://cdn.jsdelivr.net/npm/@fontsource/{id}@{meta["npmVersion"]}'
    fetch(base + '/LICENSE', directory / 'LICENSE')
    license_text = (directory / 'LICENSE').read_text()
    if not license_text.strip(): raise RuntimeError(f'{id}: missing license')
    weights = sorted(set([min(meta['weights'], key=lambda w:abs(w-400)), min(meta['weights'], key=lambda w:abs(w-700))]))
    listing = pathlib.Path(__file__).with_name('.cache') / f'{id}-{meta["npmVersion"]}-files.json'
    fetch(f'https://data.jsdelivr.com/v1/package/npm/@fontsource/{id}@{meta["npmVersion"]}/flat', listing)
    package_files = {entry['name'] for entry in json.loads(listing.read_text())['files']}
    jobs = []
    for weight in weights:
        for style, subsets in meta['variants'][str(weight)].items():
            for subset in subsets:
                file = f'{id}-{subset}-{weight}-{style}.woff2'
                if '/files/' + file in package_files:
                    jobs.append((weight, style, subset, file))
    if not any(weight == weights[0] and style == 'normal' for weight, style, subset, file in jobs):
        raise RuntimeError(f'{id}: package contains no regular face')
    def face(job):
        weight, style, subset, file = job
        path = directory / file
        fetch(base + '/files/' + file, path)
        data = path.read_bytes()
        if data[:4] != b'wOF2': raise RuntimeError(f'{id}/{file}: invalid WOFF2')
        return {'file':file, 'weight':weight, 'style':style, 'subset':subset, 'sha256':hashlib.sha256(data).hexdigest(), 'bytes':len(data)}
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        faces = list(pool.map(face, jobs))
    css = []
    for face in faces:
        unicode = meta.get('unicodeRange', {}).get(face['subset'])
        css.append('@font-face {\n' + f'  font-family: "{meta["family"]}";\n  font-style: {face["style"]};\n  font-weight: {face["weight"]};\n  font-display: swap;\n  src: url("./{face["file"]}") format("woff2");\n' + (f'  unicode-range: {unicode};\n' if unicode else '') + '}')
    (directory / 'font.css').write_text('\n'.join(css)+'\n')
    print(f'{id}: {len(faces)} verified faces, {sum(f["bytes"] for f in faces)} bytes', flush=True)
    return {'id':id, 'family':meta['family'], 'category':meta['category'], 'subsets':meta['subsets'], 'license':meta['license'], 'version':meta['npmVersion'], 'source':meta['source'], 'css':f'/fonts/{id}/font.css', 'faces':faces}

if __name__ == '__main__':
    parser = argparse.ArgumentParser(); parser.add_argument('--ids', nargs='+'); parser.add_argument('--refresh', action='store_true'); args = parser.parse_args()
    ids = args.ids or DEFAULT
    lock = json.loads(LOCK.read_text()) if LOCK.exists() else {}
    for id in ids:
        if args.refresh or id not in lock:
            cache = pathlib.Path(__file__).with_name('.cache') / f'{id}.json'
            if args.refresh: cache.unlink(missing_ok=True)
            fetch(f'https://api.fontsource.org/v1/fonts/{id}', cache)
            lock[id] = json.loads(cache.read_text())
            if lock[id]['license'] not in ['OFL-1.1', 'Apache-2.0', 'Ubuntu-font-1.0', 'UFL-1.0']: raise RuntimeError(f'Unreviewed license: {id} {lock[id]["license"]}')
            LOCK.write_text(json.dumps(lock, ensure_ascii=False, indent=2)+'\n')
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        catalog = list(pool.map(family, [(id, lock[id]) for id in ids]))
    (PUBLIC / 'manifest.json').write_text(json.dumps(catalog, ensure_ascii=False, indent=2)+'\n')
    compact = [{key:value for key,value in item.items() if key != 'faces'} for item in catalog]
    (ROOT / 'apps/editor/src/typography/font-assets.json').write_text(json.dumps(compact, ensure_ascii=False, indent=2)+'\n')
    print(f'Complete: {len(catalog)} families', flush=True)
