#!/bin/bash
# Verify PM2 isolation for microstable-keeper

echo "=== PM2 Isolation Check ==="
echo "PM2_HOME=${PM2_HOME:-NOT SET (using default ~/.pm2)}"
PM2_HOME=${PM2_HOME:-/home/spritz/.pm2-keeper} pm2 jlist 2>/dev/null | python3 -c "
import json,sys
procs=json.load(sys.stdin)
names=[p['name'] for p in procs]
print(f'Processes in PM2 domain: {names}')
if len(procs)==1 and procs[0]['name']=='microstable-keeper':
    print('✅ ISOLATED')
else:
    print('⚠️  NOT ISOLATED — other processes detected')
"

echo "=== .env permissions ==="
ls -la /home/spritz/microstable-keeper/.env 2>/dev/null || echo "⚠️  .env not found"
