idf_component_register(
    SRCS "bridge_protocol.c" "bridge_session.c" "bridge_led.c" "bridge_radio.c" "bridge_maintenance.c"
    INCLUDE_DIRS "include"
    REQUIRES esp_wifi esp_event esp_netif esp_timer nvs_flash freertos driver led_strip esp_psram spi_flash
)
target_include_directories(${COMPONENT_LIB} PRIVATE "${CMAKE_CURRENT_LIST_DIR}/../config/private")
# bridge_config.h selects the checked-in fixture header for nondeployable builds.
target_include_directories(${COMPONENT_LIB} PUBLIC "${CMAKE_CURRENT_LIST_DIR}/../config")
