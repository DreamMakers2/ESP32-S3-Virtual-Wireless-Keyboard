#include "bridge_session.h"
#include <string.h>

void bridge_session_reset(bridge_session_t *s, uint64_t epoch, uint32_t now) { memset(s, 0, sizeof(*s)); s->state=BRIDGE_SESSION_IDLE; s->epoch=epoch; s->next_rx_sequence=1; s->last_liveness_ms=now; s->last_progress_ms=now; }
bool bridge_session_start(bridge_session_t *s, uint64_t capture, uint64_t epoch, uint32_t now) { if (!s || !capture || epoch != s->epoch) return false; if (s->state == BRIDGE_SESSION_ACTIVE && s->session == capture) return true; if (s->state != BRIDGE_SESSION_IDLE) return false; s->session=capture; s->state=BRIDGE_SESSION_WAITING_READY; s->next_rx_sequence=1; s->last_liveness_ms=now; s->last_progress_ms=now; return true; }
bool bridge_session_accept_state(const bridge_session_t *s, const bridge_packet_t *p) { return s && p && s->state == BRIDGE_SESSION_ACTIVE && p->type == BRIDGE_STATE && p->session == s->session && p->epoch == s->epoch && p->sequence == s->next_rx_sequence && p->payload_len == BRIDGE_STATE_BYTES; }
bool bridge_session_liveness_valid(const bridge_session_t *s, const bridge_packet_t *p) { return s && p && s->state == BRIDGE_SESSION_ACTIVE && p->session == s->session && p->epoch == s->epoch && (p->type == BRIDGE_HEARTBEAT || p->type == BRIDGE_STOP); }
bool bridge_session_expired(const bridge_session_t *s, uint32_t now, bool progress_pending) {
    return s && s->state == BRIDGE_SESSION_ACTIVE &&
           ((uint32_t)(now - s->last_liveness_ms) > BRIDGE_DEADMAN_MS ||
            (progress_pending && (uint32_t)(now - s->last_progress_ms) > BRIDGE_DEADMAN_MS));
}
