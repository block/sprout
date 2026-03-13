#!/usr/bin/env bash

set -euo pipefail

DB_HOST="${SPROUT_DB_HOST:-127.0.0.1}"
DB_PORT="${SPROUT_DB_PORT:-3306}"
DB_USER="${SPROUT_DB_USER:-sprout}"
DB_PASS="${SPROUT_DB_PASS:-sprout_dev}"
DB_NAME="${SPROUT_DB_NAME:-sprout}"
DOCKER_DB_HOST="${SPROUT_DOCKER_DB_HOST:-mysql}"
DOCKER_NETWORK="${SPROUT_DOCKER_NETWORK:-sprout-net}"
MYSQL_CLIENT_IMAGE="${SPROUT_DB_CLIENT_IMAGE:-mysql:8.0}"

SYSTEM_PUBKEY="0000000000000000000000000000000000000000000000000000000000000000"
ALICE_PUBKEY="953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f"
BOB_PUBKEY="bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260"
CHARLIE_PUBKEY="554cef57437abac34522ac2c9f0490d685b72c80478cf9f7ed6f9570ee8624ea"
TYLER_PUBKEY="e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34"
AGENT_PUBKEY="db0b028cd36f4d3e36c8300cce87252c1f7fc9495ffecc53f393fcac341ffd36"

if command -v mysql >/dev/null 2>&1; then
  run_mysql() { MYSQL_PWD="$DB_PASS" mysql -h"$DB_HOST" -P"$DB_PORT" -u"$DB_USER" "$DB_NAME" "$@"; }
elif docker exec sprout-mysql mysql --version >/dev/null 2>&1; then
  run_mysql() {
    docker run --rm -i --network "$DOCKER_NETWORK" \
      -e MYSQL_PWD="$DB_PASS" \
      "$MYSQL_CLIENT_IMAGE" \
      mysql -h"$DOCKER_DB_HOST" -u"$DB_USER" "$DB_NAME" "$@"
  }
else
  echo "No mysql client available. Start docker compose or install mysql." >&2
  exit 1
fi

run_sql() {
  run_mysql --silent --skip-column-names -e "$1"
}

uuid5_hex() {
  local slug="$1"
  python3 - "$slug" <<'PYEOF'
import sys, uuid
print(uuid.uuid5(uuid.NAMESPACE_DNS, sys.argv[1]).hex)
PYEOF
}

echo "Checking database connection..."
run_sql "SELECT 1" >/dev/null

UUID_GENERAL=$(uuid5_hex "sprout.channel.general")
UUID_RANDOM=$(uuid5_hex "sprout.channel.random")
UUID_ENGINEERING=$(uuid5_hex "sprout.channel.engineering")
UUID_AGENTS=$(uuid5_hex "sprout.channel.agents")
UUID_WATERCOOLER=$(uuid5_hex "sprout.channel.watercooler")
UUID_ANNOUNCEMENTS=$(uuid5_hex "sprout.channel.announcements")
UUID_DM_ALICE_TYLER=$(uuid5_hex "sprout.channel.dm.alice-tyler")
UUID_DM_BOB_TYLER=$(uuid5_hex "sprout.channel.dm.bob-tyler")
UUID_DM_BOB_CHARLIE_TYLER=$(uuid5_hex "sprout.channel.dm.bob-charlie-tyler")

SYSTEM_HEX="UNHEX('${SYSTEM_PUBKEY}')"

run_sql "
INSERT IGNORE INTO channels
  (id, name, channel_type, visibility, description, created_by, topic_required)
VALUES
  (UNHEX('${UUID_GENERAL}'), 'general', 'stream', 'open', 'General discussion for everyone', ${SYSTEM_HEX}, 0),
  (UNHEX('${UUID_RANDOM}'), 'random', 'stream', 'open', 'Off-topic, fun stuff', ${SYSTEM_HEX}, 0),
  (UNHEX('${UUID_ENGINEERING}'), 'engineering', 'stream', 'open', 'Engineering discussions', ${SYSTEM_HEX}, 0),
  (UNHEX('${UUID_AGENTS}'), 'agents', 'stream', 'open', 'AI agent testing and collaboration', ${SYSTEM_HEX}, 0),
  (UNHEX('${UUID_WATERCOOLER}'), 'watercooler', 'forum', 'open', 'Casual forum for async discussions', ${SYSTEM_HEX}, 1),
  (UNHEX('${UUID_ANNOUNCEMENTS}'), 'announcements', 'forum', 'open', 'Company announcements', ${SYSTEM_HEX}, 1),
  (UNHEX('${UUID_DM_ALICE_TYLER}'), 'alice-tyler', 'dm', 'private', 'DM between alice and tyler', ${SYSTEM_HEX}, 0),
  (UNHEX('${UUID_DM_BOB_TYLER}'), 'bob-tyler', 'dm', 'private', 'DM between bob and tyler', ${SYSTEM_HEX}, 0),
  (UNHEX('${UUID_DM_BOB_CHARLIE_TYLER}'), 'bob-charlie-tyler', 'dm', 'private', 'Group DM: bob, charlie, tyler', ${SYSTEM_HEX}, 0)
;
"

run_sql "
INSERT IGNORE INTO channel_members
  (channel_id, pubkey, role, invited_by)
VALUES
  (UNHEX('${UUID_GENERAL}'), UNHEX('${TYLER_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_GENERAL}'), UNHEX('${ALICE_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_GENERAL}'), UNHEX('${BOB_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_RANDOM}'), UNHEX('${TYLER_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_ENGINEERING}'), UNHEX('${TYLER_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_AGENTS}'), UNHEX('${TYLER_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_WATERCOOLER}'), UNHEX('${TYLER_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_ANNOUNCEMENTS}'), UNHEX('${TYLER_PUBKEY}'), 'guest', ${SYSTEM_HEX}),
  (UNHEX('${UUID_DM_ALICE_TYLER}'), UNHEX('${ALICE_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_DM_ALICE_TYLER}'), UNHEX('${TYLER_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_DM_BOB_TYLER}'), UNHEX('${BOB_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_DM_BOB_TYLER}'), UNHEX('${TYLER_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_DM_BOB_CHARLIE_TYLER}'), UNHEX('${BOB_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_DM_BOB_CHARLIE_TYLER}'), UNHEX('${CHARLIE_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_DM_BOB_CHARLIE_TYLER}'), UNHEX('${TYLER_PUBKEY}'), 'member', ${SYSTEM_HEX}),
  (UNHEX('${UUID_GENERAL}'), UNHEX('${AGENT_PUBKEY}'), 'bot', ${SYSTEM_HEX}),
  (UNHEX('${UUID_RANDOM}'), UNHEX('${AGENT_PUBKEY}'), 'bot', ${SYSTEM_HEX}),
  (UNHEX('${UUID_ENGINEERING}'), UNHEX('${AGENT_PUBKEY}'), 'bot', ${SYSTEM_HEX}),
  (UNHEX('${UUID_AGENTS}'), UNHEX('${AGENT_PUBKEY}'), 'bot', ${SYSTEM_HEX})
;
"

echo "Desktop e2e data ready."
