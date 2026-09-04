---
name: Digest
description: Calm, precise control plane for evaluating agent-generated learning artifacts
colors:
  background: "oklch(1 0 0)"
  surface: "oklch(0.975 0.004 130)"
  ink: "oklch(0.2 0.018 130)"
  muted: "oklch(0.47 0.018 130)"
  line: "oklch(0.89 0.008 130)"
  primary: "oklch(0.49 0.13 130)"
  primary-hover: "oklch(0.43 0.13 130)"
  accent: "oklch(0.5 0.16 255)"
  danger: "oklch(0.53 0.19 25)"
typography:
  body:
    fontFamily: "Inter, ui-sans-serif, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
  heading:
    fontFamily: "Inter, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.8rem"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "-0.03em"
rounded:
  sm: "6px"
  md: "10px"
spacing:
  sm: "8px"
  md: "16px"
  lg: "28px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.background}"
    rounded: "{rounded.sm}"
    height: "39px"
  button-primary-hover:
    backgroundColor: "{colors.primary-hover}"
    textColor: "{colors.background}"
    rounded: "{rounded.sm}"
    height: "39px"
---

## Overview

Digest is a restrained, task-focused Linux control plane. It should feel dense but unhurried: the operator immediately sees session setup, ordered activity, and durable artifacts. `article_learning_engine_prd.md` remains the product authority.

## Colors

Pure white keeps long evaluation sessions clear under normal office lighting. Olive is reserved for primary action and successful lifecycle state; blue distinguishes tool activity; red is used only for actionable failures. All implementation colors use OKLCH.

## Typography

Use one familiar sans-serif family throughout. Headings are compact and sentence case. IDs, hashes, and paths use the system monospace stack. Body copy remains at or above WCAG 2.2 AA contrast.

## Elevation

Structure comes from surface changes and one-pixel dividers, not floating cards. Focus rings are the primary elevated interaction state. Avoid broad decorative shadows and glass effects.

## Components

Controls use 6px radii, visible labels, and conventional hover, focus, active, disabled, loading, error, and empty states. The desktop workspace is a three-column inspection surface; it collapses into stacked bordered regions for narrow Chromium/WebView viewports. Motion communicates state within 180ms and is removed under reduced-motion preferences.

## Do's and Don'ts

- Do keep the active run, next action, ordered events, and artifact provenance visible.
- Do translate ACP details into canonical workflow language while retaining inspectable diagnostics.
- Do use status text alongside color and preserve complete keyboard operation.
- Don't use consumer-learning gamification, oversized metrics, decorative AI imagery, or protocol jargon as primary labels.
- Don't add nested cards, excessive rounding, gradients, or decorative motion.
