#!/usr/bin/env bash
# Drops the per-test databases (`zs_t_*`) created by the `testkit` feature.
set -euo pipefail
docker exec zebra-pg psql -U zebra -d zebra -Atc "SELECT datname FROM pg_database WHERE datname LIKE 'zs\_t\_%'" \
  | while read -r db; do docker exec zebra-pg psql -U zebra -d zebra -qc "DROP DATABASE IF EXISTS $db"; done
docker exec zebra-mysql mysql -uroot -pzebra -Nse "SELECT schema_name FROM information_schema.schemata WHERE schema_name LIKE 'zs\_t\_%'" 2>/dev/null \
  | while read -r db; do docker exec zebra-mysql mysql -uroot -pzebra -e "DROP DATABASE IF EXISTS \`$db\`" 2>/dev/null; done
echo "test databases dropped"
