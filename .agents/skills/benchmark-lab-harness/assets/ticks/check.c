#include <stdint.h>

int timer_due(uint32_t now, uint32_t start, uint32_t interval);

int main(void) {
    uint32_t start = UINT32_MAX - 2;
    return timer_due(10, 8, 3) || !timer_due(11, 8, 3) || !timer_due(8, 8, 0)
        || timer_due(UINT32_MAX, start, 5) || timer_due(1, start, 5)
        || !timer_due(2, start, 5) || !timer_due(3, start, 5);
}
