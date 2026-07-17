# Testing strategy

1. Red: `python3 scripts/verify_rfc_process.py` fails while `docs/RFC.md` is
   absent.
2. Green: the verifier passes only when the lifecycle, required template
   sections, ADR reference, and contributor-workflow reference are present.
3. The verifier intentionally checks navigation and durable content, not prose
   quality; the document itself received a manual diff review.
