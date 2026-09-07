#include "bridge_radio.h"
#include "bridge_config.h"
#include "esp_event.h"
#include "esp_flash.h"
#include "esp_log.h"
#include "esp_mac.h"
#include "esp_now.h"
#include "esp_random.h"
#include "esp_psram.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/queue.h"
#include "nvs_flash.h"
#include <string.h>

typedef struct { uint8_t mac[6]; uint8_t data[BRIDGE_MAX_PACKET]; uint8_t length; } inbound_t;
static QueueHandle_t received;
static QueueHandle_t received_stop;
static bridge_radio_receive_fn receiver;
static void *receiver_context;
static bool radio_is_ready;
static const uint8_t expected_peer[6] = BRIDGE_PEER_MAC;

static bool hardware_matches_board(void) {
    uint32_t flash_size = 0;
    size_t psram_size = esp_psram_get_size();
    if (esp_flash_get_size(NULL, &flash_size) != ESP_OK || flash_size != 16u * 1024u * 1024u || psram_size < 8u * 1024u * 1024u) {
        ESP_LOGE("bridge_radio", "unsupported board: flash=%u psram=%u", (unsigned)flash_size, (unsigned)psram_size);
        return false;
    }
    ESP_LOGI("bridge_radio", "board check: ESP32-S3 flash=%u psram=%u", (unsigned)flash_size, (unsigned)psram_size);
    return true;
}

static void on_receive(const esp_now_recv_info_t *info, const uint8_t *data, int len) {
    if (!info || !data || len < (int)(BRIDGE_HEADER_SIZE+BRIDGE_CRC_SIZE) || len > (int)BRIDGE_MAX_PACKET || memcmp(info->src_addr, expected_peer, 6) != 0) return;
    inbound_t event = { .length=(uint8_t)len }; memcpy(event.mac, info->src_addr, 6); memcpy(event.data,data,len);
    /* STOP is allowed to replace an earlier STOP and has its own queue slot. */
    if (data[1] == BRIDGE_STOP) (void)xQueueOverwrite(received_stop,&event);
    else (void)xQueueSend(received,&event,0);
}
static void radio_task(void *ignored) {
    (void)ignored; inbound_t event; bridge_packet_t packet;
    for (;;) {
        /* STOP bypasses queued state traffic. */
        if (xQueueReceive(received_stop,&event,0) != pdTRUE && xQueueReceive(received,&event,0) != pdTRUE) {
            vTaskDelay(pdMS_TO_TICKS(1));
            continue;
        }
        if (bridge_packet_decode(event.data,event.length,&packet) && receiver) receiver(&packet,receiver_context);
    }
}
bool bridge_radio_init(bridge_radio_receive_fn callback, void *context, uint32_t *ready_us) {
    if (!hardware_matches_board()) return false;
#ifdef BRIDGE_NONDEPLOYABLE_FIXTURE
    ESP_LOGE("bridge_radio", "NONDEPLOYABLE test fixture: radio disabled"); return false;
#else
    uint8_t own[6] = BRIDGE_OWN_MAC, peer[6] = BRIDGE_PEER_MAC, pmk[16] = BRIDGE_PMK, lmk[16] = BRIDGE_LMK, actual[6];
    if (esp_read_mac(actual, ESP_MAC_WIFI_STA) != ESP_OK || memcmp(actual,own,6)) { ESP_LOGE("bridge_radio","provisioned identity does not match eFuse MAC"); return false; }
    receiver=callback; receiver_context=context;
    received=xQueueCreate(12,sizeof(inbound_t)); received_stop=xQueueCreate(1,sizeof(inbound_t));
    if (!received || !received_stop) return false;
    esp_err_t nvs= nvs_flash_init();
    if (nvs == ESP_ERR_NVS_NO_FREE_PAGES) {
        if (nvs_flash_erase() != ESP_OK) return false;
        nvs=nvs_flash_init();
    }
    if (nvs != ESP_OK) return false;
    if (esp_netif_init()!=ESP_OK || esp_event_loop_create_default()!=ESP_OK) return false;
    wifi_init_config_t cfg=WIFI_INIT_CONFIG_DEFAULT(); if (esp_wifi_init(&cfg)!=ESP_OK || esp_wifi_set_mode(WIFI_MODE_STA)!=ESP_OK || esp_wifi_start()!=ESP_OK || esp_wifi_set_channel(BRIDGE_PAIR_CHANNEL,WIFI_SECOND_CHAN_NONE)!=ESP_OK || esp_wifi_set_ps(WIFI_PS_NONE)!=ESP_OK || esp_now_init()!=ESP_OK || esp_now_set_pmk(pmk)!=ESP_OK) return false;
    esp_now_peer_info_t pi={0}; memcpy(pi.peer_addr,peer,6); memcpy(pi.lmk,lmk,16); pi.channel=BRIDGE_PAIR_CHANNEL; pi.ifidx=WIFI_IF_STA; pi.encrypt=true;
    if (esp_now_add_peer(&pi)!=ESP_OK || esp_now_register_recv_cb(on_receive)!=ESP_OK) return false;
    xTaskCreate(radio_task,"bridge_radio",4096,NULL,10,NULL); radio_is_ready=true; if (ready_us) *ready_us=(uint32_t)esp_timer_get_time(); return true;
#endif
}
bool bridge_radio_send(const bridge_packet_t *packet) {
#ifdef BRIDGE_NONDEPLOYABLE_FIXTURE
    (void)packet; return false;
#else
    uint8_t wire[BRIDGE_MAX_PACKET], peer[6]=BRIDGE_PEER_MAC; size_t len=0; return radio_is_ready && bridge_packet_encode(packet,wire,sizeof(wire),&len) && esp_now_send(peer,wire,len)==ESP_OK;
#endif
}
bool bridge_radio_ready(void) { return radio_is_ready; }
