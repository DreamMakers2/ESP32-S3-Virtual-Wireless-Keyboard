#pragma once
#include "bridge_protocol.h"
#include <stdbool.h>
#include <stdint.h>

typedef void (*bridge_radio_receive_fn)(const bridge_packet_t *packet, void *context);
bool bridge_radio_init(bridge_radio_receive_fn callback, void *context, uint32_t *ready_us);
bool bridge_radio_send(const bridge_packet_t *packet);
bool bridge_radio_ready(void);
