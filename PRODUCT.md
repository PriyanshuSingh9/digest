# Product

`article_learning_engine_prd.md` is the authoritative product specification. This file records only the design context used to implement its interfaces.

## Register

product

## Users

The initial user is a technically fluent operator evaluating how well OpenCode and `agy` can turn source material into inspectable learning artifacts. They work in a Linux desktop client and need to start runs, understand agent activity, inspect outputs, and diagnose failures without reading raw ACP traffic.

## Product Purpose

Digest is an article-to-learning compilation system. Its V1 interface is a quality-evaluation control plane: it makes agent runs, canonical events, and durable artifacts legible. Success means the operator can quickly judge output quality and trust what happened during a run. V2 moves the actor to Hermes on a remote server while preserving these contracts.

## Brand Personality

Calm, precise, technical. The interaction density and restraint should feel at home beside Linear and Raycast without copying either product.

## Anti-references

Avoid colorful consumer-learning dashboards, decorative AI imagery, glass-heavy control panels, oversized marketing metrics, and interfaces that expose protocol jargon instead of workflow state.

## Design Principles

1. Put the active run and its next useful action first.
2. Translate protocol activity into concise, trustworthy workflow events.
3. Keep artifacts durable, inspectable, and clearly associated with their run.
4. Prefer familiar controls and information density over decorative novelty.
5. Make failures actionable without hiding diagnostic detail.

## Accessibility & Inclusion

Target WCAG 2.2 AA. All workflows must be keyboard operable, retain visible focus, meet text and control contrast requirements, avoid color-only status communication, and respect reduced-motion preferences. The interface must remain usable at narrow Chromium/WebView viewport widths.
