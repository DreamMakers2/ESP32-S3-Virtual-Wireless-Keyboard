# Public release checklist

This checklist records evidence; unchecked items are not complete.

- [x] Local repository initialized; existing public remote verified.
- [x] Private provisioning, binaries and local tool directories excluded from Git.
- [x] Current tree and retained history reviewed for secrets and personal data.
- [x] Both firmware targets build with locked dependencies.
- [x] App tests and release packaging pass (28 tests).
- [x] Native visual/input, target shortcuts and actual BIOS/UEFI accepted by user.
- [x] Final activation fix accepted by the user.
- [x] Unmeasured timing and unrecorded target/recovery checks documented; user
  explicitly requested no further tests or reviews.
- [x] Independent app/firmware reviews and affected automated regressions completed.
- [x] Setup commands, requirements, local links, license and notices checked against code.
- [x] Only intended source/documentation committed; test clutter removed.
- [x] Public push approved by the repository owner.
- [x] Public source prepared as one `initial public release` commit.

Publication is performed from this commit to the existing `main` remote branch;
local pairing data, operational notes and agent instructions are excluded.

Do not change repository visibility or rewrite existing history as part of ordinary
development. The owner requested the single-commit release model for this initial publication.
