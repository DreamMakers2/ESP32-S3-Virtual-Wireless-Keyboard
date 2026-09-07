#include "bridge_config.h"
#include "bridge_led.h"
#include "bridge_maintenance.h"
#include "bridge_protocol.h"
#include "bridge_radio.h"
#include "bridge_session.h"
#include "esp_log.h"
#include "esp_random.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "freertos/task.h"
#include "tinyusb.h"
#include "tinyusb_default_config.h"
#include "class/hid/hid_device.h"
#include "driver/uart.h"
#include <stdio.h>
#include <string.h>

#define TAG "bridge_b"
#define TUSB_DESC_TOTAL_LEN (TUD_CONFIG_DESC_LEN + TUD_HID_DESC_LEN)
#define HOST_HELLO_TIMEOUT_MS 750u

static const uint8_t report_descriptor[] = { TUD_HID_REPORT_DESC_KEYBOARD() };
static const char *strings[] = { (char[]){0x09,0x04}, "DreamMakers2", "Wireless Keyboard Bridge B", "B-HID-001", "Keyboard" };
static const uint8_t configuration_descriptor[] = { TUD_CONFIG_DESCRIPTOR(1,1,0,TUSB_DESC_TOTAL_LEN,TUSB_DESC_CONFIG_ATT_REMOTE_WAKEUP,100), TUD_HID_DESCRIPTOR(0,4,true,sizeof(report_descriptor),0x81,8,1) };

static bridge_session_t session;
static SemaphoreHandle_t state_lock;
static uint64_t epoch;
static uint32_t boot_us, radio_ready_us, outstanding_sequence, last_host_hello_ms;
static uint8_t lock_leds;
static bool hid_suspended, hid_busy, release_pending, ready_pending, debug_enabled, host_hello_seen;
static uint8_t outstanding_report[8];
static bridge_key_state_t canonical_keys;
static bool usb_recovering, reconnect_pending;
static uint32_t disconnected_ms;

static uint32_t now_ms(void) { return (uint32_t)(esp_timer_get_time() / 1000); }
static uint64_t new_epoch(void) { uint64_t value=((uint64_t)esp_random()<<32)|esp_random(); return value ? value : 1; }
static void lock_state(void) { (void)xSemaphoreTake(state_lock, portMAX_DELAY); }
static void unlock_state(void) { (void)xSemaphoreGive(state_lock); }
static bool usb_ready(void) { return tud_mounted() && !hid_suspended && !usb_recovering; }

uint8_t const *tud_hid_descriptor_report_cb(uint8_t instance) { (void)instance; return report_descriptor; }
uint16_t tud_hid_get_report_cb(uint8_t instance, uint8_t report_id, hid_report_type_t type, uint8_t *buffer, uint16_t length) {
    (void)instance; (void)report_id; (void)type;
    if (length < sizeof(outstanding_report)) return 0;
    lock_state(); memcpy(buffer, outstanding_report, sizeof(outstanding_report)); unlock_state();
    return sizeof(outstanding_report);
}

static bool submit_report_locked(const uint8_t report[8], uint32_t sequence) {
#ifdef BRIDGE_NONDEPLOYABLE_FIXTURE
    (void)report; (void)sequence; return false;
#else
    if (!usb_ready() || hid_busy || !tud_hid_ready()) return false;
    /* Publish completion ownership before handing the report to TinyUSB. */
    hid_busy=true; outstanding_sequence=sequence; memcpy(outstanding_report, report, 8);
    if (!tud_hid_n_report(0,0,report,8)) { hid_busy=false; outstanding_sequence=0; return false; }
    return true;
#endif
}

static bool pump_release_locked(void) {
    uint8_t release[8]={0};
    if (!release_pending || hid_busy) return false;
    if (!submit_report_locked(release, 0)) return false;
    release_pending=false;
    return true;
}

static void send_status(void) {
    bridge_status_t status; uint64_t local_epoch;
    lock_state();
    status=(bridge_status_t){.ready=(uint8_t)(bridge_radio_ready() && usb_ready() && session.state==BRIDGE_SESSION_IDLE),.leds=lock_leds,.error=0,.a_boot_us=0,.b_boot_us=boot_us,.radio_ready_us=radio_ready_us};
    local_epoch=epoch;
    unlock_state();
    bridge_packet_t p; bridge_packet_init(&p,BRIDGE_STATUS,0,local_epoch,0); p.payload_len=16; bridge_status_encode(&status,p.payload); (void)bridge_radio_send(&p);
}
static void send_ack(uint64_t capture, uint64_t capture_epoch, uint32_t sequence, bool debug, uint64_t complete_us) {
    bridge_packet_t p; bridge_packet_init(&p,BRIDGE_ACK,capture,capture_epoch,sequence);
    if (debug && complete_us) { p.payload_len=8; for (unsigned i=0;i<8;++i) p.payload[i]=(uint8_t)(complete_us>>(i*8)); }
    (void)bridge_radio_send(&p);
}
static void send_ready(uint64_t capture, uint64_t capture_epoch) { bridge_packet_t p; bridge_packet_init(&p,BRIDGE_READY,capture,capture_epoch,0); (void)bridge_radio_send(&p); }

static void invalidate_locked(void) {
    epoch=new_epoch();
    bridge_session_reset(&session,epoch,now_ms());
    ready_pending=false;
    memset(&canonical_keys,0,sizeof(canonical_keys));
    memset(outstanding_report,0,sizeof(outstanding_report));
    release_pending=true;
    if (!hid_busy) (void)pump_release_locked();
}

void tud_hid_set_report_cb(uint8_t instance,uint8_t report_id,hid_report_type_t type,uint8_t const *buffer,uint16_t length) {
    (void)instance; (void)report_id;
    if (type != HID_REPORT_TYPE_OUTPUT || !length) return;
    lock_state();
    lock_leds=buffer[0];
    uint64_t capture=session.session, capture_epoch=epoch;
    unlock_state();
    bridge_packet_t p; bridge_packet_init(&p,BRIDGE_LEDS,capture,capture_epoch,0); p.payload_len=1; p.payload[0]=buffer[0]; (void)bridge_radio_send(&p);
}
void tud_suspend_cb(bool remote_wakeup_en) {
    (void)remote_wakeup_en;
    lock_state();
    // A soft disconnect forces a new bus reset before any retained IN transfer
    // can be polled. Keep ownership blocked until configuration is mounted again.
    hid_suspended=true; usb_recovering=true;
    invalidate_locked();
    disconnected_ms=now_ms(); reconnect_pending=true;
    unlock_state();
    (void)tud_disconnect();
}
void tud_resume_cb(void) { /* Recovery requires fresh USB enumeration. */ }
static void usb_event(tinyusb_event_t *event, void *arg) {
    (void)arg;
    lock_state();
    if (event->id==TINYUSB_EVENT_DETACHED) {
        hid_suspended=true; usb_recovering=true;
        hid_busy=false; outstanding_sequence=0;
        invalidate_locked();
    } else if (event->id==TINYUSB_EVENT_ATTACHED) {
        hid_busy=false; outstanding_sequence=0;
        hid_suspended=false; usb_recovering=false; reconnect_pending=false;
        invalidate_locked();
    }
    unlock_state();
}

void tud_hid_report_complete_cb(uint8_t instance,uint8_t const *report,uint16_t length) {
    (void)instance; (void)report; (void)length;
    uint32_t finished_sequence; uint64_t capture=0, capture_epoch=0; bool ack=false, debug=false, ready=false;
    uint64_t completed=(uint64_t)esp_timer_get_time();
    lock_state();
    if (usb_recovering || !hid_busy) { unlock_state(); return; }
    finished_sequence=outstanding_sequence; hid_busy=false; outstanding_sequence=0;
    if (finished_sequence && session.state==BRIDGE_SESSION_ACTIVE) {
        session.last_complete_sequence=finished_sequence; session.next_rx_sequence=finished_sequence+1; session.last_progress_ms=now_ms();
        capture=session.session; capture_epoch=epoch; debug=debug_enabled; ack=true;
    }
    (void)pump_release_locked();
    if (!finished_sequence && !hid_busy && !release_pending && ready_pending && session.state==BRIDGE_SESSION_WAITING_READY) {
        ready_pending=false; session.state=BRIDGE_SESSION_ACTIVE; session.next_rx_sequence=1; session.last_liveness_ms=now_ms(); session.last_progress_ms=now_ms();
        capture=session.session; capture_epoch=epoch; ready=true;
    }
    unlock_state();
    if (ack) send_ack(capture,capture_epoch,finished_sequence,debug,completed);
    if (ready) send_ready(capture,capture_epoch);
}

static void build_boot_report(const bridge_key_state_t *keys,uint8_t report[8]) {
    memset(report,0,8); report[0]=keys->modifiers; unsigned n=0;
    for (unsigned usage=4;usage<256;++usage) if (keys->usages[usage>>3] & (1u<<(usage&7))) { if (n==6) { memset(report+2,1,6); return; } report[2+n++]=(uint8_t)usage; }
}

static void handle_radio(const bridge_packet_t *p, void *context) {
    (void)context; uint32_t now=now_ms(); bool status=false, ready=false, ack=false, reply=false;
    uint64_t capture=0, capture_epoch=0; uint32_t ack_sequence=0; bool debug=false; bridge_packet_t sync_reply;
    lock_state();
    if (p->type==BRIDGE_HELLO && p->payload_len==0) { host_hello_seen=true; last_host_hello_ms=now; status=true; }
    else if (p->type==BRIDGE_DEBUG && p->payload_len==1 && p->payload[0]<=1) { debug_enabled=p->payload[0]; }
    else if (p->type==BRIDGE_START && p->payload_len==0 && p->session && p->epoch==epoch && usb_ready()) {
        if (session.state==BRIDGE_SESSION_IDLE && bridge_session_start(&session,p->session,p->epoch,now)) {
            release_pending=true; ready_pending=true; (void)pump_release_locked();
        } else if (session.session==p->session && session.state==BRIDGE_SESSION_ACTIVE) { capture=session.session; capture_epoch=epoch; ready=true; }
    } else if (p->type==BRIDGE_STOP && p->payload_len==0 && session.state!=BRIDGE_SESSION_IDLE && p->session==session.session && p->epoch==epoch) { invalidate_locked(); status=true; }
    else if (p->type==BRIDGE_HEARTBEAT && bridge_session_liveness_valid(&session,p)) { session.last_liveness_ms=now; }
    else if (p->type==BRIDGE_SYNC && debug_enabled && p->payload_len==8 && p->session==session.session && p->epoch==epoch) {
        bridge_packet_init(&sync_reply,BRIDGE_SYNC_REPLY,p->session,epoch,0); sync_reply.payload_len=24; memcpy(sync_reply.payload,p->payload,8);
        uint64_t receive=esp_timer_get_time(); for(unsigned i=0;i<8;++i) sync_reply.payload[8+i]=(uint8_t)(receive>>(i*8));
        uint64_t send=esp_timer_get_time(); for(unsigned i=0;i<8;++i) sync_reply.payload[16+i]=(uint8_t)(send>>(i*8)); reply=true;
    } else if (p->type==BRIDGE_STATE && p->session==session.session && p->epoch==epoch && p->payload_len==BRIDGE_STATE_BYTES && session.state==BRIDGE_SESSION_ACTIVE) {
        if (p->sequence != session.next_rx_sequence || hid_busy) { capture=session.session; capture_epoch=epoch; ack_sequence=session.last_complete_sequence; debug=debug_enabled; ack=true; }
        else {
            canonical_keys.modifiers=p->payload[0]; memcpy(canonical_keys.usages,p->payload+1,32); uint8_t report[8]; build_boot_report(&canonical_keys,report);
            if (submit_report_locked(report,p->sequence)) { session.last_liveness_ms=now; session.last_progress_ms=now; bridge_led_activity(); }
            else { invalidate_locked(); status=true; bridge_led_set(BRIDGE_LED_ERROR); }
        }
    }
    unlock_state();
    if (status) send_status();
    if (ready) send_ready(capture,capture_epoch);
    if (ack) send_ack(capture,capture_epoch,ack_sequence,debug,0);
    if (reply) (void)bridge_radio_send(&sync_reply);
}

static void watchdog_task(void *ignored) {
    (void)ignored;
    for (;;) {
        uint32_t now=now_ms(); bridge_session_state_t state; bool error=false, reconnect=false, paused=false;
        lock_state();
        if (reconnect_pending && (uint32_t)(now-disconnected_ms)>=100) {
            reconnect_pending=false; reconnect=true;
        }
        (void)pump_release_locked();
        if (!bridge_radio_ready() || !usb_ready()) { if (session.state!=BRIDGE_SESSION_IDLE) invalidate_locked(); state=BRIDGE_SESSION_IDLE; }
        else if (bridge_session_expired(&session,now,hid_busy && outstanding_sequence)) { invalidate_locked(); state=BRIDGE_SESSION_IDLE; error=true; }
        else state=session.state;
        paused=state==BRIDGE_SESSION_IDLE && host_hello_seen && now-last_host_hello_ms<=HOST_HELLO_TIMEOUT_MS;
        unlock_state();
        if (reconnect) (void)tud_connect();
        if (error) bridge_led_set(BRIDGE_LED_ERROR);
        else if (!bridge_radio_ready() || !usb_ready()) bridge_led_set(BRIDGE_LED_SEARCHING);
        else if (state!=BRIDGE_SESSION_IDLE) bridge_led_set(BRIDGE_LED_CONNECTED);
        else if (paused) bridge_led_set(BRIDGE_LED_PAUSED);
        else if (now-(boot_us/1000)>60000) bridge_led_set(BRIDGE_LED_AMBER);
        else bridge_led_set(BRIDGE_LED_SEARCHING);
        vTaskDelay(pdMS_TO_TICKS(5));
    }
}
static void identify(bool enabled) { bridge_led_identify(true,enabled); }
static void status(void) {
    uint64_t local_epoch; bool hid; lock_state(); local_epoch=epoch; hid=usb_ready(); unlock_state();
    char out[132]; int n=snprintf(out,sizeof(out),"role=B fixture=%d hid=%d radio=%d epoch=%llx boot_us=%u\r\n",
#ifdef BRIDGE_NONDEPLOYABLE_FIXTURE
        1,
#else
        0,
#endif
        hid,bridge_radio_ready(),(unsigned long long)local_epoch,(unsigned)boot_us); uart_write_bytes(UART_NUM_0,out,n);
}
void app_main(void) {
    epoch=new_epoch(); bridge_session_reset(&session,epoch,now_ms()); state_lock=xSemaphoreCreateMutex(); configASSERT(state_lock);
    bridge_led_init(BRIDGE_RGB_GPIO); bridge_led_set(BRIDGE_LED_BOOT);
    tinyusb_config_t usb=TINYUSB_DEFAULT_CONFIG(); usb.event_cb=usb_event; usb.descriptor.device=NULL; usb.descriptor.full_speed_config=configuration_descriptor; usb.descriptor.string=strings; usb.descriptor.string_count=sizeof(strings)/sizeof(strings[0]); ESP_ERROR_CHECK(tinyusb_driver_install(&usb));
    bool radio=bridge_radio_init(handle_radio,NULL,&radio_ready_us);
    boot_us=(uint32_t)esp_timer_get_time();
    ESP_LOGW(TAG,"role=B fixture=%d radio=%d boot_us=%u",
#ifdef BRIDGE_NONDEPLOYABLE_FIXTURE
        1,
#else
        0,
#endif
        radio,(unsigned)boot_us);
    bridge_maintenance_start(identify,status); xTaskCreate(watchdog_task,"bridge_watchdog",4096,NULL,8,NULL);
}
