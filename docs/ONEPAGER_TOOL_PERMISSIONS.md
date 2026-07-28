# CuNi for agent tool permissions (seed)

**Status:** secondary vertical — use only if the conversation needs it.  
**Primary vertical remains spend / limits.**

## One line
Agent tool allow/deny rules that must match in every runtime — or they don’t ship.

## Shape (same as spend)
- Rule written once in CuNi  
- Exactness across py/go/js  
- Speech can request (“allow tool X under cap Y”)  
- Publish only after PASS  

## Example gate
Allow a tool call only if the tool is permitted **and** any related spend fits a cap (see spend vertical examples).

## When to use this page
Pitch already understood spend caps and asks “what about agent tools?”
