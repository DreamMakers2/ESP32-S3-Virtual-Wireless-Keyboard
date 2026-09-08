#include "bridge_config.h"
#include "bridge_led.h"
#include "bridge_maintenance.h"
#include "bridge_protocol.h"
#include "bridge_radio.h"
#include "bridge_session.h"
#include "esp_log.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/queue.h"
#include "freertos/semphr.h"
#include "freertos/task.h"
#include "tinyusb.h"
#include "tinyusb_cdc_acm.h"
#include "tinyusb_default_config.h"
#include "class/cdc/cdc_device.h"
#include "driver/uart.h"
#include <stdio.h>
#include <stdatomic.h>
#include <string.h>

#define TAG "bridge_a"
#define HOST_HELLO_TIMEOUT_MS 750u
typedef struct { uint8_t bytes[128]; size_t length; uint32_t generation; } cdc_chunk_t;
typedef struct { bridge_packet_t packet; uint32_t sent_ms, queued_ms; } pending_t;
static QueueHandle_t cdc_rx_queue;
static SemaphoreHandle_t cdc_write_lock;
static SemaphoreHandle_t state_lock;
static pending_t pending[BRIDGE_QUEUE_CAPACITY];
static uint8_t pending_count;
static uint64_t active_session, peer_epoch;
static uint32_t expected_sequence=1, lease_ms, boot_us, radio_ready_us, last_status_ms, last_app_hello_ms, last_ack_sequence;
static bool debug_enabled, peer_ready, app_hello_seen;
static bridge_packet_t last_ack;
static bool have_last_ack;
/* Lock-free on the CDC path; state transitions occur under state_lock. */
static _Atomic bool usb_recovering, usb_attached;
static _Atomic uint32_t cdc_generation;
static bool reconnect_pending;
static uint32_t disconnected_ms;

static uint32_t now_ms(void) { return (uint32_t)(esp_timer_get_time()/1000); }
/* Queue depth is 32, so signed modular ordering remains unambiguous at wrap. */
static bool sequence_before(uint32_t a, uint32_t b) { return (int32_t)(a-b) < 0; }
static bool sequence_before_or_equal(uint32_t a, uint32_t b) { return (int32_t)(a-b) <= 0; }
static void cdc_write_packet(const bridge_packet_t *packet) {
    uint8_t raw[BRIDGE_MAX_PACKET], framed[BRIDGE_MAX_COBS+1]; size_t raw_len=0, framed_len;
    if (atomic_load(&usb_recovering) || !atomic_load(&usb_attached)) return;
    if (!bridge_packet_encode(packet,raw,sizeof(raw),&raw_len)) return;
    framed_len=bridge_cobs_encode(raw,raw_len,framed,sizeof(framed)-1);
    if (!framed_len) return;
    framed[framed_len++]=0;
    if (xSemaphoreTake(cdc_write_lock,pdMS_TO_TICKS(20)) == pdTRUE) {
        if (!atomic_load(&usb_recovering) && atomic_load(&usb_attached)) {
            tinyusb_cdcacm_write_queue(TINYUSB_CDC_ACM_0,framed,framed_len);
            tinyusb_cdcacm_write_flush(TINYUSB_CDC_ACM_0,0);
        }
        xSemaphoreGive(cdc_write_lock);
    }
}
static void lock_state(void) { (void)xSemaphoreTake(state_lock,portMAX_DELAY); }
static void unlock_state(void) { (void)xSemaphoreGive(state_lock); }
static void send_error_locked(uint16_t code) { bridge_packet_t p; bridge_packet_init(&p,BRIDGE_ERROR,active_session,peer_epoch,0); p.payload_len=2; p.payload[0]=(uint8_t)code;p.payload[1]=(uint8_t)(code>>8); cdc_write_packet(&p); }
static void stop_capture_locked(bool tell_b) {
    if (tell_b && active_session) { bridge_packet_t stop; bridge_packet_init(&stop,BRIDGE_STOP,active_session,peer_epoch,0); bridge_radio_send(&stop); }
    active_session=0; expected_sequence=1; pending_count=0; peer_ready=false; have_last_ack=false; last_ack_sequence=0;
}
static void cdc_rx_callback(int itf, cdcacm_event_t *event) {
    (void)event; cdc_chunk_t chunk={0}; size_t read=0;
    if (tinyusb_cdcacm_read(itf,chunk.bytes,sizeof(chunk.bytes),&read)==ESP_OK && read) {
        if (atomic_load(&usb_recovering) || !atomic_load(&usb_attached)) return;
        chunk.length=read; chunk.generation=atomic_load(&cdc_generation);
        (void)xQueueSend(cdc_rx_queue,&chunk,0);
    }
}
static void handle_from_radio(const bridge_packet_t *packet, void *context) {
    (void)context;
    bridge_packet_t forwarded=*packet;
    lock_state();
    if (packet->type==BRIDGE_STATUS && packet->payload_len==16) { bridge_status_t status; if (bridge_status_decode(packet->payload,16,&status)) { if (active_session && peer_epoch && peer_epoch!=packet->epoch) stop_capture_locked(false); peer_epoch=packet->epoch; last_status_ms=now_ms(); if (!active_session) peer_ready=status.ready!=0; else if (status.ready) peer_ready=true; status.ready=peer_ready; status.a_boot_us=boot_us; bridge_status_encode(&status,forwarded.payload); } }
    if (packet->type==BRIDGE_ACK && active_session && packet->session==active_session && packet->epoch==peer_epoch) { while (pending_count && sequence_before_or_equal(pending[0].packet.sequence,packet->sequence)) { memmove(pending,pending+1,(--pending_count)*sizeof(pending[0])); } last_ack=*packet; have_last_ack=true; last_ack_sequence=packet->sequence; }
    if (packet->type==BRIDGE_READY && active_session && packet->session==active_session && packet->epoch==peer_epoch) peer_ready=true;
    unlock_state();
    cdc_write_packet(&forwarded);
}
static bool queue_state_locked(const bridge_packet_t *packet) {
    if (pending_count>=BRIDGE_QUEUE_CAPACITY) { send_error_locked(BRIDGE_ERROR_OVERFLOW); stop_capture_locked(true); return false; }
    pending[pending_count++]=(pending_t){.packet=*packet,.sent_ms=now_ms(),.queued_ms=now_ms()}; bridge_radio_send(packet); return true;
}
static void handle_cdc_packet(const bridge_packet_t *p, uint32_t generation) {
    uint32_t now=now_ms();
    lock_state();
    if (generation!=atomic_load(&cdc_generation) || atomic_load(&usb_recovering) || !atomic_load(&usb_attached)) { unlock_state(); return; }
    if (p->type==BRIDGE_HELLO && p->payload_len==0) { app_hello_seen=true; last_app_hello_ms=now; bridge_packet_t q; bridge_packet_init(&q,BRIDGE_HELLO,0,0,0); bridge_radio_send(&q); unlock_state(); return; }
    if (p->type==BRIDGE_DEBUG && p->payload_len==1 && p->payload[0]<=1) { debug_enabled=p->payload[0]; bridge_radio_send(p); unlock_state(); return; }
    if (p->type==BRIDGE_START && p->payload_len==0 && p->session && p->epoch==peer_epoch && !active_session && peer_ready) { active_session=p->session; expected_sequence=1; lease_ms=now; bridge_radio_send(p); unlock_state(); return; }
    if (!active_session || p->session!=active_session || p->epoch!=peer_epoch) { send_error_locked(BRIDGE_ERROR_PROTOCOL); unlock_state(); return; }
    if (p->type==BRIDGE_START && p->payload_len==0) { bridge_radio_send(p); unlock_state(); return; }
    if (p->type==BRIDGE_STATE) {
        if (p->payload_len != BRIDGE_STATE_BYTES || (p->sequence != expected_sequence && !sequence_before(p->sequence,expected_sequence))) { send_error_locked(BRIDGE_ERROR_PROTOCOL); stop_capture_locked(true); unlock_state(); return; }
        if (p->sequence != expected_sequence) {
            if (have_last_ack && sequence_before_or_equal(p->sequence,last_ack_sequence)) cdc_write_packet(&last_ack);
            else for (uint8_t i=0;i<pending_count;++i) if (pending[i].packet.sequence==p->sequence) { bridge_radio_send(&pending[i].packet); break; }
            unlock_state(); return;
        }
        ++expected_sequence; bridge_led_activity(); queue_state_locked(p); unlock_state(); return;
    }
    if (p->type==BRIDGE_HEARTBEAT) { lease_ms=now; bridge_radio_send(p); unlock_state(); return; }
    if (p->type==BRIDGE_STOP) { stop_capture_locked(true); unlock_state(); return; }
    if (p->type==BRIDGE_SYNC && debug_enabled && p->payload_len==8) { bridge_radio_send(p); unlock_state(); return; }
    send_error_locked(BRIDGE_ERROR_PROTOCOL); unlock_state();
}
static void cdc_task(void *ignored) {
    (void)ignored; cdc_chunk_t chunk; uint8_t frame[BRIDGE_MAX_COBS]; size_t used=0; bool discarding=false; uint32_t generation=atomic_load(&cdc_generation);
    for (;;) {
        if (generation!=atomic_load(&cdc_generation)) { used=0; discarding=false; generation=atomic_load(&cdc_generation); }
        if (xQueueReceive(cdc_rx_queue,&chunk,pdMS_TO_TICKS(10))!=pdTRUE) continue;
        if (chunk.generation!=generation) { used=0; discarding=false; generation=chunk.generation; }
        if (chunk.generation!=atomic_load(&cdc_generation) || atomic_load(&usb_recovering) || !atomic_load(&usb_attached)) { used=0; discarding=false; generation=atomic_load(&cdc_generation); continue; }
        for (size_t i=0;i<chunk.length;++i) {
        if (generation!=atomic_load(&cdc_generation) || atomic_load(&usb_recovering) || !atomic_load(&usb_attached)) { used=0; discarding=false; generation=atomic_load(&cdc_generation); break; }
        uint8_t c=chunk.bytes[i];
        if (!c) {
            if (discarding) {
                /* A suffix is untrusted until this delimiter; never parse it. */
                lock_state(); send_error_locked(BRIDGE_ERROR_PROTOCOL); unlock_state();
                discarding=false;
            } else {
                uint8_t raw[BRIDGE_MAX_PACKET]; size_t n=bridge_cobs_decode(frame,used,raw,sizeof(raw)); bridge_packet_t p;
                if (n && bridge_packet_decode(raw,n,&p)) handle_cdc_packet(&p,generation);
                else if (used) { lock_state(); send_error_locked(BRIDGE_ERROR_PROTOCOL); unlock_state(); }
            }
            used=0;
        } else if (!discarding) {
            if (used<sizeof(frame)) frame[used++]=c;
            else discarding=true;
        }
        }
    }
}
void tud_suspend_cb(bool remote_wakeup_en) {
    (void)remote_wakeup_en;
    lock_state();
    /* Force a clean enumeration before accepting or emitting CDC traffic. */
    atomic_store(&usb_attached,false); atomic_store(&usb_recovering,true);
    (void)atomic_fetch_add(&cdc_generation,1);
    stop_capture_locked(true);
    app_hello_seen=false;
    disconnected_ms=now_ms(); reconnect_pending=true;
    unlock_state();
    if (xSemaphoreTake(cdc_write_lock,pdMS_TO_TICKS(20)) == pdTRUE) {
        (void)tud_cdc_n_write_clear(TINYUSB_CDC_ACM_0);
        xSemaphoreGive(cdc_write_lock);
    }
    (void)xQueueReset(cdc_rx_queue);
    (void)tud_disconnect();
}
void tud_resume_cb(void) { /* Recovery requires a fresh USB attachment. */ }
static void usb_event(tinyusb_event_t *event, void *arg) {
    (void)arg;
    lock_state();
    if (event->id==TINYUSB_EVENT_DETACHED) {
        atomic_store(&usb_attached,false); atomic_store(&usb_recovering,true);
        (void)atomic_fetch_add(&cdc_generation,1);
        stop_capture_locked(true);
        app_hello_seen=false;
    } else if (event->id==TINYUSB_EVENT_ATTACHED) {
        atomic_store(&usb_attached,true); atomic_store(&usb_recovering,false); reconnect_pending=false;
        (void)atomic_fetch_add(&cdc_generation,1);
        stop_capture_locked(true);
        app_hello_seen=false;
    }
    unlock_state();
    if (event->id==TINYUSB_EVENT_DETACHED) (void)xQueueReset(cdc_rx_queue);
}
static void control_task(void *ignored) {
    (void)ignored; for (;;) { uint32_t now=now_ms();
        lock_state();
        bool reconnect=false;
        if (reconnect_pending && (uint32_t)(now-disconnected_ms)>=100) { reconnect_pending=false; reconnect=true; }
        if (peer_ready && now-last_status_ms>HOST_HELLO_TIMEOUT_MS) { peer_ready=false; if (active_session) { send_error_locked(BRIDGE_ERROR_TIMEOUT); stop_capture_locked(true); } }
        if (active_session && now-lease_ms>BRIDGE_LEASE_MS) { send_error_locked(BRIDGE_ERROR_TIMEOUT); stop_capture_locked(true); }
        if (pending_count && now-pending[0].queued_ms>=BRIDGE_DEADMAN_MS) { send_error_locked(BRIDGE_ERROR_TIMEOUT); stop_capture_locked(true); }
        if (pending_count && now-pending[0].sent_ms>=BRIDGE_RETRANSMIT_MS) { bridge_radio_send(&pending[0].packet); pending[0].sent_ms=now; }
        bool active=active_session!=0;
        bool usb_ready=atomic_load(&usb_attached) && !atomic_load(&usb_recovering);
        bool paused=usb_ready && peer_ready && !active && app_hello_seen && now-last_status_ms<=HOST_HELLO_TIMEOUT_MS && now-last_app_hello_ms<=HOST_HELLO_TIMEOUT_MS;
        bool ready=usb_ready && peer_ready;
        unlock_state();
        if (reconnect) (void)tud_connect();
        if (!bridge_radio_ready()) bridge_led_set(BRIDGE_LED_ERROR);
        else if (active || (ready && !paused)) bridge_led_set(BRIDGE_LED_CONNECTED);
        else if (paused) bridge_led_set(BRIDGE_LED_PAUSED);
        else if (now - (radio_ready_us/1000) > 60000) bridge_led_set(BRIDGE_LED_AMBER);
        else bridge_led_set(BRIDGE_LED_SEARCHING);
        vTaskDelay(pdMS_TO_TICKS(5));
    }
}
static void identify(bool enabled) { bridge_led_identify(false,enabled); }
static void status(void) { lock_state(); bool ready=peer_ready; uint64_t local_epoch=peer_epoch; unlock_state(); char out[128]; int n=snprintf(out,sizeof(out),"role=A fixture=%d radio=%d peer=%d epoch=%llx boot_us=%u\r\n",(int)!!(0
#ifdef BRIDGE_NONDEPLOYABLE_FIXTURE
+1
#endif
),bridge_radio_ready(),ready,(unsigned long long)local_epoch,(unsigned)boot_us); uart_write_bytes(UART_NUM_0,out,n); }
void app_main(void) {
    bridge_led_init(BRIDGE_RGB_GPIO); bridge_led_set(BRIDGE_LED_BOOT); cdc_rx_queue=xQueueCreate(8,sizeof(cdc_chunk_t)); cdc_write_lock=xSemaphoreCreateMutex(); state_lock=xSemaphoreCreateMutex(); configASSERT(cdc_rx_queue && cdc_write_lock && state_lock);
    tinyusb_config_t usb=TINYUSB_DEFAULT_CONFIG(usb_event); ESP_ERROR_CHECK(tinyusb_driver_install(&usb)); tinyusb_config_cdcacm_t acm={.cdc_port=TINYUSB_CDC_ACM_0,.callback_rx=cdc_rx_callback}; ESP_ERROR_CHECK(tinyusb_cdcacm_init(&acm));
    bool radio=bridge_radio_init(handle_from_radio,NULL,&radio_ready_us);
    boot_us=(uint32_t)esp_timer_get_time();
    ESP_LOGW(TAG,"role=A fixture=%d radio=%d boot_us=%u",(int)!!(0
#ifdef BRIDGE_NONDEPLOYABLE_FIXTURE
+1
#endif
),radio,(unsigned)boot_us);
    bridge_maintenance_start(identify,status); xTaskCreate(cdc_task,"bridge_cdc",4096,NULL,9,NULL); xTaskCreate(control_task,"bridge_ctl",4096,NULL,8,NULL);
}
