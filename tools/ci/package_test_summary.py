#!/usr/bin/env python3
"""Preserve adversarial filenames inside a portable CI artifact container.

The application gate intentionally creates CR/LF filenames on POSIX. GitHub's
artifact service rejects those names even when the tests passed. Keep their
exact bytes and names in a tar member, excluding reproducible compiler caches.
No acceptance input, output, oracle or result is changed.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
from pathlib import Path
import tarfile

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, default=ROOT / 'compiler/target/ci/test-summary')
    parser.add_argument('--output', type=Path, default=ROOT / 'compiler/target/ci/test-summary.tar.gz')
    parser.add_argument('--prefix', default='test-summary', help='Single directory name inside the archive')
    args = parser.parse_args()
    if args.prefix in ('', '.', '..') or '/' in args.prefix or '\\' in args.prefix:
        parser.error('--prefix must be one nonempty directory name')
    source, output = args.input.resolve(), args.output.resolve()
    if not source.exists():
        print('No application evidence directory was produced; nothing to package.')
        return 0
    if not source.is_dir() or output.is_relative_to(source):
        raise ValueError('Evidence must be a directory and its archive must be outside it')
    output.parent.mkdir(parents=True, exist_ok=True)
    members = [p for p in source.rglob('*') if 'empty-caches' not in p.relative_to(source).parts]
    files, directories, size = 0, 0, 0
    temporary = output.with_suffix(output.suffix + '.part')
    with temporary.open('wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as archive:
            for path in sorted(members):
                if path.is_symlink():
                    raise ValueError('Unexpected symlink in application evidence')
                if not path.is_file() and not path.is_dir():
                    raise ValueError('Unexpected special file in application evidence')
                info = archive.gettarinfo(str(path), arcname=args.prefix + '/' + path.relative_to(source).as_posix())
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ''
                if info.isdir():
                    archive.addfile(info)
                    directories += 1
                else:
                    with path.open('rb') as content:
                        archive.addfile(info, content)
                    files += 1
                    size += info.size
    temporary.replace(output)
    digest = hashlib.sha256()
    with output.open('rb') as content:
        for block in iter(lambda: content.read(1024 * 1024), b''):
            digest.update(block)
    schema = 'yavo-test-summary-evidence-package/1' if args.prefix == 'test-summary' else 'yavo-application-evidence-package/1'
    result = {'schema': schema, 'archive': output.name, 'member_prefix': args.prefix,
              'bytes': output.stat().st_size, 'sha256': digest.hexdigest(),
              'files': files, 'directories': directories, 'payload_bytes': size, 'excluded': ['empty-caches'],
              'preserved': 'Original file contents and relative names, including POSIX control characters'}
    output.with_suffix(output.suffix + '.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
