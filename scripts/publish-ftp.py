"""Publish the release archives of a successful `release` run over explicit FTPS.

Usage:
  publish-ftp.py --validate-run        check BUILD_RUN_ID (or pick the latest
                                       successful release run when it is empty)
                                       and write run_id / sha / version to
                                       $GITHUB_OUTPUT
  publish-ftp.py ARTIFACT_DIRECTORY    upload the archives and documents found
                                       there to FTP_DIRECTORY
"""
import ftplib
import json
import os
import re
from pathlib import Path, PurePosixPath
import ssl
import sys
import urllib.request

WORKFLOW = '.github/workflows/release.yml'
ARCHIVE = re.compile(r'^lz80n-(\d+)\.(\d+)\.(\d+)-[a-z0-9]+-[a-z0-9]+\.(?:zip|tar\.gz)$', re.IGNORECASE)
DOCUMENTS = {'README.txt', 'CHANGELOG.md', 'LICENSE', 'SHA256SUMS.txt'}
REQUIRED_PLATFORMS = ['windows-x64', 'windows-arm64', 'macos-x64', 'macos-arm64',
                      'linux-x64', 'linux-arm64', 'zxnext-sdcard']


def github(path):
    request = urllib.request.Request(
        f'https://api.github.com/repos/{os.environ["GH_REPOSITORY"]}/{path}',
        headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'],
                 'Accept': 'application/vnd.github+json',
                 'X-GitHub-Api-Version': '2022-11-28'},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def validate_run():
    run_id = os.environ.get('BUILD_RUN_ID', '').strip()
    if not run_id:
        runs = github('actions/workflows/release.yml/runs?status=success&per_page=1')['workflow_runs']
        if not runs:
            raise ValueError('No successful release run found.')
        run_id = str(runs[0]['id'])
        print(f'No run ID given, using the latest successful release run {run_id}.')
    if not run_id.isascii() or not run_id.isdecimal():
        raise ValueError('Release run ID must be a positive integer.')
    run = github(f'actions/runs/{run_id}')
    if run.get('path', '').split('@')[0] != WORKFLOW:
        raise ValueError('The selected run is not a release run.')
    if run.get('status') != 'completed' or run.get('conclusion') != 'success':
        raise ValueError('The selected release run must have completed successfully.')
    print(f'Validated release run {run_id} ({run.get("head_branch")}), commit {run["head_sha"]}.')
    output = os.environ.get('GITHUB_OUTPUT')
    if output:
        with open(output, 'a', encoding='utf-8') as out:
            out.write(f'run_id={run_id}\nsha={run["head_sha"]}\n')


def version(name):
    match = ARCHIVE.fullmatch(name)
    return tuple(map(int, match.groups())) if match else None


def collect_files(root):
    files = {}
    for path in sorted(Path(root).rglob('*')):
        if path.is_symlink():
            raise ValueError('Artifact symlinks are not supported.')
        if not path.is_file():
            continue
        if version(path.name) is None and path.name not in DOCUMENTS:
            continue
        if path.name in files:
            raise ValueError(f'Duplicate output filename: {path.name}')
        if '\r' in path.name or '\n' in path.name or path.stat().st_size == 0:
            raise ValueError('Invalid or empty artifact file.')
        files[path.name] = path
    releases = {version(name) for name in files if version(name) is not None}
    if not releases:
        raise ValueError('No release archives found in the downloaded artifacts.')
    if len(releases) != 1:
        raise ValueError('The downloaded archives contain multiple release versions.')
    missing = DOCUMENTS - files.keys()
    if missing:
        raise ValueError('Missing document files: ' + ', '.join(sorted(missing)))
    # the Next package (release.yml, the zxnext job) must go up with the others
    missing = [p for p in REQUIRED_PLATFORMS if not any(n.endswith(f'-{p}.zip') or n.endswith(f'-{p}.tar.gz') for n in files)]
    if missing:
        raise ValueError('Missing release archives: ' + ', '.join(missing))
    return files, releases.pop()


def safe_error(error):
    message = str(error)
    for key in ('FTP_PASSWORD', 'GH_TOKEN'):
        secret = os.environ.get(key)
        if secret:
            message = message.replace(secret, '[REDACTED]')
    return ' '.join(message.splitlines())[:600]


def remove_previous_archives(ftp, files, directory, current):
    removed = 0
    for entry in ftp.nlst():
        path = PurePosixPath(entry)
        if len(path.parts) > 1 and path.parent != directory:
            continue
        name = path.name
        previous = version(name)
        if name in files or previous is None or previous >= current:
            continue
        print(f'Removing previous archive: {name}', flush=True)
        ftp.delete(name)
        removed += 1
    print(f'Removed {removed} previous archives.', flush=True)


def upload(root):
    files, current = collect_files(root)
    password = os.environ.get('FTP_PASSWORD', '')
    if not password:
        raise ValueError('Set the FTP_PASSWORD repository Actions secret before publishing.')
    directory = PurePosixPath(os.environ['FTP_DIRECTORY'])
    if not directory.is_absolute() or '..' in directory.parts:
        raise ValueError('FTP_DIRECTORY must be an absolute directory without parent traversal.')
    print(f'Publishing LZ80N {".".join(map(str, current))} to {directory}', flush=True)
    stage = 'opening FTPS connection'
    try:
        # Validate the server certificate and encrypt both login and data connections.
        with ftplib.FTP_TLS(context=ssl.create_default_context(), timeout=120) as ftp:
            stage = 'connecting to server'
            ftp.connect(os.environ['FTP_HOST'], 21)
            stage = 'login (check Forpsi country/IP restrictions and the FTP_PASSWORD secret)'
            ftp.login(os.environ['FTP_USERNAME'], password)
            stage = 'enabling encrypted data transfers'
            ftp.prot_p()
            ftp.set_pasv(True)
            stage = f'opening target directory {directory}'
            try:
                ftp.cwd(str(directory))
            except ftplib.error_perm as error:
                if not str(error).startswith('550'):
                    raise
                ftp.cwd('/')
                for part in directory.parts[1:]:
                    stage = f'opening directory component {part}'
                    try:
                        ftp.cwd(part)
                    except ftplib.error_perm as error:
                        if not str(error).startswith('550'):
                            raise
                        stage = f'creating directory component {part}'
                        ftp.mkd(part)
                        ftp.cwd(part)
            for name, path in files.items():
                stage = f'uploading {name}'
                print(f'Uploading {name} ({path.stat().st_size} bytes)', flush=True)
                with path.open('rb') as source:
                    ftp.storbinary('STOR ' + name, source, blocksize=256 * 1024)
                stage = f'verifying uploaded size of {name}'
                ftp.voidcmd('TYPE I')
                if ftp.size(name) != path.stat().st_size:
                    raise ValueError(f'Remote size verification failed: {name}')
            stage = 'removing previous archives'
            remove_previous_archives(ftp, files, directory, current)
    except (ftplib.Error, OSError, EOFError) as error:
        detail = safe_error(error)
        if stage.startswith('login') and str(error).startswith('530'):
            detail += ' Forpsi can also reject valid credentials through country/IP restrictions. Check its FTP log for DENY and the runner country.'
        raise ValueError(f'FTP {stage}: {detail}') from None
    print(f'Published {len(files)} files. Existing unrelated files were retained.')


if __name__ == '__main__':
    try:
        if sys.argv[1:] == ['--validate-run']:
            validate_run()
        elif len(sys.argv) == 2:
            upload(sys.argv[1])
        else:
            raise ValueError('Usage: publish-ftp.py ARTIFACT_DIRECTORY | --validate-run')
    except Exception as error:
        # FTP errors can contain server responses: never print login details.
        if isinstance(error, ValueError):
            print(f'Publish failed: {error}', file=sys.stderr)
        else:
            print(f'Publish failed ({type(error).__name__}). Check the connection and source build.', file=sys.stderr)
        sys.exit(1)
