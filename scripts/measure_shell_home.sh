#!/usr/bin/env bash
# A-0 (iteration 0.7.0, SB-183): what plain `claude` and `codex` send from a shell home that holds no
# sign-in, with a local stub as the provider. Every home and folder is a fresh temporary directory;
# the real HOME, Keychain items and sign-ins are never read or written (`env -i`, own config dirs).
# Prints one line per case: which headers arrived (names, and whether the values are the expected
# placeholder and capability; never a value), and what the client printed in its first line.
set -euo pipefail
work=$(mktemp -d "${TMPDIR:-/tmp}/sb-a0.XXXXXX")
trap 'kill "$stub" 2>/dev/null || true; wait "$stub" 2>/dev/null || true; rm -rf "$work"' EXIT
port_file="$work/port"; log="$work/requests.jsonl"
python3 -I - "$port_file" "$log" <<'PY' &
import http.server, json, sys
port_file, log = sys.argv[1], sys.argv[2]
class H(http.server.BaseHTTPRequestHandler):
    def _seen(self):
        n = int(self.headers.get("content-length") or 0)
        if n: self.rfile.read(n)
        h = {k.lower(): v for k, v in self.headers.items()}
        rec = {"path": self.path.split("?")[0],
               "authorization": "placeholder" if h.get("authorization") == "Bearer switchboard-shell"
                   else ("other" if "authorization" in h else "absent"),
               "x-api-key": "present" if "x-api-key" in h else "absent",
               "capability": "expected" if h.get("x-switchboard-capability") == "test-capability"
                   else ("other" if "x-switchboard-capability" in h else "absent")}
        with open(log, "a") as f: f.write(json.dumps(rec) + "\n")
        body = json.dumps({"type": "error", "error": {"type": "invalid_request_error",
                           "message": "a0 stub"}}).encode()
        self.send_response(400); self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body))); self.end_headers(); self.wfile.write(body)
    do_POST = do_GET = _seen
    def log_message(self, *a): pass
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(port_file, "w").write(str(s.server_address[1]))
s.serve_forever()
PY
stub=$!
disown "$stub" 2>/dev/null || true
for _ in $(seq 50); do [ -s "$port_file" ] && break; sleep 0.1; done
port=$(cat "$port_file"); base="http://127.0.0.1:$port"
base_env=(PATH="$PATH" TERM=dumb LANG=en_US.UTF-8)

claude_case() { # name, put-placeholder-in-home (yes/no)
  local name=$1 placeholder=$2 home="$work/$1-home" folder="$work/$1-folder"
  mkdir -p "$home" "$folder/.claude"
  if [ "$placeholder" = yes ]; then
    printf '{"env":{"ANTHROPIC_AUTH_TOKEN":"switchboard-shell"}}\n' > "$home/settings.json"
  fi
  printf '{"env":{"ANTHROPIC_BASE_URL":"%s","ANTHROPIC_CUSTOM_HEADERS":"X-Switchboard-Capability: test-capability"}}\n' \
    "$base" > "$folder/.claude/settings.local.json"
  : > "$log"
  local out; out=$(cd "$folder" && env -i "${base_env[@]}" HOME="$work/$1-user" CLAUDE_CONFIG_DIR="$home" \
    perl -e "alarm 90; exec @ARGV" claude -p "hi" </dev/null 2>&1 | head -3 | tr '\n' ' ' | cut -c1-160) || true
  echo "claude/$name: requests=$(wc -l < "$log" | tr -d ' ') first=$(head -1 "$log" 2>/dev/null) output=\"$out\""
}
codex_case() { # name, requires_openai_auth (true/false)
  local name=$1 auth=$2 home="$work/$1-codex" folder="$work/$1-codex-folder"
  mkdir -p "$home" "$folder"
  cat > "$home/config.toml" <<TOML
model_provider = "switchboard-auto"
[model_providers.switchboard-auto]
name = "Fabric Switchboard"
base_url = "$base/codex/_auto/v1"
wire_api = "responses"
requires_openai_auth = $auth
supports_websockets = false
http_headers = { "X-Switchboard-Capability" = "test-capability" }
TOML
  : > "$log"
  local out; out=$(cd "$folder" && env -i "${base_env[@]}" HOME="$work/$1-user" CODEX_HOME="$home" \
    perl -e "alarm 90; exec @ARGV" codex exec --skip-git-repo-check "hi" </dev/null 2>&1 | grep -v '^\s*$' | tail -2 | tr '\n' ' ' | cut -c1-160) || true
  echo "codex/$name: requests=$(wc -l < "$log" | tr -d ' ') first=$(head -1 "$log" 2>/dev/null) output=\"$out\""
}
echo "claude $(claude --version 2>/dev/null | head -1); codex $(codex --version 2>/dev/null | head -1)"
claude_case placeholder yes
claude_case no-placeholder no
codex_case auth-required true
codex_case no-auth false
