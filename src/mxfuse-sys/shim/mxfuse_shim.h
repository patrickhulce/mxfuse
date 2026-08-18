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

#define MXFUSE_TRACK_HAS_SAMPLING_RATE  (1u << 0)
#define MXFUSE_TRACK_HAS_CHANNEL_COUNT  (1u << 1)
#define MXFUSE_TRACK_HAS_QUANT_BITS     (1u << 2)
#define MXFUSE_TRACK_HAS_STORED_WIDTH   (1u << 3)
#define MXFUSE_TRACK_HAS_STORED_HEIGHT  (1u << 4)
#define MXFUSE_TRACK_HAS_CONTAINER_UL   (1u << 5)
#define MXFUSE_TRACK_HAS_CODING_UL      (1u << 6)

typedef struct MxfuseError {
    char message[MXFUSE_ERROR_LEN];
} MxfuseError;

typedef struct MxfuseByteSourceVtable {
    int32_t (*read)(void *ctx, uint8_t *data, uint32_t count);
    int32_t (*write)(void *ctx, const uint8_t *data, uint32_t count);
    int (*seek)(void *ctx, int64_t offset, int whence);
    int64_t (*tell)(void *ctx);
    int64_t (*size)(void *ctx);
    int (*is_seekable)(void *ctx);
    void (*close)(void *ctx);
} MxfuseByteSourceVtable;

typedef struct MxfuseReader MxfuseReader;
typedef struct MxfuseWriter MxfuseWriter;

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

typedef struct MxfuseTrackSpec {
    int32_t essence_type;
    uint32_t flags;
    MxfuseRational sampling_rate;
    uint32_t channel_count;
    uint32_t quantization_bits;
    uint32_t stored_width;
    uint32_t stored_height;
    uint8_t essence_container_ul[MXFUSE_UL_LEN];
    uint8_t picture_coding_ul[MXFUSE_UL_LEN];
} MxfuseTrackSpec;

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

int mxfuse_writer_open(
    const MxfuseByteSourceVtable *vt,
    void *ctx,
    int flavour,
    MxfuseRational edit_rate,
    int64_t duration,
    MxfuseWriter **out,
    MxfuseError *err
);
int mxfuse_writer_create_track(
    MxfuseWriter *writer,
    const MxfuseTrackSpec *spec,
    uint32_t *out_index,
    MxfuseError *err
);
int mxfuse_writer_prepare(MxfuseWriter *writer, MxfuseError *err);
int mxfuse_writer_write_samples(
    MxfuseWriter *writer,
    uint32_t track_index,
    const uint8_t *data,
    uint32_t size,
    uint32_t num_samples,
    MxfuseError *err
);
int mxfuse_writer_complete(MxfuseWriter *writer, MxfuseError *err);
void mxfuse_writer_free(MxfuseWriter *writer);

#ifdef __cplusplus
}
#endif

#endif
