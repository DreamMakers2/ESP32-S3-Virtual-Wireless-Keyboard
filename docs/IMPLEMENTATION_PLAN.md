# Implementation plan

Keep the technical concept's native Linux app → USB CDC A → encrypted ESP-NOW B
→ boot-compatible USB HID keyboard design, with hobby-project scope.

1. **Implement and check locally.** Define the shared wire format and session
   handshake; build both ESP-IDF targets and the native Rust app. Verify framing,
   ordered transitions, disconnect release, capture focus and firmware identity
   guards. Review the implementation before hardware deployment.
2. **Flash B first.** Keep both boards on their labelled COM/UART connectors
   (left USB-C on these boards). Verify B's chip identity and flashed bytes, check
   startup over UART, and enable its repeating two-pulse magenta RGB identifier.
   Confirm the visible indicator; if GPIO48 is wrong, test GPIO38 on B while
   retaining the existing private pair configuration.
3. **Hand off B.** Ask the user to move the identified B board to the remote PC,
   connecting its labelled USB/OTG connector (right USB-C). Wait for confirmation.
4. **Flash A.** Verify its identity, written bytes and UART startup. Enable the
   one-pulse magenta identifier, then ask the user to swap A's cable to its right
   USB/OTG connector while keeping it connected to the source computer.
5. **Ready gate.** Confirm native CDC access and peer readiness. Ask the user to
   signal ready before any physical keyboard capture. Begin with a blank editor
   on the remote PC and keep the mouse available for focus changes.
6. **Validate the complete chain.** Check typing, modifiers, shortcuts, preheld
   keys, focus loss, unplug/reconnect, USB suspend recovery, lock LEDs and debug
   timing. Check the actual target BIOS/UEFI after desktop acceptance. Record
   observed results and distinguish them from untested compatibility claims.
7. **Finish.** Address justified findings, run the affected regression checks,
   update relevant documentation, remove test clutter and commit working changes.
   Ask once before pushing to the public repository.

Observed compatibility and documented validation limits are recorded in
[requirements](REQUIREMENTS.md).
