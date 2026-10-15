"""Deterministic protocol fixture; it never reads credentials or uses the network."""
import datetime
import json
import os
import subprocess
import sys
import time
from pathlib import Path

request = json.load(sys.stdin)
root = Path(os.environ['HOME']) / '.statusline'
with (root / 'provider-calls').open('a') as stream:
    stream.write(json.dumps({'request': request, 'args': sys.argv[1:]}) + '\n')
mode = os.environ.get('TEST_PROVIDER_MODE', '')
if mode == 'fail':
    sys.exit(1)
if mode == 'malformed':
    print('not JSON')
    sys.exit(0)
if mode == 'large':
    print(' ' * (1024 * 1024 + 1))
    sys.exit(0)
if mode == 'hang':
    (root / 'process-group').write_text(json.dumps({'provider_pid': os.getpid(), 'provider_group': os.getpgrp()}))
    subprocess.Popen(['/bin/sh', '-c', 'ps -o pid=,pgid= -p $$ >> "$2"; sleep 3; echo leaked > "$1"', 'sh', str(root / 'leaked'), str(root / 'process-group')])
    time.sleep(10)
if mode == 'slow':
    time.sleep(0.3)
now = datetime.datetime.now(datetime.timezone.utc)
print(json.dumps({'version': 1, 'key': request['key'], 'observed_at': now.isoformat(),
    'account': {'id': request['key'], 'label': request['key'], 'color': '#64c8dc'},
    'limits': [{'id': 'custom', 'label': 'Burst', 'used_percent': 42, 'resets_at': (now + datetime.timedelta(hours=1)).isoformat()}]}))
