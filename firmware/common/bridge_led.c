#include "bridge_led.h"
#include "bridge_config.h"
#include "esp_log.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "led_strip.h"

static led_strip_handle_t strip;
static volatile bridge_led_state_t current = BRIDGE_LED_BOOT;
static volatile int64_t changed_us;
static volatile int64_t activity_until_us;
static volatile int64_t error_until_us;
static volatile bool identify;
static volatile bool identify_b;

static void set_rgb(uint8_t r, uint8_t g, uint8_t b) { if (!strip) return; led_strip_set_pixel(strip, 0, r, g, b); led_strip_refresh(strip); }
static void led_task(void *unused) {
    (void)unused;
    for (;;) {
        int64_t now = esp_timer_get_time(); bridge_led_state_t state = current; bool on = true; uint8_t r=0,g=0,b=0; int64_t phase = now / 1000;
        if (state == BRIDGE_LED_ERROR) { on=(phase % 240) < 120; r=64; }
        else if (identify) { int period = identify_b ? 900 : 600; int pulse = identify_b ? ((phase % period < 100) || (phase % period >= 200 && phase % period < 300)) : phase % period < 100; on=pulse; r=64; b=64; }
        else if (state == BRIDGE_LED_BOOT) { on=(phase % 400) < 150; r=16;g=8;b=64; }
        else if (state == BRIDGE_LED_SEARCHING) { on=(phase % 1000) < 300; g=64; }
        else if (state == BRIDGE_LED_PAUSED) { g=64; }
        else if (state == BRIDGE_LED_AMBER) { on=(phase % 2000) < 400; r=64;g=18; }
        else { r=0;g=24;b=64; }
        if (state != BRIDGE_LED_ERROR && state != BRIDGE_LED_PAUSED && !identify && now < activity_until_us) { on=true;r=64;g=59;b=24; }
        if (state != BRIDGE_LED_ERROR && state != BRIDGE_LED_PAUSED && !identify && now - changed_us > 300000000LL) on=false;
        if (on) set_rgb(r,g,b); else if (strip) { led_strip_clear(strip); }
        vTaskDelay(pdMS_TO_TICKS(20));
    }
}
void bridge_led_init(int gpio) {
    led_strip_config_t cfg = { .strip_gpio_num=gpio, .max_leds=1, .led_model=LED_MODEL_WS2812, .color_component_format=LED_STRIP_COLOR_COMPONENT_FMT_GRB };
    led_strip_rmt_config_t rmt = { .clk_src=RMT_CLK_SRC_DEFAULT, .resolution_hz=10*1000*1000, .flags.with_dma=false };
    if (led_strip_new_rmt_device(&cfg, &rmt, &strip) != ESP_OK) { ESP_LOGW("bridge_led", "RGB unavailable"); return; }
    changed_us=esp_timer_get_time(); xTaskCreate(led_task,"bridge_led",2048,NULL,1,NULL);
}
void bridge_led_set(bridge_led_state_t state) {
    int64_t now=esp_timer_get_time();
    if (state != BRIDGE_LED_ERROR && now < error_until_us) return;
    if (state == BRIDGE_LED_ERROR) error_until_us=now+5000000;
    if (current != state) {
        current=state;
        changed_us=now;
    }
}
void bridge_led_activity(void) {
    int64_t now=esp_timer_get_time();
    changed_us=now;
    activity_until_us=now+100000;
}
void bridge_led_identify(bool bridge_b, bool enabled) { identify_b=bridge_b; identify=enabled; changed_us=esp_timer_get_time(); }
