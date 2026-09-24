# CuNi Studio PASS gate — citizen receipt for Agent-Rider

**Date stamp:** 2026-09-24 (America/New_York)  
**Status:** Implemented in this PR (callable surface + Studio→Rider push on publish). Live Fly deploy follows Cos GREEN + merge.  
**Locks:** Exactness or refuse (no approximate mode) · Fund = Rider settle / XPay (never PCC) · Never print `ar_` / JWT keys · Do not soften refuse.

Related: [`E2E_STUDIO_RIDER.md`](./E2E_STUDIO_RIDER.md) · [`RIDER_CUTOVER.md`](./RIDER_CUTOVER.md) · [`STATUS.md`](./STATUS.md) · Agent-Rider [`CUNI_CITIZEN_GATE.md`](https://github.com/ceedot-rock/Agent-Rider/blob/main/docs/CUNI_CITIZEN_GATE.md)

---

## What this is

CuNi **Translate** (Studio exactness) mints a **citizen receipt**. Rider **Fund** (hop settle via XPay) and **Execute** (contract register / job accept) should refuse when that receipt does not PASS.

PASS fields:

| Field | Rule |
| --- | --- |
| `source_hash` | Non-empty SHA-256 hex of `.cuni` source bytes (`sourceHash` alias OK) |
| `exactness.passed` | Must be `true` |

Studio hosted gate = **py / go / js** (`CUNI_PLAYGROUND_CHECK_ONLY`). Full 119-lang catalog remains CLI/CI.

---

## Surfaces

### 1. Studio → Rider push (cutover — primary)

On **`POST /api/publish`** after exactness PASS:

1. Studio builds publish `meta` + `citizen_receipt`.
2. When `CUNI_RIDER_URL` is set, `playground/rider_client.py` POSTs to Rider:

```http
POST {CUNI_RIDER_URL}/api/v0/contracts
Content-Type: application/json
X-Cuni-Studio: called
```

```json
{
  "meta": { "source": "...", "sourceHash": "<sha256>", "exactness": { "passed": true, "...": "..." }, "...": "..." },
  "citizen_receipt": {
    "source_hash": "<sha256>",
    "exactness": { "passed": true }
  },
  "studio": "called"
}
```

3. Exactness FAIL → **HTTP 400 REFUSE** — **no** remote push.
4. Local stub (`POST /api/rider/register`) still runs as fallback.

Live Studio today (pre-merge deploy may still push meta-only until this PR is on Fly): https://cuni-studio.fly.dev/ · Rider: https://agentrider.fly.dev/

### 2. Rider-callable verify — `POST /api/pass` (second door)

Machine-facing verify-by-source. Rider Execute may call this **before** a sealed ride.

| | |
| --- | --- |
| Paths | `POST /api/pass` · alias `POST /api/citizen/pass` |
| Body | `{ "source": "<cuni source>" }` required |
| Gate | Same as `/api/check` — `cuni check --only py,go,js` |
| Fund | **None** — verify only |

**PASS (HTTP 200):**

```json
{
  "ok": true,
  "verdict": "PASS",
  "citizen_receipt": {
    "source_hash": "<sha256 hex>",
    "exactness": { "passed": true }
  },
  "gate": ["py", "go", "js"],
  "studio": "called"
}
```

**REFUSE (HTTP 400):**

```json
{
  "ok": false,
  "verdict": "REFUSE",
  "citizen_receipt": null,
  "exactness": { "passed": false },
  "error": "...",
  "diagnostics": "... check_log / refuse reason ...",
  "studio": "called"
}
```

`studio: "called"` means this response came from Studio HTTP. Rider’s **local** shape gate still labels its own refuses `studio: "not_called"` (it did not round-trip Studio).

Optional later: `sourceHash` lookup against registered contracts — **not** in this PR (would be local stub vs remote; document honesty if added).

---

## Honesty

| Claim | Truth after this PR lands on Fly |
| --- | --- |
| Studio publish pushes `citizen_receipt` to Rider `/api/v0/contracts` | **Yes** when `CUNI_RIDER_URL` set and exactness PASS |
| `POST /api/pass` exists for Rider pre-execute verify | **Yes** (code in PR; live after deploy) |
| Studio funds hops | **No** — Fund = Rider settle / XPay |
| Soften refuse / approximate mode | **Forbidden** |

Until Cos GREEN + merge + Studio redeploy, treat live Fly as **prior** behavior (meta register may already work; explicit `citizen_receipt` envelope is this PR).

---

## Smoke

Shape unit test (no `cuni` binary required):

```bash
python3 -m unittest playground.test_citizen_pass -v
```

Live curl (after deploy), known-good spend-control:

```bash
curl -sS -X POST https://cuni-studio.fly.dev/api/pass \
  -H 'Content-Type: application/json' \
  -d "{\"source\": $(python3 -c 'import json;print(json.dumps(open("examples/laws/spend-control.cuni").read()))')}" \
  | jq '{ok, verdict, citizen_receipt, studio}'
```

Broken snippet must REFUSE:

```bash
curl -sS -X POST https://cuni-studio.fly.dev/api/pass \
  -H 'Content-Type: application/json' \
  -d '{"source":"say(1+"}' | jq '{ok, verdict, studio}'
```

---

## agent^rider wire-up follow-up

- Rider Execute should prefer Studio `POST /api/pass` (or trust pushed receipt from publish) **before** sealed ride.
- Local Rider gate (`src/lib/cuni-citizen-gate.ts`) remains validate-when-present / optional strict.
- Coord seats: CuNi `44beb26e49c64d67` · agent^rider `6e1031b9adb12231`. Never paste keys into chat.
