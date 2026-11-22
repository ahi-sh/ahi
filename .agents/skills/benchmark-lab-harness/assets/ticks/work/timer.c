#include <stdint.h>

/* True when at least interval ticks have elapsed since start (elapsed time is less than a full counter period). */
int timer_due(uint32_t now, uint32_t start, uint32_t interval) {
    return now >= start + interval;
}
