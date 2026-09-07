# Contributing

Keep changes small and appropriate for a hobby project. Read the
technical concept before changing behavior. Preserve focus-bound capture, ordered
key transitions and release-on-failure semantics. Avoid unrelated infrastructure.

For protocol changes, update docs/PROTOCOL.md and both C/Rust implementations and
fixtures together. Run the relevant app tests and both firmware builds. Changes
to capture, USB or radio behavior also require hardware acceptance; describe any
tests that could not be run instead of presenting build success as device proof.

Never contribute pairing keys, real device identifiers, captured typing, logs
containing personal data, or provisioned firmware binaries. Use synthetic fixtures.
Contributions are under the project license in LICENSE; preserve third-party
attribution when adapting code from upstream examples.
