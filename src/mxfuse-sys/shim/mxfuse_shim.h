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
#define MXFUSE_UMID_LEN 32
#define MXFUSE_XML_LANG_LEN 32
#define MXFUSE_XML_MIME_LEN 64
#define MXFUSE_XML_NS_LEN 128
#define MXFUSE_NAME_LEN 64
#define MXFUSE_VERSION_LEN 64
#define MXFUSE_PIXEL_LAYOUT_LEN 16

#define MXFUSE_TRACK_HAS_SAMPLING_RATE  (1u << 0)
#define MXFUSE_TRACK_HAS_CHANNEL_COUNT  (1u << 1)
#define MXFUSE_TRACK_HAS_QUANT_BITS     (1u << 2)
#define MXFUSE_TRACK_HAS_STORED_WIDTH   (1u << 3)
#define MXFUSE_TRACK_HAS_STORED_HEIGHT  (1u << 4)
#define MXFUSE_TRACK_HAS_CONTAINER_UL   (1u << 5)
#define MXFUSE_TRACK_HAS_CODING_UL      (1u << 6)
#define MXFUSE_TRACK_HAS_ELEMENT_TYPE   (1u << 7)
#define MXFUSE_TRACK_HAS_ELEMENT_LLEN   (1u << 8)
#define MXFUSE_TRACK_HAS_TEMPORAL_REORDER (1u << 9)
#define MXFUSE_TRACK_HAS_DESCRIPTOR     (1u << 10)
#define MXFUSE_TRACK_HAS_COMPONENT_DEPTH (1u << 11)
#define MXFUSE_TRACK_HAS_SUBSAMPLING    (1u << 12)
#define MXFUSE_TRACK_HAS_FRAME_LAYOUT   (1u << 13)
#define MXFUSE_TRACK_HAS_ASPECT_RATIO   (1u << 14)
#define MXFUSE_TRACK_HAS_VIDEO_LINE_MAP (1u << 15)
#define MXFUSE_TRACK_HAS_PIXEL_LAYOUT   (1u << 16)
#define MXFUSE_TRACK_HAS_COLOR_PRIMARIES (1u << 17)
#define MXFUSE_TRACK_HAS_TRANSFER       (1u << 18)
#define MXFUSE_TRACK_HAS_CODING_EQ      (1u << 19)

#define MXFUSE_DESCRIPTOR_DEFAULT 0
#define MXFUSE_DESCRIPTOR_CDCI 1
#define MXFUSE_DESCRIPTOR_RGBA 2
#define MXFUSE_DESCRIPTOR_WAVE 3
#define MXFUSE_DESCRIPTOR_DATA 4

#define MXFUSE_CLIP_HAS_START_TIMECODE      (1u << 0)
#define MXFUSE_CLIP_HAS_TIMECODE_TRACK      (1u << 1)
#define MXFUSE_CLIP_HAS_SYSTEM_ITEM         (1u << 2)
#define MXFUSE_CLIP_HAS_COMPANY             (1u << 3)
#define MXFUSE_CLIP_HAS_PRODUCT             (1u << 4)
#define MXFUSE_CLIP_HAS_VERSION_STRING      (1u << 5)
#define MXFUSE_CLIP_HAS_PRODUCT_VERSION     (1u << 6)
#define MXFUSE_CLIP_HAS_PRODUCT_UID         (1u << 7)
#define MXFUSE_CLIP_HAS_CREATION_DATE       (1u << 8)
#define MXFUSE_CLIP_HAS_GENERATION_UID      (1u << 9)
#define MXFUSE_CLIP_HAS_MATERIAL_UID        (1u << 10)
#define MXFUSE_CLIP_HAS_FILE_SOURCE_UID     (1u << 11)

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

typedef struct MxfuseTimecode {
    int16_t hour;
    int16_t minute;
    int16_t second;
    int16_t frame;
    int drop_frame;
} MxfuseTimecode;

typedef struct MxfuseClipInfo {
    MxfuseRational edit_rate;
    int64_t duration;
    size_t num_tracks;
    int has_start_timecode;
    MxfuseTimecode start_timecode;
} MxfuseClipInfo;

typedef struct MxfuseTrackInfo {
    size_t index;
    int32_t data_def;
    int32_t essence_type;
    uint8_t essence_container_ul[MXFUSE_UL_LEN];
    uint8_t coding_ul[MXFUSE_UL_LEN];
    int32_t descriptor_kind;
    uint32_t stored_width;
    uint32_t stored_height;
    uint32_t display_width;
    uint32_t display_height;
    uint32_t component_depth;
    uint32_t horiz_subsampling;
    uint32_t vert_subsampling;
    uint8_t frame_layout;
    MxfuseRational aspect_ratio;
    int32_t video_line_map[2];
    uint8_t pixel_layout[MXFUSE_PIXEL_LAYOUT_LEN];
    uint8_t pixel_layout_count;
    uint8_t color_primaries[MXFUSE_UL_LEN];
    uint8_t transfer_characteristic[MXFUSE_UL_LEN];
    uint8_t coding_equations[MXFUSE_UL_LEN];
    uint32_t sampling_rate;
    uint32_t channel_count;
    uint32_t quantization_bits;
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

typedef struct MxfuseXmlView {
    uint8_t *data;
    uint32_t size;
    uint8_t scheme_id[MXFUSE_UL_LEN];
    char language[MXFUSE_XML_LANG_LEN];
    char mime_type[MXFUSE_XML_MIME_LEN];
    char ns[MXFUSE_XML_NS_LEN];
    int is_xml;
} MxfuseXmlView;

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
    uint8_t element_type;
    uint8_t element_llen;
    int temporal_reordering;
    int32_t descriptor_kind;
    uint32_t component_depth;
    uint32_t horiz_subsampling;
    uint32_t vert_subsampling;
    uint8_t frame_layout;
    MxfuseRational aspect_ratio;
    int32_t video_line_map[2];
    uint8_t pixel_layout[MXFUSE_PIXEL_LAYOUT_LEN];
    uint8_t pixel_layout_count;
    uint8_t color_primaries[MXFUSE_UL_LEN];
    uint8_t transfer_characteristic[MXFUSE_UL_LEN];
    uint8_t coding_equations[MXFUSE_UL_LEN];
} MxfuseTrackSpec;

typedef struct MxfuseClipOptions {
    uint32_t flags;
    MxfuseTimecode start_timecode;
    int timecode_track;
    int system_item;
    char company_name[MXFUSE_NAME_LEN];
    char product_name[MXFUSE_NAME_LEN];
    char version_string[MXFUSE_VERSION_LEN];
    uint16_t product_version[5];
    uint8_t product_uid[MXFUSE_UL_LEN];
    int16_t creation_year;
    uint8_t creation_month;
    uint8_t creation_day;
    uint8_t creation_hour;
    uint8_t creation_min;
    uint8_t creation_sec;
    uint8_t creation_qmsec;
    uint8_t generation_uid[MXFUSE_UL_LEN];
    uint8_t material_package_uid[MXFUSE_UMID_LEN];
    uint8_t file_source_package_uid[MXFUSE_UMID_LEN];
} MxfuseClipOptions;

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
int mxfuse_reader_num_xml(MxfuseReader *reader, size_t *out, MxfuseError *err);
int mxfuse_reader_xml(
    MxfuseReader *reader,
    size_t index,
    MxfuseXmlView *out,
    MxfuseError *err
);
void mxfuse_xml_free(MxfuseXmlView *view);
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
int mxfuse_writer_configure(
    MxfuseWriter *writer,
    const MxfuseClipOptions *options,
    MxfuseError *err
);
int mxfuse_writer_create_track(
    MxfuseWriter *writer,
    const MxfuseTrackSpec *spec,
    uint32_t *out_index,
    MxfuseError *err
);
int mxfuse_writer_add_xml(
    MxfuseWriter *writer,
    const uint8_t *data,
    uint32_t size,
    const uint8_t *scheme_id,
    const char *language,
    const char *ns,
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
