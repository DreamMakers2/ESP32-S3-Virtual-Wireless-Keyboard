# Security

This is a fixed-pair hobby keyboard bridge. Radio traffic uses encrypted ESP-NOW
unicast with private PMK/LMK values and explicit peer identity filters. Application
sessions and acknowledgements prevent stale or duplicate input from being applied
as new transitions. These mechanisms do not make the boards tamper-resistant.

Pairing keys are compiled into local firmware. Physical firmware extraction is
outside the current threat model; secure boot and flash encryption are not enabled.
Do not publish provisioned images or private configuration. Source defaults must
never become operational shared credentials.

Raw keyboard access is sensitive. Grant it only to the configured device, use the
GUI as an ordinary user, and keep capture limited to an explicitly activated,
focused window. The mouse remains available to leave capture. Typed history is
session-local and should not be written into logs or diagnostics.

Report reproducible, non-sensitive issues through the repository's GitHub issues.
For sensitive details, use GitHub private vulnerability reporting if the owner has
enabled it; otherwise contact the owner through an available private GitHub route
before disclosing details. No dedicated security response SLA is offered.
