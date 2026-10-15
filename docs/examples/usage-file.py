#!/usr/bin/env python3
"""Read an account snapshot produced by your own collector; no network or dependencies."""
import json
import sys

request = json.load(sys.stdin)
if request.get('version') != 1 or not request.get('key'):
    sys.exit('unsupported request')
with open(sys.argv[1], encoding='utf-8') as source:
    accounts = json.load(source)
snapshot = accounts.get(request['key'])
if not isinstance(snapshot, dict):
    sys.exit('account unavailable')
# Keep the collector's observed_at. Reading a file must never make old data fresh.
json.dump({**snapshot, 'version': 1, 'key': request['key']}, sys.stdout)
sys.stdout.write('\n')
