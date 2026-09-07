"""Delete stale lift scratch directories, keeping the newest few.

Each run leaves a ~4 GB scratch dir and 31 had accumulated, taking free space
below the 40 GB guard in the run scripts - which correctly aborted run 71 rather
than starting a lift that might die mid-link.

Keeps KEEP newest by mtime. The newest is the last completed run's, which is
still worth having for a follow-up measurement; everything older has already
been mined and its numbers recorded.

Reports reclaimed space by measuring only what it removes, and prints before
deleting so a wrong list is visible rather than silent.
"""
import os
import shutil
import sys
import time

BASE = os.environ.get('TEMP', r'C:\Users\felix\AppData\Local\Temp')
KEEP = int(sys.argv[1]) if len(sys.argv) > 1 else 2

dirs = []
for name in os.listdir(BASE):
    if not name.startswith('azul-web-transpiler-'):
        continue
    p = os.path.join(BASE, name)
    if os.path.isdir(p):
        try:
            dirs.append((os.path.getmtime(p), p))
        except OSError:
            pass

dirs.sort(reverse=True)
keep, drop = dirs[:KEEP], dirs[KEEP:]

print('found %d scratch dirs; keeping %d newest' % (len(dirs), len(keep)))
for mt, p in keep:
    print('  KEEP  %s  (%s)' % (os.path.basename(p),
                                time.strftime('%m-%d %H:%M', time.localtime(mt))))
print('')

freed = 0
for mt, p in drop:
    n = 0
    for root, _d, files in os.walk(p):
        for f in files:
            try:
                n += os.path.getsize(os.path.join(root, f))
            except OSError:
                pass
    try:
        shutil.rmtree(p, ignore_errors=True)
        freed += n
        print('  removed %-34s %6.1f MB' % (os.path.basename(p), n / 1048576.0))
    except Exception as e:
        print('  FAILED  %s: %s' % (os.path.basename(p), e))

print('')
print('reclaimed %.1f GB' % (freed / 1073741824.0))
