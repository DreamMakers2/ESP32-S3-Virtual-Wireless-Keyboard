#include "bridge_session.h"
#include <assert.h>
#include <stdio.h>

int main(void) {
    bridge_session_t s;
    bridge_session_reset(&s, 100, 0);
    assert(!bridge_session_start(&s, 7, 99, 0));
    assert(bridge_session_start(&s, 7, 100, 0));
    s.state = BRIDGE_SESSION_ACTIVE;
    bridge_packet_t p = {.type=BRIDGE_STATE, .session=7, .epoch=100,
                         .sequence=1, .payload_len=BRIDGE_STATE_BYTES};
    assert(bridge_session_accept_state(&s, &p));
    p.epoch=99;
    assert(!bridge_session_accept_state(&s, &p));
    /* Heartbeats preserve a quiet hold, but cannot hide a stuck transfer. */
    s.last_liveness_ms=1000;
    assert(!bridge_session_expired(&s, 1000, false));
    assert(bridge_session_expired(&s, 1000, true));
    s.last_progress_ms=1000;
    assert(!bridge_session_expired(&s, 1250, true));
    assert(bridge_session_expired(&s, 1251, true));
    bridge_session_reset(&s, 101, 1300);
    assert(s.session==0 && s.next_rx_sequence==1);
    assert(!bridge_session_start(&s, 7, 100, 1300));
    puts("session epoch, quiet-hold and progress-deadline checks passed");
}
