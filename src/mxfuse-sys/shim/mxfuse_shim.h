#ifndef MXFUSE_SHIM_H
#define MXFUSE_SHIM_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MXFUSE_OK 0
#define MXFUSE_ERR -1
#define MXFUSE_ERR_NO_FRAME 1

#define MXFUSE_ERROR_LEN 512
#define MXFUSE_UL_LEN 16
#define MXFUSE_KEY_LEN 16

typedef struct MxfuseError {
    char message[MXFUSE_ERROR_LEN];
} MxfuseError;

typedef struct MxfuseByteSourceVtable {
    int32_t (*read)(void *ctx, uint8_t *data, uint32_t count);
    int (*seek)(void *ctx, int64_t offset, int whence);
    int64_t (*tell)(void *ctx);
    int64_t (*size)(void *ctx);
    void (*close)(void *ctx);
} MxfuseByteSourceVtable;

typedef struct MxfuseReader MxfuseReader;

typedef struct MxfuseRational {
    int32_t num;
    int32_t den;
} MxfuseRational;

typedef struct MxfuseClipInfo {
    MxfuseRational edit_rate;
    int64_t duration;
    size_t num_tracks;
} MxfuseClipInfo;

typedef struct MxfuseTrackInfo {
    size_t index;
    int32_t data_def;
    int32_t essence_type;
    uint8_t essence_container_ul[MXFUSE_UL_LEN];
    MxfuseRational edit_rate;
    int64_t duration;
    int enabled;
} MxfuseTrackInfo;

typedef struct MxfuseFrameView {
    uint8_t *data;
    uint32_t size;
    uint8_t element_key[MXFUSE_KEY_LEN];
    int64_t file_position;
    uint8_t kl_size;
    int64_t position;
} MxfuseFrameView;

int mxfuse_reader_open(
    const MxfuseByteSourceVtable *vt,
    void *ctx,
    uint32_t cache_bytes,
    MxfuseReader **out,
    MxfuseError *err
);

void mxfuse_reader_free(MxfuseReader *reader);

int mxfuse_reader_clip_info(MxfuseReader *reader, MxfuseClipInfo *out, MxfuseError *err);
int mxfuse_reader_track_info(
    MxfuseReader *reader,
    size_t index,
    MxfuseTrackInfo *out,
    MxfuseError *err
);
int mxfuse_reader_set_enable(
    MxfuseReader *reader,
    size_t index,
    int enable,
    MxfuseError *err
);
int mxfuse_reader_seek(MxfuseReader *reader, int64_t position, MxfuseError *err);
int mxfuse_reader_read(
    MxfuseReader *reader,
    uint32_t num_samples,
    uint32_t *out_read,
    MxfuseError *err
);
int mxfuse_reader_pop_frame(
    MxfuseReader *reader,
    size_t track,
    MxfuseFrameView *out,
    MxfuseError *err
);
void mxfuse_frame_free(MxfuseFrameView *view);
const char *mxfuse_essence_type_name(int32_t essence_type);

#ifdef __cplusplus
}
#endif

#endif
