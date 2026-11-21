# HAOMA Screenshot Reconstruction Contract

The screenshot is evidence of a target visual/functional state, not permission to ship a rasterized imitation.

## Output objective

Produce a reconstruction that can be normalized into HAOMA's production frontend.

## Required implementation properties

- Preserve layout geometry, hierarchy, density, spacing, typography intent, borders, elevation, and visible interaction states.
- Prefer reusable component structure over one-off markup.
- Do not embed the source screenshot as UI.
- Do not use generated images for controls, text, tables, forms, charts, navigation, or other deterministic interface elements.
- Preserve real source assets when extraction is available.
- Keep content semantics and accessibility recoverable.

## HAOMA normalization target

The winning reconstruction is an intermediate artifact. Before production acceptance it must be converted to:

- canonical Mantine primitives and HAOMA reusable components;
- semantic surface, text, border, radius, spacing, and elevation tokens;
- Matrix tables/cards/charts/forms/drawers where applicable;
- light and dark themes;
- RTL and LTR behavior;
- responsive layout behavior;
- GraphQL-backed data paths where the target route is migrated;
- no arbitrary Tailwind styling in the final HAOMA implementation unless explicitly retained by architecture.

## Acceptance

A screenshot-to-code variant is not complete merely because it renders.

It must be followed by:

1. visual comparison against the reference;
2. structural/component review;
3. HAOMA normalization;
4. Playwright route capture;
5. functional and accessibility checks;
6. independent parity review.
