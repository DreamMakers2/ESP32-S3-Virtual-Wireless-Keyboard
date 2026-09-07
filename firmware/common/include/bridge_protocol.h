#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define BRIDGE_PROTOCOL_VERSION 1u
#define BRIDGE_MAX_PAYLOAD 96u
#define BRIDGE_HEADER_SIZE 28u
#define BRIDGE_CRC_SIZE 4u
#define BRIDGE_MAX_PACKET (BRIDGE_HEADER_SIZE + BRIDGE_MAX_PAYLOAD + BRIDGE_CRC_SIZE)
#define BRIDGE_MAX_COBS (BRIDGE_MAX_PACKET + (BRIDGE_MAX_PACKET / 254u) + 1u)
#define BRIDGE_STATE_BYTES 33u

typedef enum {
    BRIDGE_HELLO = 1, BRIDGE_STATUS, BRIDGE_START, BRIDGE_READY, BRIDGE_STATE,
    BRIDGE_ACK, BRIDGE_HEARTBEAT, BRIDGE_STOP, BRIDGE_DEBUG, BRIDGE_LEDS,
    BRIDGE_SYNC, BRIDGE_SYNC_REPLY, BRIDGE_ERROR, BRIDGE_IDENTIFY,
} bridge_message_type_t;

enum { BRIDGE_FLAG_KEYPRESS_SAMPLE = 1u };
enum { BRIDGE_ERROR_PROTOCOL = 1, BRIDGE_ERROR_OVERFLOW, BRIDGE_ERROR_TIMEOUT,
       BRIDGE_ERROR_USB, BRIDGE_ERROR_RADIO, BRIDGE_ERROR_CONFIGURATION };

typedef struct {
    uint8_t type;
    uint16_t flags;
    uint64_t session;
    uint64_t epoch;
    uint32_t sequence;
    uint16_t payload_len;
    uint8_t payload[BRIDGE_MAX_PAYLOAD];
} bridge_packet_t;

typedef struct { uint8_t modifiers; uint8_t usages[32]; } bridge_key_state_t;
typedef struct { uint8_t ready, leds; uint16_t error; uint32_t a_boot_us, b_boot_us, radio_ready_us; } bridge_status_t;

uint32_t bridge_crc32(const uint8_t *data, size_t len);
bool bridge_packet_encode(const bridge_packet_t *packet, uint8_t *out, size_t out_capacity, size_t *out_len);
bool bridge_packet_decode(const uint8_t *wire, size_t wire_len, bridge_packet_t *out);
size_t bridge_cobs_encode(const uint8_t *input, size_t length, uint8_t *output, size_t capacity);
size_t bridge_cobs_decode(const uint8_t *input, size_t length, uint8_t *output, size_t capacity);
bool bridge_packet_is_valid_type(uint8_t type);
void bridge_packet_init(bridge_packet_t *packet, bridge_message_type_t type, uint64_t session, uint64_t epoch, uint32_t sequence);
void bridge_status_encode(const bridge_status_t *status, uint8_t payload[16]);
bool bridge_status_decode(const uint8_t *payload, size_t len, bridge_status_t *status);
