# Contributing

Keep changes focused, easy to review, and aligned with the bridge's core behavior.
Preserve focus-bound capture, ordered key transitions, and release-on-failure
semantics. Avoid unrelated infrastructure changes.

For protocol changes, update `docs/PROTOCOL.md` and both C/Rust implementations and
fixtures together. Run the relevant app tests and both firmware builds. Changes to
capture, USB, or radio behavior should include the corresponding hardware checks.

Never contribute pairing keys, real device identifiers, captured typing, logs
containing personal data, or provisioned firmware binaries. Use synthetic fixtures.
Contributions are under the project license in LICENSE; preserve third-party
attribution when adapting code from upstream examples.
