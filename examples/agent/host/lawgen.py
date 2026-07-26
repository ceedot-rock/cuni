"""Generate ephemeral CuNi entrypoints with host-injected args.

Keeps modules pure; only the entry file is generated so exactness still
applies to real numeric/string law, not host-only glue.
"""

from __future__ import annotations


def gen_budget(*, usd: int = 3, cap: int = 5) -> str:
    return f"""use budget

# generated entry — args from host speech/CLI
say(allow_spend({int(usd)}))
say(clamp_spend({int(usd)}, {int(cap)}))
"""


def gen_text(*, name: str = "CuNi", a: str = "law", b: str = "speech") -> str:
    # escape for CuNi string literals in backticks — keep simple alnum
    name, a, b = _s(name), _s(a), _s(b)
    return f"""use text

say(tag_line("{name}"))
say(join_two("{a}", "{b}"))
"""


def gen_score(*, a: int = 7, b: int = 11, n: int = 150) -> str:
    return f"""use score

say(prefer({int(a)}, {int(b)}))
say(score_ok({int(n)}))
"""


def gen_tool_echo(*, msg: str = "ping") -> str:
    msg = _s(msg)
    return f"""def tool_echo(m: str) -> str do
    ret `tool:echo:${{m}}`
end

def tool_ok(code: int) -> int do
    if code == 0 do
        ret 1
    end
    ret 0
end

say(tool_echo("{msg}"))
say(tool_ok(0))
"""


def gen_tool_plan_get(*, path: str = "/health") -> str:
    path = _s(path)
    return f"""# Portable plan for a host-side GET (law only formats; host may fetch)
def plan_get(p: str) -> str do
    ret `GET ${{p}}`
end

say(plan_get("{path}"))
"""


def gen_mind() -> str:
    # static mind — read from disk preferred; this is fallback
    return """use budget
use text
use score

let spend = allow_spend(3)
let blocked = allow_spend(9)
let capped = clamp_spend(12, 5)
let line = tag_line("CuNi")
let bag = join_two("law", "speech")
let best = prefer(7, 11)
let ok = score_ok(150)

say(spend)
say(blocked)
say(capped)
say(line)
say(bag)
say(best)
say(ok)
"""


def _s(v: str) -> str:
    """Sanitize for CuNi double-quoted strings (no escapes in language)."""
    out = []
    for ch in str(v):
        if ch.isalnum() or ch in " _-./:@":
            out.append(ch)
        else:
            out.append("_")
    s = "".join(out).strip() or "x"
    return s[:80]


GENERATORS = {
    "budget": gen_budget,
    "text": gen_text,
    "score": gen_score,
    "tool_echo": gen_tool_echo,
    "tool_plan_get": gen_tool_plan_get,
    "mind": lambda **_: gen_mind(),
}
