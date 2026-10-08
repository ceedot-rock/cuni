# Slid Phi Labs — Licensing

**Plain-language guide to what the license allows, when you need a commercial grant, and what each grant costs.**

---

## The short version

Every Slid Phi Labs product ships under a dual license.

- **Open use (AGPL-3.0 or GPL-3.0):** Free. Use it, modify it, build with it — as long as your project stays open source under the same terms.
- **Commercial grant:** A paid exception that lets you embed the code in a closed, proprietary product without publishing your source.

The code is identical either way. The license is what changes.

If you are building in public and your code is open source, you are already covered. Stop here.

If you are building a closed product, a SaaS with proprietary internals, a private agent pipeline, a hosted service where you do not publish your application code — keep reading.

---

## What open use means

**AGPL-3.0** (used for: Agent-Rider, Warrant, CuNi, AwLPay, ExactOdds, and hosted service clients)

AGPL-3.0 is a strong copyleft license. It requires that:

1. Any software you distribute that incorporates AGPL-licensed code must itself be released under AGPL-3.0 (or a compatible license).
2. If you run AGPL-licensed code as a network service — meaning users interact with it over a network — you must publish the complete source of that service under AGPL-3.0.

The second point is the critical one for SaaS and hosted-agent developers. Wrapping an AGPL library in a private API endpoint and serving it to users over HTTP triggers the network-service provision. Your entire application source must be published.

**GPL-3.0** (used for: pulsar, shard-zip, shard-tsdb, and other standalone binaries)

GPL-3.0 is the same strong copyleft, minus the network-service clause. If you distribute a binary or library that contains GPL-licensed code, your application must be released under GPL-3.0. If you run GPL-licensed code privately without distributing it, no source-release obligation is triggered.

**What open use clearly allows — no grant needed:**

- Using the tools in your own open-source project (same license)
- Running the hosted APIs (PCC, Rider, CuNi, AwLPay) from any codebase — the API client is what's licensed, not the server
- Building open-source agents, scripts, or tools that call Slid Phi Labs MCP tools
- Academic research, personal projects, and internal tools you never distribute
- Contributing to the Slid Phi Labs repos themselves

---

## When you need a commercial grant

You need a commercial grant if **any of the following are true**:

1. **You distribute a closed product** that contains or links AGPL or GPL source from a Slid Phi Labs repo — a compiled binary, a bundled library, a private npm package shipped to customers — and you do not intend to publish your full application source.
2. **You run an AGPL-licensed component as a network service** (reachable by users over HTTP, WebSocket, or any network protocol) inside a proprietary product whose source you do not publish.
3. **You embed a Slid Phi Labs library** into a commercial product — a plugin, an SDK, a packaged agent runtime, an OEM integration — and ship it to customers under a proprietary license.

**Practical examples:**

| What you're building | Grant required? |
|---|---|
| An open-source agent that calls the Rider API | No — you're calling the hosted service |
| An open-source CLI tool that bundles pulsar | No — GPL-3.0 allows open distribution |
| A closed SaaS that runs TNSSRC internally and exposes it over HTTP | Yes — AGPL network-service clause |
| A proprietary desktop app that statically links pulsar | Yes — GPL-3.0, distribution of a closed binary |
| A private internal tool that uses CuNi, never distributed | No — no distribution, no obligation |
| An OEM agent runtime that ships Agent-Rider embedded in a closed product | Yes — closed distribution of AGPL code |
| A hosted compression service built on PCC's npm client | No — you're calling the hosted PCC API |

If you are unsure, email corey@slidphilabs.com. A short description of what you're building is enough to get a clear answer.

---

## Commercial grants by product

All grants are per-product, per-organization. An exception covers one product at one company. Bundles are noted where they exist.

### Agent-Rider (AGPL-3.0)

Closed-embed grant is included in all paid Rider seats. If you hold a Rider seat, you may ship Agent-Rider in a closed product without a separate grant.

| SKU | Price | Coverage |
|---|---|---|
| `rider-solo` | $13.31/mo | 1 agent seat + closed-embed grant |
| `rider-bundle` | $19.31/mo | Bundle seat + closed-embed grant |
| `rider-crew` | $49.00/mo | Crew seat + closed-embed grant |
| `rider-shop` | $199.00/mo | Shop seat + closed-embed grant |
| `rider-fleet` | $631.00/mo | Fleet seat + closed-embed grant |

For OEM redistribution (shipping Agent-Rider inside a product you sell to other companies), contact corey@slidphilabs.com.

---

### Warrant (AGPL-3.0)

Closed-embed grant is included in all paid Warrant seats.

| SKU | Price |
|---|---|
| `warrant-month` | $29.00/mo |
| `warrant-year` | $290.00/yr |

---

### PCC / TRUSTREAM (AGPL-3.0)

PCC is a hosted service. Using PCC through the API, npm, or MCP does not require a grant — you are calling the hosted service, not distributing its source.

Closed-embed grant is included in all paid PCC seats.

| SKU | Price |
|---|---|
| `gc-month` (PCC Pro) | $39.00/mo |
| `gc-year` (PCC Year) | $390.00/yr |

---

### TNSSRC (AGPL-3.0)

TNSSRC is the local compression engine. Running it on your own machine under open-source terms is free.

A grant is required to embed TNSSRC in a closed application you distribute or run as a network service.

| SKU | Price |
|---|---|
| `tnssrc-grant` | $390.00/yr |

---

### pulsar (GPL-3.0)

pulsar is a standalone compressor binary. GPL-3.0 allows open distribution — if you ship a product that includes pulsar and that product is itself open source, no grant is needed.

A grant is required to embed pulsar in a closed, proprietary binary you distribute to customers.

| SKU | Price |
|---|---|
| `pulsar-exception` | $390.00/yr |

---

### CuNi (AGPL-3.0)

CuNi Studio is free and open. The closed-app exception covers building a product on CuNi's exactness engine that you do not publish as open source.

| SKU | Price |
|---|---|
| `cuni-exception` | $390.00/yr |

---

### Chamber (proprietary)

Chamber is not open-source. It is a paid service from day one. A seat is required for any use.

| SKU | Price |
|---|---|
| `chamber-month` | $9.00/mo |
| `chamber-year` | $99.00/yr |

---

### AwLPay (AGPL-3.0)

AwLPay has a free tier for open use. A grant is included in paid AwLPay seats.

| SKU | Price |
|---|---|
| AwLPay Free | 0.5% per payment, no fixed fee |
| AwLPay Pro | $39.00/mo (0% platform fee under $30k volume or 500 txs/mo) |
| AwLPay L33t | $799.00/mo (unlimited, fair-use compute guard) |

---

### shard-zip / shard-tsdb npm packages (GPL-3.0)

Commercial support licenses for these npm libraries. Support does not include the PCC engine.

| SKU | Price |
|---|---|
| `shard-zip` | $199.00 |
| `shard-tsdb` | $199.00 |

---

### Bundles

| SKU | Price | What it includes |
|---|---|---|
| `lab-pass` | $490.00/yr | Chamber Year + PCC Year |
| `foundry-plus` | Contact for price | Lab Pass + TNSSRC closed grant + Rider Crew |

---

## Five common questions

**1. I'm building an open-source agent that calls Rider, PCC, and CuNi over their APIs. Do I need any grants?**

No. Calling hosted Slid Phi Labs services from any codebase — open or closed — does not require a grant. The grant applies to redistributing or embedding the source libraries in a closed product, not to making API calls. Open source developers using the hosted APIs owe nothing beyond the service usage fees.

**2. I'm building a closed SaaS that compresses user files using TNSSRC on my server. Users never see my code. Do I need a grant?**

Yes. Running AGPL-licensed code as a network service triggers the AGPL's network-service provision, even when users never receive your source. Because your SaaS is a proprietary product and you are not publishing the application source, you need the TNSSRC grant (`tnssrc-grant`, $390/yr). Alternatively, use the hosted PCC API instead — calling a hosted service never requires a grant.

**3. I want to embed pulsar in my commercial desktop app. What do I need?**

The `pulsar-exception` grant ($390/yr). pulsar is GPL-3.0, which permits open-source distribution but not closed distribution without a commercial exception. One grant covers one product at one organization.

**4. My company's agent runtime ships to enterprise customers with Agent-Rider bundled inside. Is one Rider seat enough, or do I need something else?**

A standard Rider seat covers your own agents' usage of Agent-Rider — it does not cover OEM redistribution, where you are shipping Agent-Rider as a component inside a product you sell. Email corey@slidphilabs.com with a description of the use case. OEM terms are handled case by case.

**5. We have an internal tool that uses CuNi for exactness checks. We never distribute it and it's only used inside our company. Do we need a grant?**

No. Using AGPL or GPL software privately — without distributing it and without exposing it as a network service to external users — does not trigger any source-release obligation. Internal tools are free to use under open-source terms with no grant required.

---

## How to buy a grant

**x402 (AI agents):** POST to `https://www.slidphilabs.com/api/x402-products` with the SKU. The endpoint returns HTTP 402, the agent pays in USDC on Solana or Base, and the `claim_token` in the response is the access credential.

**Stripe (humans):** `https://www.slidphilabs.com/pay?sku=<sku>` — for any SKU listed above.

**Questions or OEM terms:** corey@slidphilabs.com

---

## License texts

- [AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.html)
- [GPL-3.0](https://www.gnu.org/licenses/gpl-3.0.html)

Repository-level `LICENSE` files in each GitHub repo at [github.com/ceedot-rock](https://github.com/ceedot-rock) contain the exact license applicable to that project.

---

*Last updated: October 8, 2026. Slid Phi Labs · Cherry Hill, New Jersey · corey@slidphilabs.com*
