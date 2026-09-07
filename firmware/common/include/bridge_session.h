#pragma once
#include "bridge_protocol.h"

#define BRIDGE_QUEUE_CAPACITY 32u
#define BRIDGE_HEARTBEAT_MS 50u
#define BRIDGE_LEASE_MS 150u
#define BRIDGE_DEADMAN_MS 250u
#define BRIDGE_RETRANSMIT_MS 10u

typedef enum { BRIDGE_SESSION_IDLE, BRIDGE_SESSION_WAITING_READY, BRIDGE_SESSION_ACTIVE, BRIDGE_SESSION_FAILED } bridge_session_state_t;
typedef struct {
    bridge_session_state_t state;
    uint64_t session, epoch;
    uint32_t next_rx_sequence, last_complete_sequence;
    uint32_t last_liveness_ms, last_progress_ms;
} bridge_session_t;

void bridge_session_reset(bridge_session_t *session, uint64_t new_epoch, uint32_t now_ms);
bool bridge_session_start(bridge_session_t *session, uint64_t capture_id, uint64_t advertised_epoch, uint32_t now_ms);
bool bridge_session_accept_state(const bridge_session_t *session, const bridge_packet_t *packet);
bool bridge_session_liveness_valid(const bridge_session_t *session, const bridge_packet_t *packet);
/* A quiet, healthy keyboard is valid.  Progress is only required while an HID
 * report is waiting for completion. */
bool bridge_session_expired(const bridge_session_t *session, uint32_t now_ms, bool progress_pending);
