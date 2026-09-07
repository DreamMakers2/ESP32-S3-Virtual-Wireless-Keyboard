#pragma once
#include <stdbool.h>
#include <stdint.h>
typedef enum { BRIDGE_LED_BOOT, BRIDGE_LED_SEARCHING, BRIDGE_LED_CONNECTED, BRIDGE_LED_PAUSED, BRIDGE_LED_AMBER, BRIDGE_LED_ERROR, BRIDGE_LED_IDENTIFY_A, BRIDGE_LED_IDENTIFY_B } bridge_led_state_t;
void bridge_led_init(int gpio);
void bridge_led_set(bridge_led_state_t state);
void bridge_led_activity(void);
void bridge_led_identify(bool bridge_b, bool enabled);
