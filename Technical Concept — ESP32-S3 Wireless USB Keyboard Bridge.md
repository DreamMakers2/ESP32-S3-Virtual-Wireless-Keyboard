# Technical Concept — ESP32-S3 Wireless USB Keyboard Bridge

## System architecture

The system consists of a CachyOS Linux application, a Linux-side ESP32-S3 bridge and a target-side ESP32-S3 HID bridge. The data path is Linux application → USB CDC-ACM → ESP32-S3 bridge A → encrypted ESP-NOW → ESP32-S3 bridge B → USB HID keyboard → target host. The two ESP32-S3 devices form a permanently paired point-to-point radio link and never associate with an access point, obtain an IP address, run DHCP, TCP, TLS or any other network protocol. ESP-NOW is used directly between the two fixed peers, removing Wi-Fi association and IP-stack startup latency.

Bridge A is permanently configured as the USB CDC endpoint for Linux and as the ESP-NOW peer of bridge B. Bridge B is permanently configured as the ESP-NOW peer of bridge A and as the USB HID keyboard connected to the target host. Both firmwares contain the expected peer MAC address, fixed Wi-Fi radio channel, protocol version and cryptographic material. No discovery, pairing, SSID scanning or channel scanning occurs during normal startup. Each device sets the predefined Wi-Fi channel immediately after the Wi-Fi radio becomes operational and registers only the other ESP32-S3 as an ESP-NOW peer.

Both radios remain continuously awake while operational. Wi-Fi power saving is disabled using `WIFI_PS_NONE`; modem sleep, connectionless receive-window sleep and application light-sleep are not used on the communication path. Both devices are USB-powered and prioritize deterministic latency over radio power consumption.

## Security model

All operational radio traffic is encrypted unicast ESP-NOW traffic. A private random sixteen-byte PMK and a separate private random sixteen-byte LMK are generated specifically for the hardware pair. The custom PMK is installed explicitly and each ESP-NOW peer entry contains the shared LMK with encryption enabled. The ESP-IDF default PMK is never intentionally relied upon and operational peers are never created without encryption.

Bridge A accepts application traffic only from bridge B's hard-coded MAC address and bridge B accepts application traffic only from bridge A's hard-coded MAC address. The MAC addresses are identity filters rather than cryptographic credentials; actual confidentiality and peer authentication derive from the encrypted ESP-NOW relationship.

Every application packet additionally contains a protocol version, current session identifier and sequence information. Each boot creates a new random session identifier so packets belonging to an earlier runtime cannot become valid commands after a reset. A newly established application session always begins with an explicit all-keys-released state.

The cryptographic values may initially be compiled directly into each firmware image because the system is a permanently paired prototype and this avoids storage access during startup. If physical firmware extraction becomes part of the threat model, ESP32-S3 Secure Boot and flash encryption can be enabled independently without changing the transport protocol.

## Linux input capture

The Linux application owns a normal resizable desktop window with the normal operating-system title bar and standard close, minimize and maximize controls. Keyboard forwarding exists only while the application's input area has been explicitly activated and the application remains the focused desktop window.

The physical keyboard is read through Linux evdev rather than relying on GUI key events. When capture becomes active, the application obtains an exclusive `EVIOCGRAB` on the selected physical keyboard device. This prevents the Linux desktop, compositor and ordinary applications from receiving captured keyboard events while forwarding is active. Alt-Tab, Alt-F4, Escape, Super-key combinations, function keys and other shortcuts therefore remain part of the captured input stream instead of being interpreted locally. The application itself does not map Alt-F4, Escape or similar combinations to local application actions while capture is active; they are forwarded as keyboard state like any other combination. Window close, minimize and maximize remain accessible through the normal mouse-operated title-bar controls.

When the user clicks another application or otherwise causes this application to lose desktop focus, capture stops immediately. The application first requests an all-keys-released remote state, stops accepting new forwarding input and releases the evdev grab. Once released, the physical keyboard behaves as if the bridge application were not running. The application must never continue capturing globally while unfocused.

Linux security permissions must allow the process to open and exclusively grab the configured `/dev/input/event*` keyboard. The application itself remains fully contained in its project folder; the README documents the one-time CachyOS permission requirement for raw input and USB CDC access rather than running the GUI as root.

## Keyboard state model

The transport is physical-key-state based rather than character based. Linux `KEY_*` events are mapped to USB HID Keyboard/Keypad usages. The application maintains the complete current pressed-key state and modifier bitmap. A physical press modifies that state and generates a transport transition; a physical release modifies it again and generates another transition.

Linux hardware-repeat notifications are not converted into synthetic press/release cycles. A key remains logically pressed in the transmitted HID state until its physical release is observed, allowing the target operating system to implement normal key-repeat timing.

The radio-side state representation is not restricted to the classic USB six-key rollover structure. Internally the protocol carries a sufficiently large pressed-key bitmap plus modifier state so transport semantics remain independent from the final HID report descriptor. Bridge B can initially expose a conventional boot-compatible keyboard while retaining the option to expose an NKRO report later without redesigning the ESP-NOW protocol.

The target-side ESP32-S3 performs no character translation and knows nothing about keyboard layouts. It receives physical HID state and expresses that state through USB. Character interpretation remains the responsibility of the target operating system. Source and target should therefore use matching keyboard layouts when identical visible character output is expected.

## Local text-history model

The Linux application's visible input history is completely separate from the forwarded HID state. The same raw input events feed both systems, but rendered text never determines what is transmitted. libxkbcommon maintains the source-side keymap, modifier state, Caps Lock state, dead keys, Compose state and UTF-8 character interpretation for the visual history.

Printable input behaves like input in a normal text editor. Printable characters are inserted at the logical caret position. Space inserts a space. Enter creates a new line. Backspace removes preceding editable input. Delete removes following editable input. Arrow and navigation keys affect the application's logical caret as appropriate. Shift affects typed characters while physically held. Caps Lock modifies subsequent text generation according to the local XKB state.

Special keys that do not normally create visible text are represented as concise inline tokens such as `[F3]`, `[Left]`, `[Right]`, `[Esc]`, `[Tab]`, `[Home]`, `[End]` or equivalent representations. Shortcut combinations that would normally manipulate an editor or desktop are not executed against the application history. A physical `Ctrl+A`, `Ctrl+X`, `Ctrl+C`, `Ctrl+V`, `Ctrl+Z`, `Alt+F4`, `Alt+Tab` or equivalent chord is forwarded to the target and represented appropriately in the history instead of selecting, modifying, copying, pasting or otherwise operating on local text.

The visible history is therefore editor-like but is not implemented as a normal editable text widget. It is an application-controlled document model rendered into the input surface. Mouse text selection, mouse caret placement, drag selection, copying, pasting, context-menu editing and ordinary rich-text interactions are disabled. The user can alter the displayed input history only through the same physical typing functions that are being forwarded.

Debug content, when present, exists outside the editable input-history model. Backspace, Delete, navigation keys, shortcut combinations and other keyboard input can never modify, select or delete debug output or its divider.

## USB CDC link

Bridge A exposes a USB CDC-ACM interface through the ESP32-S3 USB device peripheral. The Linux application identifies the correct device using fixed USB identity information and should resolve it through a persistent device path rather than depending on an arbitrary `/dev/ttyACM0` number.

CDC is treated as a raw binary transport rather than a serial-text protocol. No JSON, newline protocol or human-readable serialization is used in the critical path. Messages use a compact framing format that allows the stream to resynchronize after partial reads or application restarts. COBS framing with a zero-byte delimiter is suitable. Every decoded message contains a protocol version, message type and payload.

The main Linux-to-bridge message contains an ordered keyboard-state transition. Reverse-direction messages report connection state, cumulative acknowledgements, bridge state, target keyboard LED state and, when debug mode is enabled, requested diagnostic timing or error information.

CDC input/output executes independently from GUI rendering and keyboard capture. A dedicated I/O worker or asynchronous event loop owns the CDC file descriptor. GUI and input-capture code never block waiting for USB completion.

## ESP-NOW transport and reliability

ESP-NOW delivery confirmation at the Wi-Fi MAC level is insufficient by itself because successful MAC delivery does not prove that bridge B processed and applied the keyboard state. The application protocol therefore provides explicit sequence numbers, positive processing acknowledgements, retransmission and duplicate rejection.

Bridge A maintains a monotonically increasing transition sequence within the current random session. Recently transmitted but not cumulatively acknowledged transitions remain in a small fixed-size ring buffer. Only bounded fixed-capacity data structures are used in the realtime path; routine keyboard forwarding performs no heap allocation.

Bridge B tracks the next expected transition sequence. A valid in-order transition is applied exactly once. Duplicate transitions are not applied again but are acknowledged again. A transition arriving ahead of a missing sequence is retained in a very small bounded reorder buffer or causes bridge B to report the highest contiguous applied sequence. Bridge A retransmits the oldest required transition after a short timeout. When the missing transition arrives, bridge B applies subsequent queued transitions in order.

The acknowledgement represents successful application of the new keyboard state to the bridge B HID path. It does not claim that a target operating-system process has consumed the keystroke.

ESP-NOW send and receive callbacks remain minimal because they execute in the Wi-Fi task context. Callbacks perform only basic validation, capture necessary metadata, copy the fixed-size packet into a statically allocated queue and return. Sequence processing, retransmission decisions, CDC traffic and USB HID work run outside the Wi-Fi callback.

## Heartbeat and dead-man behavior

Bridge A sends periodic encrypted heartbeat traffic throughout an active forwarding session. The interval is short enough to detect a broken controller without becoming a meaningful source of radio contention. Initial values around several tens of milliseconds for heartbeat cadence and a few hundred milliseconds for final dead-man release are appropriate starting points and are finalized by measurement.

Bridge B continuously checks active-session liveness. Loss of valid heartbeat traffic, loss of protocol progress, bridge A reset, radio failure, unresolved sequence gaps or explicit Linux-session termination causes bridge B to immediately submit an all-keys-released HID state, clear pending keyboard transitions and invalidate the current forwarding session.

Bridge B cannot resume an invalidated session merely because an old packet arrives. A fresh HELLO/READY exchange with a current session identifier and initial released-state synchronization is required.

Linux performs the same release proactively on focus loss or orderly shutdown. Normal focus loss therefore follows the path local capture stop → remote all-released transition → acknowledgement where possible → evdev ungrab. The dead-man mechanism handles failure cases in which this orderly exchange cannot complete.

## USB HID behavior

Bridge B exposes a conventional USB HID keyboard through the ESP32-S3 USB device peripheral. It enumerates without depending on bridge A or the radio link. Until an authenticated application session becomes ready, the authoritative HID state is all keys released.

Standard USB Keyboard/Keypad usages provide letters, digits, punctuation, modifiers, Enter, Backspace, Delete, arrows, function keys and navigation keys. Consumer-control functions such as media volume can later be added through a separate HID report if required without altering the main keyboard transport.

Bridge B also listens for target HID output reports. Caps Lock, Num Lock and Scroll Lock LED state can be returned over ESP-NOW to bridge A and CDC to the Linux application so local display logic can know the lock state reported by the actual target rather than assuming it.

Any USB reset, ESP-NOW session reset, protocol fault or dead-man timeout returns the bridge to an all-released state.

## Startup and latency optimization

Startup latency is a first-class requirement. USB initialization and ESP-NOW initialization proceed independently and overlap wherever possible.

Bridge A starts its USB CDC device functionality immediately when application execution begins so the CachyOS host can enumerate it while the Wi-Fi radio is being initialized. Bridge B similarly starts USB HID immediately and exposes a valid all-released keyboard to the target while its radio link is being initialized. Neither USB interface waits for ESP-NOW readiness before enumeration.

The Wi-Fi initialization path contains only functionality required for ESP-NOW. The device enters station mode without association, starts the radio, sets the hard-coded channel, disables Wi-Fi power saving, initializes ESP-NOW, installs the private PMK, registers callbacks and adds the single encrypted hard-coded peer containing the destination MAC and LMK. No SSID scan, AP association, DHCP, DNS, socket initialization or TLS state exists.

Radio configuration, peer MAC addresses and cryptographic values are available without filesystem access. Unnecessary NVS-dependent configuration is avoided in the startup-critical path. Bluetooth, provisioning services, filesystems, network stacks not required by ESP-NOW, diagnostic servers and other unrelated components remain disabled.

Production firmware minimizes bootloader and application logging. Boot-time optimization is based on measured timestamps rather than speculative configuration changes. Logging reduction, removal of unused startup work, early USB initialization and parallel radio initialization are applied before considering bootloader options that weaken image validation.

The ESP-NOW PHY rate is selected for low jitter and reliable short-range operation rather than throughput. Keyboard packets are extremely small and provide no reason to maximize nominal bitrate at the expense of robustness.

The normal performance metric is measured from Linux evdev event receipt through CDC, encrypted ESP-NOW, bridge B processing and HID submission. Worst-case latency and jitter matter more than average throughput.

## Operational state

At power-up bridge B can enumerate as a USB keyboard before bridge A is available but remains all released. Bridge A can enumerate as a CDC device before bridge B is available. Both initialize their fixed encrypted radio peer and begin the application-level readiness exchange as soon as their radio is ready.

The Linux application may start and connect to bridge A before the wireless pair is ready. Keyboard capture is not activated until CDC is available and bridge A reports that bridge B has completed the current-session synchronization.

Activation begins with a known empty local physical-key state and explicit all-released remote synchronization. Each subsequent physical transition passes through CDC, bridge A's ordered radio queue, encrypted ESP-NOW, bridge B's sequence processor and USB HID.

Packet loss causes retransmission. Loss of bridge A causes bridge B's dead-man mechanism to release all keys. Loss or reset of bridge B causes bridge A to create a fresh application session when it returns. Linux application termination causes the kernel evdev grab to disappear and causes bridge B to release all HID state when heartbeats stop. No failure path intentionally preserves a pressed remote key across loss of the controlling session.

## ESP32-S3 onboard RGB status indication

Both ESP32-S3 boards use their onboard addressable RGB LED as an immediate local state indicator. LED handling is isolated from the communication-critical tasks and is event driven; status transitions update a small LED-state object and a low-priority LED task handles timing. LED effects never sleep, busy-wait or delay inside USB, ESP-NOW or HID tasks.

During boot the LED uses RGB `(16, 8, 64)`, blue-violet, with a fast repeating pattern of 150 ms illuminated and 250 ms off. Once local initialization is complete and the device is operational but has not established the application-level ESP-NOW session with its peer, the LED uses RGB `(0, 64, 0)`, green, with 300 ms illuminated and 700 ms off. Once the encrypted peer is synchronized and operational, the LED changes to RGB `(0, 24, 64)`, cyan-blue, continuously illuminated.

If the device remains without a working peer connection for one minute, the LED state changes to RGB `(64, 18, 0)`, amber, with 400 ms illuminated and 1600 ms off. The one-minute timer is measured from the point at which the device itself becomes ready to communicate, not from reset assertion. A critical condition not represented by the normal disconnected state changes the LED to RGB `(64, 0, 0)`, red, blinking 120 ms on and 120 ms off.

A successfully accepted keyboard activity event causes a brief RGB `(64, 59, 24)` calibrated-white activity pulse. Activity indication is subordinate to error indication. It may temporarily override boot, searching or connected colors only when doing so cannot hide a critical red error state. The activity pulse is generated from the application-level keyboard path rather than raw radio traffic so heartbeats, acknowledgements and retransmissions do not flash the LED.

When the complete app/USB/radio/HID chain is ready but capture is paused, both boards and the app use steady RGB `(0, 64, 0)` for `Paused - All devices connected - No keypresses are captured or transmitted`. The boards require recent app-origin HELLO traffic and peer/USB readiness; local radio initialization alone is insufficient. This paused indication remains continuously illuminated and is not overridden by activity or the five-minute LED sleep.

Each ESP independently tracks the time of its most recent status transition. If the underlying status has remained unchanged for five minutes, the physical RGB LED is switched completely off while the logical status remains unchanged. Key activity does not restart this five-minute status timer. A subsequent actual status transition immediately wakes the LED and displays the newly applicable status pattern. If a critical error occurs while the LED is asleep, the error is a status transition and therefore immediately reactivates the red indication.

LED code is intentionally nonessential. Failure of the LED peripheral or LED task can be reported diagnostically when debug mode is enabled but cannot prevent USB, ESP-NOW or HID operation.

During provisioning, an explicit COM/UART maintenance command can temporarily identify a board with magenta pulses: one pulse for bridge A and two pulses for bridge B, repeating until stopped or reset. Critical error indication retains priority. This maintenance mode does not change the normal status colors. The RGB GPIO is configured for the actual board and visibly verified before a cable handoff; it must not be inferred from left/right connector orientation.

## Linux application implementation

The Linux program is designed as a compact native application rather than a browser-based interface. A compiled Rust implementation is preferred for the production application because it provides deterministic low-overhead evdev handling, native asynchronous USB I/O, straightforward fixed-capacity data structures and a portable release binary without requiring a Python interpreter or Electron runtime. The GUI layer uses a lightweight native-window-capable Rust UI implementation with standard OS window decorations rather than a custom title bar.

The application release remains completely contained below the project-level `app` directory. The only executable object directly visible in that directory is the launcher script. The launcher resolves its own directory, validates the packaged runtime, starts the bundled application binary and returns the application's exit status. It does not install files elsewhere during ordinary launch.

A corresponding layout is:

```text
app/
├── run.sh
├── README.md
├── bin/
│   └── keyboard-bridge
├── src/
│   └── application source
├── config/
│   ├── defaults
│   └── linux-permissions documentation
├── resources/
│   └── application resources
└── build/
    └── application build metadata and packaging support
```

Runtime user preferences are not written into the project directory because a portable read-only copy of the application must remain usable. Persistent light/dark and debug settings are stored in the appropriate per-user XDG configuration location, under a single application-specific directory. The packaged default configuration remains in `app/config`. No application source, binary, generated cache or configuration file is placed loosely beside `run.sh` and `README.md`.

The Linux application does not require network access. It opens only its GUI, the configured evdev keyboard and the USB CDC bridge during normal operation.

## Linux application visual design

The application window is intentionally minimal. It contains no application title inside the client area, no toolbar, no menu bar, no status bar, no permanent labels and no auxiliary panels. The only conventional window chrome is the operating system's normal title bar with its standard close, minimize and maximize controls.

The client area contains one large rectangular input surface with comfortably rounded corners and consistent margins from the window edges. The surface expands and contracts with the window while preserving reasonable minimum padding. Its text layout reflows automatically with width changes. Long lines soft-wrap onto subsequent visual lines exactly as expected from a text-editor display, while actual Enter presses create explicit paragraph/newline breaks. Resizing never alters the underlying history.

A comfortable system sans-serif font is used by default so the application matches the CachyOS desktop rather than introducing a visibly foreign bundled typeface. The input history uses a normal readable weight and size selected relative to desktop scaling. The paused overlay uses a slightly larger semibold primary message and a smaller regular-weight subtitle. Debug information uses a smaller but fully readable size, restrained contrast and stylistic distinction from forwarded input. Font metrics, line spacing and box padding scale correctly with HiDPI display scaling.

The input surface itself is the activation target. Clicking inside it activates capture if the connection state permits forwarding. Merely bringing the window to the foreground through another mechanism does not silently begin raw keyboard capture unless the input surface has been activated for that focus session.

## Linux application focus and paused state

The active capture state requires both explicit activation of the input surface and foreground focus of the application window. Once active, the application obtains its exclusive evdev grab and captured keyboard input cannot escape to the local desktop. Alt-Tab, Alt-F4, Escape, Super-key combinations and equivalent keyboard actions remain captured and are represented locally and forwarded remotely.

A mouse click outside the application causes ordinary desktop focus transfer. The application's focus-loss notification immediately disables acceptance of new keyboard events, sends an all-keys-released state to the remote HID bridge without disconnecting or suspending its USB keyboard interface, releases the evdev grab and changes the input surface to paused presentation. Because mouse input itself is never globally captured, the user can always click another application or the OS window controls to exit active keyboard capture.

While paused, the large rounded input surface is visually blurred or equivalently softened beneath an overlay. Centered in the available content area is the semibold message `paused - click to activate`. Directly beneath it appears the smaller subtitle `no keypresses are captured or transmitted`. The overlay scales naturally with window size and remains centered without modifying or clearing the previously rendered key history. It covers the entire rendered history, including the top, bottom and corners. The history viewport has its top and bottom limits inset by one text line (24 logical pixels) from the earlier padding-extended bounds. Both vertical fades retain their 24-pixel distance; the rounded frame and horizontal text limits remain unchanged. The paused overlay spans the full rounded frame width. The status circle has equal top and right spacing.

The right-click menu shows `Pause` only during active capture and `Activate` only while paused.

Clicking the rounded input surface removes the pause overlay and attempts activation. Capture begins only after connection readiness and successful evdev acquisition have been confirmed. Failure to acquire the keyboard does not produce partial capture; the application remains inactive and exposes the problem through its status/error mechanisms.

## Application status indicator

While ready and paused, the app uses steady RGB `(0, 64, 0)` with hover text `Paused - All devices connected - No keypresses are captured or transmitted`, matching both devices.

A small circular status indicator is positioned inside the large rounded input surface near its upper-right corner with a deliberate offset from both edges. It contains no permanently visible text.

While the application is opening or locating bridge A over USB CDC, the indicator uses RGB `(64, 32, 255)`, blue-violet, with 150 ms illuminated and 250 ms off. Its hover text is `Connecting USB CDC`.

After the USB CDC connection is established but bridge A is still waiting for the ESP-NOW/HID peer to become ready, the indicator uses RGB `(0, 255, 0)`, green, with 300 ms illuminated and 700 ms off. Its hover text is `Connected - Searching`.

When the complete Linux → CDC → bridge A → encrypted ESP-NOW → bridge B → HID chain is ready and capture is active, the indicator uses RGB `(0, 96, 255)`, cyan-blue, continuously illuminated. Its hover text is `Connected`.

If the full connection remains unavailable for one minute after the application itself has become ready to attempt the connection, the indicator changes to RGB `(255, 72, 0)`, amber, with 400 ms illuminated and 1600 ms off. Its hover text is `No Connection`.

A critical application, USB or bridge error changes the indicator to RGB `(255, 0, 0)`, red, with 120 ms illuminated and 120 ms off. Its hover text is `Error`.

Keyboard activity temporarily takes precedence over the ordinary connected-state display without masking an error. While one or more physical keys are actively held and accepted by the forwarding path, the indicator is RGB `(255, 255, 255)`, white, continuously illuminated and its hover text is `Activity`. When the active key state returns to all released, the indicator returns immediately to the underlying connection-state color and animation. This makes the defined steady-white activity state meaningful without adding arbitrary blinking timing to the desktop UI.

Hovering the mouse pointer over the circle opens a normal compact tooltip containing only the appropriate current status text. The tooltip has conventional desktop styling, no additional explanation and disappears when the pointer leaves.

## Context menu

The rounded input surface exposes one small custom context menu on right-click. It contains `Pause` while active or `Activate` while paused, followed by `Toggle light/dark mode`, `Toggle debug on/off`, `Cancel` and `Exit`.

`Toggle light/dark mode` changes between the two application themes immediately and persists the selected mode across launches. The application does not expose an automatic third theme mode unless later explicitly required.

`Toggle debug on/off` enables or disables the diagnostic presentation and instrumentation described below and persists the setting across launches. Debug is disabled by default for a new installation.

`Cancel` closes the context menu without changing state.

`Exit` invokes the application's orderly shutdown path, causing remote all-keys-released synchronization where possible, release of the evdev grab, closure of USB CDC and application termination.

No other menus exist. Right-click never exposes normal text editing, copy, paste, selection, spelling or GUI-toolkit default context actions.

## Light and dark themes

Light and dark modes alter only presentation. Input behavior, key mapping, timing, transport and capture semantics are identical in both modes.

The color system uses restrained neutral surfaces, clear text contrast, subtle border separation and the fixed status-indicator RGB values defined above. Status colors are not theme-adjusted because they have explicit semantic RGB values. Rounded-surface background, normal text, paused overlay, debug text, divider and window background adapt to the selected theme.

Theme changes are stored persistently and restored before the first fully rendered frame of the next application session so the window does not visibly flash the wrong theme during launch.

## Debug mode

Debug mode is a persistent optional diagnostic mode and is disabled by default. The design deliberately distinguishes normal-path data required for correct keyboard transport from diagnostic instrumentation that exists only to measure or explain behavior.

When debug mode is disabled, per-transition diagnostic timestamp collection, verbose ESP telemetry, recurring synchronization measurements, detailed boot instrumentation and nonessential diagnostic CDC/ESP-NOW packets are disabled rather than merely hidden. A small set of startup-duration scalars is retained even with debug off so enabling debug can display the completed startup timings. Normal protocol state, acknowledgements, dead-man handling and critical error detection continue because they are required for correctness. The objective is that disabling debug removes avoidable recurring work from Linux, bridge A and bridge B and therefore prevents diagnostics from becoming a source of latency or jitter.

When debug mode is enabled, the upper part of the rounded input surface displays a compact diagnostic segment before ordinary input history. It contains no heading, title, description, icon or explanatory copy. Each metric occupies only the space required to communicate its current value. A restrained divider with intentional spacing above and below separates diagnostic information from the input history.

The diagnostic segment reports application startup duration in milliseconds. It reports USB CDC connection time in milliseconds measured from the point at which the application is ready to attempt CDC connection until bridge A is operational over CDC. It reports bridge A boot time and bridge B boot time independently in milliseconds using firmware timestamps defined consistently at build time. It reports ESP-NOW application-session connection time in milliseconds.

A current critical application error appears as a short diagnostic line while the error remains active. Examples include invalid required configuration, input-device access failure or unrecoverable internal execution failure. A resolved temporary error is automatically removed rather than retained as a historical log.

Critical ESP errors or startup failures are similarly shown as short current-state messages. Critical ESP-NOW authentication/encryption failures and persistent heartbeat or reliability failures are shown only when sufficiently serious to affect link correctness or latency. Normal successful acknowledgements, heartbeat packets, individual retransmissions and ordinary status changes are not dumped into the debug area.

Diagnostic wording remains concise, for example `App start 118 ms`, `USB CDC 42 ms`, `ESP A boot 284 ms`, `ESP B boot 271 ms`, `ESP-NOW 18 ms`, `Error: input permission`, `ESP B: USB init failed`, `ESP-NOW: auth failure` or `Heartbeat: persistent loss`. Debug is a status display rather than a scrolling log console.

Debug content and its divider are not part of the input document. Backspace, Delete, Ctrl combinations, caret movement or any other captured keyboard operation cannot modify debug content, divider placement or divider spacing.

## End-to-end latency instrumentation

When debug mode is enabled, the system measures the complete bridge latency from Linux physical keyboard event receipt through the target-facing USB HID transfer. The metric is associated with a unique transition identifier so acknowledgements and retransmissions cannot be mistaken for new samples.

Linux timestamps the accepted evdev transition using a monotonic high-resolution clock. Bridge B timestamps completion of the corresponding USB HID IN transfer rather than merely timestamping ESP-NOW receipt. Diagnostic clock synchronization between Linux and bridge B is enabled only in debug mode. It uses lightweight bidirectional timing exchanges through CDC and ESP-NOW to estimate the offset between the Linux monotonic clock and bridge B's high-resolution timer, selecting low-delay samples to reduce queueing error.

Bridge B reports the transition identifier and HID transfer-completion timestamp back to Linux through bridge A. Linux maps the bridge timestamp into its monotonic clock domain and calculates the elapsed interval from captured physical input to completed HID transfer. This measures the complete controllable bridge path through transmission onto the target USB bus; target operating-system scheduling after USB receipt lies beyond what this hardware can directly observe.

The UI shows only `Latency min … ms / max … ms / avg … ms`, calculated over the most recent twenty completed keypress samples. Releases remain fully timestamped when needed for internal correlation but the displayed sample window is based on keypress events as specified. The oldest sample drops automatically when a twenty-first sample becomes available.

When debug is disabled, clock synchronization traffic, diagnostic event timestamps, USB completion telemetry and rolling latency statistics are not generated.

## Application text and event-history behavior

The visible input area is not a log of raw key events. Ordinary typing looks like typing into a normal editor. Text wraps naturally as the window narrows and reflows as it widens. Explicit newlines remain explicit. Backspace and Delete affect only ordinary input-history content. Space, Enter, Shift, Caps Lock, navigation and normal printable combinations behave according to the editor-like visual model.

Non-text key presses remain visible in a concise representation without altering their actual transmitted meaning. Pressing F3 forwards F3 and produces an `[F3]` indication. Arrow keys perform the intended local history navigation behavior and can additionally produce their concise special-key representation in the event-display model without being converted into ordinary characters. Modifier chords that would normally be application editing shortcuts are represented as combinations rather than executed against the local history.

The application does not support selecting old history, dragging text, moving a caret using the mouse, copying history or pasting arbitrary clipboard data into the transport. Clipboard input never generates HID traffic.

The history is session-local unless later persistence is explicitly required. Light/dark and debug preferences persist; typed content does not need to persist after application termination.

## Project organization

The project root contains the application under `app` and firmware under `firmware`. The `app` directory is a self-contained application package as described above.

The `firmware` directory contains all ESP32-S3 source, ESP-IDF project definitions, shared protocol definitions, firmware configuration, flashing tools and firmware-specific documentation. Bridge A and bridge B remain distinct firmware targets while sharing common protocol, cryptographic configuration structures, packet definitions, LED-state logic and utilities where appropriate.

A suitable internal organization is:

```text
firmware/
├── README.md
├── bridge-a/
│   └── ESP32-S3 USB CDC + ESP-NOW firmware
├── bridge-b/
│   └── ESP32-S3 ESP-NOW + USB HID firmware
├── common/
│   └── shared protocol and platform code
├── config/
│   └── board and build configuration
└── tools/
    └── build, provision and flash tooling
```

Provisioning tools generate or install the pair-specific PMK, LMK, peer MAC addresses and fixed radio-channel configuration in a controlled manner. Production flashing does not depend on manually editing source files for every build unless an intentionally fixed one-off pair is being produced.

The firmware README documents required ESP-IDF version, supported board assumptions, USB connectors, how bridge A and bridge B are distinguished, build procedure, provisioning, flashing procedure, boot-time measurement and expected LED states. Application README documentation covers launch procedure, CachyOS input permissions, CDC permissions, selecting the source keyboard, focus/capture semantics, configuration storage and debug interpretation.

Files that belong specifically to the Linux application stay under `app`. Files that belong specifically to the ESP32-S3 implementation stay under `firmware`. Cross-project documentation, public-release metadata, testing infrastructure or other root-level files must follow `agents.md` and `github-public-release-guide.md` as the authoritative repository-organization requirements. Those two files were not available while this concept revision was produced, so this concept intentionally does not invent conflicting root-level conventions beyond the explicitly required `app` and `firmware` directories.

The resulting system remains deliberately narrow: one local keyboard-capture window, one USB CDC radio bridge, one permanently paired encrypted ESP-NOW peer, one USB HID output bridge and no network service, account system, discoverable pairing mechanism or unnecessary application UI.
