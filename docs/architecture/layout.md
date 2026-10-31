# Deterministic layout

`layout::layout` converts the approved IR into an A3 document without a model or
network call. It is intentionally simpler than a general constraint solver:
correctness and reproducibility matter more than decorative freedom.

## Pipeline

1. Convert paper millimetres to PDF points (`72 / 25.4`).
2. Reserve a 58 pt header on each page, then create 3 equal content columns.
3. Walk IR nodes in stable order and choose the shortest column whose measured
   card fits. Measurement is deterministic character-width wrapping, not font
   probing, so the same IR yields the same page geometry.
4. If no column fits, finalize the current page and start a new one. A node that
   cannot fit on a single A3 page produces an explicit diagnostic; it is never
   silently omitted.
5. Group rectangles are derived from their placed nodes. Edge routing uses the
   same page geometry with orthogonal midpoints.
6. Validation checks that every IR node is placed exactly once, inside the page,
   without overlapping cards, clipping lines, or dropping below 8 pt.

## Repair safety

Layout repair may adjust width, wrapping, font scale within the 8 pt floor,
group ordering, routing and pagination for at most three rounds. It may not
rewrite knowledge text, formulas or anchors. Every repair is recorded as a
version-level operation and can be rolled back.
