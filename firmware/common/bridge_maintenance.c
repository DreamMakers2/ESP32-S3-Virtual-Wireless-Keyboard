#include "bridge_maintenance.h"
#include "driver/uart.h"
#include "esp_err.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include <string.h>

static bridge_maintenance_identify_fn identify_callback;
static bridge_maintenance_status_fn status_callback;
static void maintenance_task(void *ignored) {
    (void)ignored; char line[32]; size_t used=0; uint8_t c;
    for (;;) {
        int got=uart_read_bytes(UART_NUM_0,&c,1,pdMS_TO_TICKS(100));
        if (got != 1) continue;
        if (c=='\r' || c=='\n') { if (!used) continue; line[used]=0;
            if (!strcmp(line,"identify on")) { identify_callback(true); uart_write_bytes(UART_NUM_0,"identify on\r\n",13); }
            else if (!strcmp(line,"identify off")) { identify_callback(false); uart_write_bytes(UART_NUM_0,"identify off\r\n",14); }
            else if (!strcmp(line,"status")) status_callback();
            else uart_write_bytes(UART_NUM_0,"commands: identify on|off, status\r\n",34);
            used=0;
        } else if (used+1<sizeof(line)) line[used++]=(char)c; else used=0;
    }
}
void bridge_maintenance_start(bridge_maintenance_identify_fn identify, bridge_maintenance_status_fn status) {
    identify_callback=identify; status_callback=status;
    uart_config_t cfg={.baud_rate=115200,.data_bits=UART_DATA_8_BITS,.parity=UART_PARITY_DISABLE,.stop_bits=UART_STOP_BITS_1,.flow_ctrl=UART_HW_FLOWCTRL_DISABLE,.source_clk=UART_SCLK_DEFAULT};
    uart_param_config(UART_NUM_0,&cfg); uart_set_pin(UART_NUM_0,UART_PIN_NO_CHANGE,UART_PIN_NO_CHANGE,UART_PIN_NO_CHANGE,UART_PIN_NO_CHANGE);
    esp_err_t installed=uart_driver_install(UART_NUM_0,256,0,0,NULL,0);
    if (installed != ESP_OK && installed != ESP_ERR_INVALID_STATE) return;
    xTaskCreate(maintenance_task,"bridge_maint",3072,NULL,1,NULL);
}
