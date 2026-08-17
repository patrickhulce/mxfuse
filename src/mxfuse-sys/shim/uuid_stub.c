#include <stdint.h>
#include <string.h>

#if defined(__linux__)
#include <sys/random.h>
#endif

#if defined(_WIN32)
#include <stdlib.h>
#endif

void uuid_generate(unsigned char out[16])
{
#if defined(__linux__)
    if (getrandom(out, 16, 0) != 16) {
        memset(out, 0, 16);
        return;
    }
#elif defined(_WIN32)
    for (int i = 0; i < 16; i++) {
        out[i] = (unsigned char)(rand() & 0xff);
    }
#else
    memset(out, 0, 16);
#endif
    out[6] = (unsigned char)((out[6] & 0x0f) | 0x40);
    out[8] = (unsigned char)((out[8] & 0x3f) | 0x80);
}
