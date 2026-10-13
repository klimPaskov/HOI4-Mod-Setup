#!/usr/bin/env bash
# A synthetic, isolated Secret Service round trip; never use the desktop vault.
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)

if [[ "${1:-}" == "--session" ]]; then
  [[ "${HOI4_LINUX_VAULT_TEST_SESSION:-}" == "isolated" ]]
  unset GNOME_KEYRING_CONTROL GNOME_KEYRING_PID SSH_AUTH_SOCK
  keyring_control="$XDG_RUNTIME_DIR/keyring"
  mkdir -p -- "$keyring_control"
  chmod 700 -- "$keyring_control"
  printf '%s' 'synthetic-keyring-unlock-password' |
    /usr/bin/gnome-keyring-daemon --foreground --unlock --components=secrets \
      --control-directory "$keyring_control" >/dev/null &
  keyring_pid=$!
  trap 'kill "$keyring_pid" 2>/dev/null || true' EXIT
  sleep 0.2
  GNOME_KEYRING_CONTROL="$keyring_control" /usr/bin/gnome-keyring-daemon \
    --start --components=secrets --control-directory "$keyring_control" >/dev/null
  cd -- "$repo_root"
  native_test=credentials::tests::linux_secret_service_round_trip_keeps_only_an_opaque_reference
  # Fail closed when the ignored native test is renamed or no longer compiled.
  # A zero-test Cargo success does not establish a vault round trip.
  PATH="$CARGO_HOME/bin:/usr/bin:/bin" cargo test --locked -p hoi4-mod-setup \
    --lib --all-features "$native_test" -- --ignored --exact --list |
    /usr/bin/grep -Fx -- "$native_test: test" >/dev/null
  PATH="$CARGO_HOME/bin:/usr/bin:/bin" cargo test --locked -p hoi4-mod-setup \
    --lib --all-features "$native_test" -- --ignored --exact --test-threads=1
  exit
fi

[[ "$(uname -s)" == "Linux" ]]
for command in /usr/bin/dbus-run-session /usr/bin/gnome-keyring-daemon /usr/bin/secret-tool; do
  [[ -x "$command" ]] || { printf 'Missing Linux vault test prerequisite: %s\n' "$command" >&2; exit 1; }
done
vault_profile=$(mktemp -d /tmp/hoi4-vault-smoke.XXXXXXXX)
case "$vault_profile" in /tmp/hoi4-vault-smoke.*) ;; *) exit 1 ;; esac
trap 'rm -rf -- "$vault_profile"' EXIT
mkdir -p -- "$vault_profile/home" "$vault_profile/config" "$vault_profile/data" \
  "$vault_profile/cache" "$vault_profile/runtime"
chmod 700 -- "$vault_profile" "$vault_profile/runtime"
/usr/bin/dbus-run-session -- env \
  HOME="$vault_profile/home" XDG_CONFIG_HOME="$vault_profile/config" \
  XDG_DATA_HOME="$vault_profile/data" XDG_CACHE_HOME="$vault_profile/cache" \
  XDG_RUNTIME_DIR="$vault_profile/runtime" \
  CARGO_HOME="${CARGO_HOME:-${HOME:?}/.cargo}" \
  RUSTUP_HOME="${RUSTUP_HOME:-${HOME:?}/.rustup}" \
  HOI4_LINUX_VAULT_TEST_SESSION=isolated \
  /usr/bin/bash "$repo_root/scripts/test_linux_credentials.sh" --session
