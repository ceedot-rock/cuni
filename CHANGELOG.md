# Changelog

## [Unreleased] — Studio host + SEO

### Added
- **Hosted CuNi Studio:** https://cuni-studio.fly.dev/ (Fly.io)
- Studio **Notelog** + **Critic Book** (persist on volume)
- Studio SEO: Open Graph, Twitter cards, JSON-LD, `robots.txt`, `sitemap.xml`
- Primary CTA across README / press / outreach → Studio URL

## [0.1.6] — 2026-07-26

### Added
- **Named typ constructors:** `Circle(r: 2.0)` (all-named only; no mix with positional)
- **Call-site type checks:** concrete arg types + generic parameter binding conflicts
- **GitHub Release** for v0.1.5 platform surface
- **`assets/link-demo.gif`** — flagship link demo animation
- **`docs/REGISTRY.md`** + example `packages/greet-contract/`
- **CHANGELOG**, **CONTRIBUTING**, SEO meta on playground

### Platform (from 0.1.5)
- `cuni check`, line:col errors, playground, Exactness CI, link demo

## [0.1.5] — 2026-07-26

### Added
- `cuni check` exactness gate
- AST spans + `file:line:col` type errors
- Local playground (`playground/`)
- Exactness GitHub workflow + badge
- Flagship `examples/link/demo.sh` + `docs/LINK_TUTORIAL.md`

## [0.1.1] — prior
- CI, demos, install docs

## [0.1.0] — prior
- Initial public compiler
