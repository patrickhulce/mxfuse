#ifndef MXFUSE_UUID_STUB_H
#define MXFUSE_UUID_STUB_H

#ifdef __cplusplus
extern "C" {
#endif

typedef unsigned char uuid_t[16];

void uuid_generate(uuid_t out);

#ifdef __cplusplus
}
#endif

#endif
