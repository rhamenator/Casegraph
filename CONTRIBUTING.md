# Contributing to Casegraph

Casegraph uses the controlled assurance lifecycle in `assurance/README.md`. Before changing behavior:

1. identify the authorizing `CG-HLR-NNN`, `CG-LLR-NNN`, or `CG-PR-NNN` record;
2. update requirements and derived-requirement classification when intent changes;
3. update design/source/test traceability with the implementation;
4. add requirements-based nominal and robustness verification;
5. update migrations, dependencies, documentation, and problem reports when affected;
6. run the commands in `assurance/VERIFICATION_PLAN.md`;
7. obtain review from someone other than the author before using the change in a release assurance
   baseline.

Do not weaken an assertion, suppress a warning, skip a test, alter an applied migration, or close an
anomaly merely to make a check pass. Record the problem and assess its impact.

AI-assisted contributions are allowed for implementation support, but the human author owns every
requirement, line, test, review response, and assurance claim. Model output is not verification or
independence evidence.
