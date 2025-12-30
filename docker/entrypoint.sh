#!/usr/bin/env sh
set -eu

is_true() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

data_dir="${ONVM_DATA_DIR:-/data}"

if [ "${1:-}" = "" ] || [ "${1:-}" = "run-node" ]; then
  if [ "${1:-}" = "run-node" ]; then
    shift
  fi

  listen="${ONVM_LISTEN:-/ip4/0.0.0.0/tcp/37000}"
  rpc_bind="${ONVM_RPC_BIND:-0.0.0.0:8080}"
  min_peers="${ONVM_MIN_PEERS:-1}"
  blob_sync_mode="${ONVM_BLOB_SYNC_MODE:-full}"
  config="${ONVM_CONFIG:-config.toml}"
  bootnodes="${ONVM_BOOTNODES:-}"
  identity_passphrase="${ONVM_IDENTITY_PASSPHRASE:-}"

  allow_plaintext_identity="${ONVM_ALLOW_PLAINTEXT_IDENTITY:-0}"
  dev="${ONVM_DEV:-0}"
  init_enable_mdns="${ONVM_INIT_ENABLE_MDNS:-0}"

  mkdir -p "$data_dir"

  identity_path="$data_dir/identity"
  if [ ! -f "$identity_path" ]; then
    if [ -z "$identity_passphrase" ]; then
      echo "error: ONVM_IDENTITY_PASSPHRASE is required on first start to run \`onvm init\`" >&2
      exit 2
    fi

    echo "Initializing ONVM data dir in $data_dir..."
    if is_true "$allow_plaintext_identity"; then
      if is_true "$init_enable_mdns"; then
        onvm init \
          --data-dir "$data_dir" \
          --identity-passphrase "$identity_passphrase" \
          --allow-plaintext-identity \
          --enable-mdns
      else
        onvm init \
          --data-dir "$data_dir" \
          --identity-passphrase "$identity_passphrase" \
          --allow-plaintext-identity
      fi
    else
      if is_true "$init_enable_mdns"; then
        onvm init \
          --data-dir "$data_dir" \
          --identity-passphrase "$identity_passphrase" \
          --enable-mdns
      else
        onvm init --data-dir "$data_dir" --identity-passphrase "$identity_passphrase"
      fi
    fi
  fi

  if [ -z "$identity_passphrase" ] && ! is_true "$allow_plaintext_identity"; then
    echo "error: ONVM_IDENTITY_PASSPHRASE is required (no TTY available for prompts)" >&2
    echo "hint: set ONVM_ALLOW_PLAINTEXT_IDENTITY=1 only if your identity is plaintext" >&2
    exit 2
  fi

  set -- run-node \
    --data-dir "$data_dir" \
    --listen "$listen" \
    --rpc "$rpc_bind" \
    --min-peers "$min_peers" \
    --blob-sync-mode "$blob_sync_mode" \
    --config "$config" \
    "$@"

  if [ -n "$bootnodes" ]; then
    set -- "$@" --bootnode "$bootnodes"
  fi
  if [ -n "$identity_passphrase" ]; then
    set -- "$@" --identity-passphrase "$identity_passphrase"
  fi
  if is_true "$allow_plaintext_identity"; then
    set -- "$@" --allow-plaintext-identity
  fi
  if is_true "$dev"; then
    set -- "$@" --dev
  fi

  exec onvm "$@"
fi

exec onvm "$@"
