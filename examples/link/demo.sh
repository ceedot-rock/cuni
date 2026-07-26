#!/usr/bin/env bash
# Flagship demo: one CuNi `link` → Go server + Python client + JS client.
# Expected client stdout (each):  hello Cee x3
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIR="$(cd "$(dirname "$0")" && pwd)"
OUT="${DIR}/out"
PORT="${CUNI_LINK_PORT:-8947}"
BASE="http://127.0.0.1:${PORT}"

find_cuni() {
  if [[ -n "${CUNI_BIN:-}" && -x "${CUNI_BIN}" ]]; then
    echo "${CUNI_BIN}"
    return
  fi
  for p in "${ROOT}/target/release/cuni" "${ROOT}/target/debug/cuni"; do
    if [[ -x "$p" ]]; then
      echo "$p"
      return
    fi
  done
  if command -v cuni >/dev/null 2>&1; then
    command -v cuni
    return
  fi
  echo "cuni binary not found — run: cargo build --release" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 1
  }
}

need python3
need go
need node

CUNI="$(find_cuni)"
mkdir -p "${OUT}"
rm -rf "${OUT:?}/"*

echo "==> emit examples/link.cuni → py / go / js"
"${CUNI}" "${ROOT}/examples/link.cuni" \
  --emit-py "${OUT}/link.py" \
  --emit-go "${OUT}/link.go" \
  --emit-js "${OUT}/link.js"

# Wire a real Go HTTP server from the empty generated main().
python3 - <<PY
from pathlib import Path
p = Path("${OUT}/link.go")
src = p.read_text()
old = "func main() {\n}\n"
new = """func main() {
	http.HandleFunc("/Greet", Greet_handler)
	http.ListenAndServe("127.0.0.1:${PORT}", nil)
}
"""
if old not in src:
    raise SystemExit("generated Go main() shape changed — update examples/link/demo.sh")
p.write_text(src.replace(old, new, 1))
PY

echo "==> go build server (:${PORT})"
go build -o "${OUT}/link_server" "${OUT}/link.go"

echo "==> start Go server"
"${OUT}/link_server" &
SERVER_PID=$!
cleanup() {
  kill "${SERVER_PID}" 2>/dev/null || true
  wait "${SERVER_PID}" 2>/dev/null || true
}
trap cleanup EXIT

# Wait for listen
for _ in $(seq 1 100); do
  if python3 - <<PY
import socket
s=socket.socket(); s.settimeout(0.2)
try:
  s.connect(("127.0.0.1", int("${PORT}")))
  raise SystemExit(0)
except Exception:
  raise SystemExit(1)
finally:
  s.close()
PY
  then
    break
  fi
  sleep 0.05
done

echo "==> Python client → Go server"
PY_OUT="$(
  python3 - <<PY
import sys
sys.path.insert(0, "${OUT}")
from link import Greet_remote
print(Greet_remote("${BASE}", "Cee", 3))
PY
)"
echo "    ${PY_OUT}"

echo "==> JavaScript client → Go server"
JS_OUT="$(
  CUNI_LINK_OUT="${OUT}" CUNI_LINK_BASE="${BASE}" node -e '
const fs = require("fs");
const path = require("path");
const out = process.env.CUNI_LINK_OUT;
const base = process.env.CUNI_LINK_BASE;
const code = fs.readFileSync(path.join(out, "link.js"), "utf8")
  .replace(/\nfunction main\(\) \{\n\}\n\nmain\(\);\n?$/, "\n");
eval(code);
(async () => {
  const r = await Greet_remote(base, "Cee", 3);
  console.log(r);
})().catch((e) => { console.error(e); process.exit(1); });
'
)"
echo "    ${JS_OUT}"

echo "==> Go client → Go server (same binary symbols, remote call)"
# Client is generated from the same emit, with a client-only main() (no server listen).
cp "${OUT}/link.go" "${OUT}/link_client.go"
python3 - <<PY
from pathlib import Path
p = Path("${OUT}/link_client.go")
src = p.read_text()
marker = "func main() {"
idx = src.rfind(marker)
if idx < 0:
    raise SystemExit("no func main() in generated Go")
# Drop whatever main we injected for the server; write a remote client main.
head = src[:idx]
client_main = '''func main() {
	result, err := Greet_remote("http://127.0.0.1:${PORT}", "Cee", 3)
	if err != nil {
		panic(err)
	}
	fmt.Println(result)
}
'''
p.write_text(head + client_main)
PY
go build -o "${OUT}/link_client" "${OUT}/link_client.go"
GO_OUT="$("${OUT}/link_client")"
echo "    ${GO_OUT}"

expect="hello Cee x3"
fail=0
[[ "${PY_OUT}" == "${expect}" ]] || { echo "FAIL python: got '${PY_OUT}'" >&2; fail=1; }
[[ "${JS_OUT}" == "${expect}" ]] || { echo "FAIL js: got '${JS_OUT}'" >&2; fail=1; }
[[ "${GO_OUT}" == "${expect}" ]] || { echo "FAIL go: got '${GO_OUT}'" >&2; fail=1; }

echo
if [[ "${fail}" -eq 0 ]]; then
  cat <<EOF
╔══════════════════════════════════════════════════════════╗
║  FLAGSHIP LINK DEMO — PASS                               ║
║  One CuNi contract · three clients · one Go server       ║
║  All answered: ${expect}
╚══════════════════════════════════════════════════════════╝
EOF
  exit 0
else
  echo "FLAGSHIP LINK DEMO — FAIL" >&2
  exit 1
fi
