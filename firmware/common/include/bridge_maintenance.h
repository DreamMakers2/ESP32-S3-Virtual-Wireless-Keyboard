#pragma once
#include <stdbool.h>
typedef void (*bridge_maintenance_identify_fn)(bool enabled);
typedef void (*bridge_maintenance_status_fn)(void);
void bridge_maintenance_start(bridge_maintenance_identify_fn identify, bridge_maintenance_status_fn status);
