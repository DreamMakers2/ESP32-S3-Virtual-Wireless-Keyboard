#include "bridge_protocol.h"
#include <string.h>

static void put16(uint8_t *p, uint16_t value) { p[0] = (uint8_t)value; p[1] = (uint8_t)(value >> 8); }
static void put32(uint8_t *p, uint32_t value) { for (unsigned i = 0; i < 4; ++i) p[i] = (uint8_t)(value >> (8u * i)); }
static void put64(uint8_t *p, uint64_t value) { for (unsigned i = 0; i < 8; ++i) p[i] = (uint8_t)(value >> (8u * i)); }
static uint16_t get16(const uint8_t *p) { return (uint16_t)p[0] | ((uint16_t)p[1] << 8); }
static uint32_t get32(const uint8_t *p) { uint32_t r = 0; for (unsigned i = 0; i < 4; ++i) r |= (uint32_t)p[i] << (8u * i); return r; }
static uint64_t get64(const uint8_t *p) { uint64_t r = 0; for (unsigned i = 0; i < 8; ++i) r |= (uint64_t)p[i] << (8u * i); return r; }

uint32_t bridge_crc32(const uint8_t *data, size_t len) {
    uint32_t crc = 0xffffffffu;
    while (len--) { crc ^= *data++; for (unsigned i = 0; i < 8; ++i) crc = (crc >> 1) ^ (0xedb88320u & (-(int32_t)(crc & 1u))); }
    return ~crc;
}

bool bridge_packet_is_valid_type(uint8_t type) { return type >= BRIDGE_HELLO && type <= BRIDGE_IDENTIFY; }
void bridge_packet_init(bridge_packet_t *p, bridge_message_type_t type, uint64_t session, uint64_t epoch, uint32_t seq) {
    memset(p, 0, sizeof(*p)); p->type = (uint8_t)type; p->session = session; p->epoch = epoch; p->sequence = seq;
}
bool bridge_packet_encode(const bridge_packet_t *p, uint8_t *out, size_t cap, size_t *out_len) {
    if (!p || !out || !out_len || !bridge_packet_is_valid_type(p->type) || p->payload_len > BRIDGE_MAX_PAYLOAD || p->flags & ~BRIDGE_FLAG_KEYPRESS_SAMPLE) return false;
    size_t len = BRIDGE_HEADER_SIZE + p->payload_len + BRIDGE_CRC_SIZE;
    if (cap < len) return false;
    out[0] = BRIDGE_PROTOCOL_VERSION; out[1] = p->type; put16(out + 2, p->flags); put64(out + 4, p->session); put64(out + 12, p->epoch); put32(out + 20, p->sequence); put16(out + 24, p->payload_len); put16(out + 26, 0);
    if (p->payload_len) memcpy(out + BRIDGE_HEADER_SIZE, p->payload, p->payload_len);
    put32(out + len - BRIDGE_CRC_SIZE, bridge_crc32(out, len - BRIDGE_CRC_SIZE)); *out_len = len; return true;
}
bool bridge_packet_decode(const uint8_t *wire, size_t len, bridge_packet_t *out) {
    if (!wire || !out || len < BRIDGE_HEADER_SIZE + BRIDGE_CRC_SIZE || len > BRIDGE_MAX_PACKET || wire[0] != BRIDGE_PROTOCOL_VERSION || !bridge_packet_is_valid_type(wire[1])) return false;
    uint16_t payload_len = get16(wire + 24); if (payload_len > BRIDGE_MAX_PAYLOAD || len != BRIDGE_HEADER_SIZE + payload_len + BRIDGE_CRC_SIZE || get16(wire + 26) != 0 || (get16(wire + 2) & ~BRIDGE_FLAG_KEYPRESS_SAMPLE) || get32(wire + len - 4) != bridge_crc32(wire, len - 4)) return false;
    memset(out, 0, sizeof(*out)); out->type = wire[1]; out->flags = get16(wire + 2); out->session = get64(wire + 4); out->epoch = get64(wire + 12); out->sequence = get32(wire + 20); out->payload_len = payload_len; if (payload_len) memcpy(out->payload, wire + BRIDGE_HEADER_SIZE, payload_len); return true;
}
size_t bridge_cobs_encode(const uint8_t *in, size_t len, uint8_t *out, size_t cap) {
    if (!out || (!in && len) || cap == 0) return 0;
    size_t read = 0, write = 1, code_index = 0;
    uint8_t code = 1;
    while (read < len) { if (in[read] == 0) { if (code_index >= cap) return 0; out[code_index] = code; code = 1; code_index = write++; } else { if (write >= cap) return 0; out[write++] = in[read]; if (++code == 0xff) { if (code_index >= cap) return 0; out[code_index] = code; code = 1; code_index = write++; } } ++read; }
    if (code_index >= cap) return 0;
    out[code_index] = code;
    return write;
}
size_t bridge_cobs_decode(const uint8_t *in, size_t len, uint8_t *out, size_t cap) {
    if (!in || !out) return 0;
    size_t read = 0, write = 0;
    while (read < len) {
        uint8_t code = in[read++];
        if (!code || read + code - 1 > len || write + code - 1 > cap) return 0;
        for (uint8_t i = 1; i < code; ++i) out[write++] = in[read++];
        if (code != 0xff && read < len) { if (write >= cap) return 0; out[write++] = 0; }
    }
    return write;
}
void bridge_status_encode(const bridge_status_t *s, uint8_t p[16]) { p[0]=s->ready; p[1]=s->leds; put16(p+2,s->error); put32(p+4,s->a_boot_us); put32(p+8,s->b_boot_us); put32(p+12,s->radio_ready_us); }
bool bridge_status_decode(const uint8_t *p, size_t len, bridge_status_t *s) { if (!p || !s || len != 16) return false; s->ready=p[0]; s->leds=p[1]; s->error=get16(p+2); s->a_boot_us=get32(p+4); s->b_boot_us=get32(p+8); s->radio_ready_us=get32(p+12); return true; }
