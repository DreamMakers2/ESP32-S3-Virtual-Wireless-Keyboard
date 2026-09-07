#include "bridge_protocol.h"
#include "unity.h"

TEST_CASE("protocol encodes canonical HELLO vector", "[bridge_protocol]") {
    static const uint8_t expected[] = {0x01,0x01,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0xb5,0x91,0x49,0xef};
    bridge_packet_t p; uint8_t out[BRIDGE_MAX_PACKET]; size_t length=0;
    bridge_packet_init(&p,BRIDGE_HELLO,0,0,0);
    TEST_ASSERT_TRUE(bridge_packet_encode(&p,out,sizeof(out),&length));
    TEST_ASSERT_EQUAL_UINT32(sizeof(expected),length); TEST_ASSERT_EQUAL_HEX8_ARRAY(expected,out,length);
    TEST_ASSERT_TRUE(bridge_packet_decode(expected,sizeof(expected),&p)); TEST_ASSERT_EQUAL(BRIDGE_HELLO,p.type);
}
TEST_CASE("protocol encodes canonical STATE vector", "[bridge_protocol]") {
    static const uint8_t expected[65] = {
        0x01,0x05,0x01,0, 0x08,0x07,0x06,0x05,0x04,0x03,0x02,0x01,
        0x18,0x17,0x16,0x15,0x14,0x13,0x12,0x11, 0x01,0,0,0, 0x21,0,0,0,
        0x02,0x10,
        [61]=0xe0,0x05,0x92,0x12
    };
    bridge_packet_t p; uint8_t out[BRIDGE_MAX_PACKET]; size_t length=0;
    bridge_packet_init(&p,BRIDGE_STATE,0x0102030405060708ULL,0x1112131415161718ULL,1); p.flags=BRIDGE_FLAG_KEYPRESS_SAMPLE; p.payload_len=BRIDGE_STATE_BYTES; p.payload[0]=2; p.payload[1]=0x10;
    TEST_ASSERT_TRUE(bridge_packet_encode(&p,out,sizeof(out),&length));
    TEST_ASSERT_EQUAL_UINT32(sizeof(expected),length); TEST_ASSERT_EQUAL_HEX8_ARRAY(expected,out,length);
    TEST_ASSERT_TRUE(bridge_packet_decode(expected,sizeof(expected),&p)); TEST_ASSERT_EQUAL_UINT32(1,p.sequence); TEST_ASSERT_EQUAL_HEX8(2,p.payload[0]);
}
TEST_CASE("COBS round trip preserves zeroes", "[bridge_protocol]") {
    uint8_t input[]={0,1,2,0,3,0}, encoded[16], decoded[16]; size_t n=bridge_cobs_encode(input,sizeof(input),encoded,sizeof(encoded));
    TEST_ASSERT_NOT_EQUAL(0,n); n=bridge_cobs_decode(encoded,n,decoded,sizeof(decoded)); TEST_ASSERT_EQUAL_UINT32(sizeof(input),n); TEST_ASSERT_EQUAL_HEX8_ARRAY(input,decoded,n);
}
