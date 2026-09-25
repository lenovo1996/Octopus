import importlib.util
import json
import os
import subprocess
import tempfile
import tomllib
from pathlib import Path

repo = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('prepare_release', repo / 'scripts/prepare-release.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
files = ['package.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock']
original = {f: (repo / f).read_bytes() for f in files}
base_version = json.loads(original['package.json'])['version']
major, minor, patch = base_version.split('.')
checks = 0

def copy_source(root):
    (root / 'src-tauri').mkdir(parents=True)
    for f, content in original.items():
        (root / f).write_bytes(content)

for number in ['1', '42']:
    with tempfile.TemporaryDirectory(prefix='octopus-version-test-') as directory:
        root = Path(directory)
        copy_source(root)
        expected = f'{major}.{minor}.{number}'
        assert module.prepare_release(root, number) == expected
        assert json.loads((root / files[0]).read_text())['version'] == expected
        assert json.loads((root / files[1]).read_text())['version'] == expected
        assert tomllib.loads((root / files[2]).read_text())['package']['version'] == expected
        before = tomllib.loads(original[files[3]].decode())['package']
        after = tomllib.loads((root / files[3]).read_text())['package']
        for old, new in zip(before, after, strict=True):
            if old['name'] == 'octopus':
                assert new['version'] == expected
                new['version'] = old['version']
            assert new == old
        checks += 1

for case in ['zero', 'injection', 'mismatch', 'prerelease']:
    with tempfile.TemporaryDirectory(prefix='octopus-version-test-') as directory:
        root = Path(directory)
        copy_source(root)
        number = {'zero': '0', 'injection': '1; echo unsafe'}.get(case, '9')
        if case in ['mismatch', 'prerelease']:
            package = json.loads((root / 'package.json').read_text())
            package['version'] = f'{major}.{minor}.{int(patch) + 1}' if case == 'mismatch' else f'{base_version}-rc.1'
            (root / 'package.json').write_text(json.dumps(package))
        snapshot = {f: (root / f).read_bytes() for f in files}
        try:
            module.prepare_release(root, number)
            raise AssertionError('Expected invalid version/run rejection')
        except ValueError:
            pass
        assert snapshot == {f: (root / f).read_bytes() for f in files}
        checks += 1

stub = '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
mode = os.environ['TEST_RELEASE_MODE']
with open('gh-calls.jsonl', 'a') as stream:
    stream.write(json.dumps(args) + '\\n')
if args[0] == 'api':
    if '--method' in args:
        assert 'draft=false' in args and 'prerelease=false' in args and 'make_latest=legacy' in args
        Path('published').touch()
    elif mode == 'badtag':
        print('b' * 40)
    else:
        print('gh: Forbidden (HTTP 403)' if mode == 'api-failure' else 'gh: Not Found (HTTP 404)', file=sys.stderr)
        sys.exit(1)
elif args[:2] == ['release', 'view']:
    if 'databaseId' in args:
        print('7')
    elif mode in ['new', 'upload-failure'] and not Path('draft').exists():
        print('release not found', file=sys.stderr)
        sys.exit(1)
    else:
        print(json.dumps({'isDraft': mode != 'published', 'targetCommitish': ('b' if mode == 'badcommit' else 'a') * 40}))
elif args[:2] == ['release', 'create']:
    assert '--draft' in args and args[args.index('--target') + 1] == 'a' * 40
    Path('draft').touch()
    if mode == 'upload-failure':
        sys.exit(1)
elif args[:2] == ['release', 'upload']:
    assert '--clobber' in args
else:
    raise AssertionError(args)
'''

for case in ['new', 'draft', 'published', 'badcommit', 'badtag', 'corrupt', 'upload-failure', 'missing', 'api-failure']:
    with tempfile.TemporaryDirectory(prefix='octopus-publish-test-') as directory:
        root = Path(directory)
        (root / 'bin').mkdir()
        gh = root / 'bin/gh'
        gh.write_text(stub)
        gh.chmod(0o755)
        assets = root / 'release-assets'
        assets.mkdir()
        names = ['Octopus_0.1.42_amd64.deb', 'Octopus-0.1.42-1.x86_64.rpm', 'Octopus_0.1.42_amd64.AppImage']
        for name in names:
            (assets / name).write_text(name)
        checksums = subprocess.check_output(['sha256sum', *names], cwd=assets)
        (assets / 'SHA256SUMS').write_bytes(checksums)
        if case == 'corrupt':
            (assets / names[0]).write_text('corrupted download')
        if case == 'missing':
            (assets / names[0]).unlink()
        env = dict(os.environ, PATH=str(root / 'bin') + os.pathsep + os.environ['PATH'],
                   GH_REPO='test/octopus', GH_TOKEN='local-stub', GITHUB_SHA='a' * 40,
                   RELEASE_VERSION='0.1.42', TEST_RELEASE_MODE=case)
        result = subprocess.run(['bash', str(repo / 'scripts/publish-release.sh')], cwd=root, env=env, capture_output=True, text=True)
        should_pass = case in ['new', 'draft', 'published']
        assert (result.returncode == 0) == should_pass, (case, result.stderr)
        assert (root / 'published').exists() == (case in ['new', 'draft']), case
        calls = [json.loads(line) for line in (root / 'gh-calls.jsonl').read_text().splitlines()]
        if case == 'published':
            assert not any(call[:2] in [['release', 'create'], ['release', 'upload']] for call in calls)
        checks += 1

assert original == {f: (repo / f).read_bytes() for f in files}
print(f'{checks} release checks passed; no live GitHub calls and source versions unchanged.')
