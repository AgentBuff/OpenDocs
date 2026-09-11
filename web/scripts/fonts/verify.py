"""Offline integrity audit of all shipped font faces, CSS references and licenses."""
import hashlib, json, pathlib, re
root = pathlib.Path(__file__).resolve().parents[2]
public = root / 'apps/editor/public/fonts'
manifest = json.loads((public / 'manifest.json').read_text())
assert len({font['family'] for font in manifest}) == len(manifest)
count = total = 0
for font in manifest:
    directory = public / font['id']
    assert (directory / 'LICENSE').stat().st_size > 100, font['id']
    css = (directory / 'font.css').read_text()
    assert f'font-family: "{font["family"]}"' in css, font['id']
    references = re.findall(r'url\("\./([^"/]+)"\)', css)
    assert sorted(references) == sorted(face['file'] for face in font['faces']), font['id']
    assert 'http:' not in css and 'https:' not in css, font['id']
    for face in font['faces']:
        data = (directory / face['file']).read_bytes()
        assert data[:4] == b'wOF2', face['file']
        assert len(data) == face['bytes'], face['file']
        assert hashlib.sha256(data).hexdigest() == face['sha256'], face['file']
        count += 1; total += len(data)
print(f'Validated {len(manifest)} families, {count} WOFF2 files, {total:,} bytes; licenses and local CSS references present.')
