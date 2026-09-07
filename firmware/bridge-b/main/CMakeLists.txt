idf_component_register(SRCS "bridge_b.c" INCLUDE_DIRS "." REQUIRES common esp_tinyusb)
target_include_directories(${COMPONENT_LIB} PRIVATE "${CMAKE_CURRENT_LIST_DIR}/../../config/private")
