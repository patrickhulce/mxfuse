#if defined(_WIN32)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef _CRT_SECURE_NO_WARNINGS
#define _CRT_SECURE_NO_WARNINGS
#endif
#endif

#include "mxfuse_shim.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <new>
#include <string>

#include <bmx/BMXException.h>
#include <bmx/EssenceType.h>
#include <bmx/clip_writer/ClipWriter.h>
#include <bmx/clip_writer/ClipWriterTrack.h>
#include <bmx/frame/Frame.h>
#include <bmx/frame/FrameBuffer.h>
#include <bmx/mxf_helper/MXFDescriptorHelper.h>
#include <bmx/mxf_helper/OpaqueMXFDescriptorHelper.h>
#include <bmx/mxf_op1a/OP1AFile.h>
#include <bmx/mxf_reader/MXFFileReader.h>
#include <bmx/mxf_reader/MXFTrackInfo.h>
#include <bmx/mxf_reader/MXFTrackReader.h>
#include <libMXF++/File.h>
#include <libMXF++/MXFException.h>
#include <mxf/mxf_cache_file.h>
#include <mxf/mxf_file.h>
#include <mxf/mxf_types.h>

namespace {

void set_error(MxfuseError *err, const char *message)
{
    if (!err) {
        return;
    }
    if (!message) {
        err->message[0] = '\0';
        return;
    }
    std::snprintf(err->message, MXFUSE_ERROR_LEN, "%s", message);
}

} // namespace

struct MXFFileSysData {
    MxfuseByteSourceVtable vt;
    void *ctx;
    int eof;
};

namespace {

void file_close(MXFFileSysData *sys)
{
    if (sys && sys->vt.close && sys->ctx) {
        sys->vt.close(sys->ctx);
        sys->ctx = 0;
    }
}

uint32_t file_read(MXFFileSysData *sys, uint8_t *data, uint32_t count)
{
    if (!sys || !sys->vt.read || !sys->ctx) {
        return 0;
    }
    int32_t n = sys->vt.read(sys->ctx, data, count);
    if (n < 0) {
        sys->eof = 1;
        return 0;
    }
    if (static_cast<uint32_t>(n) < count) {
        sys->eof = 1;
    }
    return static_cast<uint32_t>(n);
}

uint32_t file_write(MXFFileSysData *sys, const uint8_t *data, uint32_t count)
{
    if (!sys || !sys->vt.write || !sys->ctx || !data) {
        return 0;
    }
    int32_t n = sys->vt.write(sys->ctx, data, count);
    if (n < 0) {
        return 0;
    }
    return static_cast<uint32_t>(n);
}

int file_get_char(MXFFileSysData *sys)
{
    uint8_t byte = 0;
    if (file_read(sys, &byte, 1) != 1) {
        return EOF;
    }
    return static_cast<int>(byte);
}

int file_put_char(MXFFileSysData *sys, int value)
{
    uint8_t byte = static_cast<uint8_t>(value);
    if (file_write(sys, &byte, 1) != 1) {
        return EOF;
    }
    return value;
}

int file_eof(MXFFileSysData *sys)
{
    return sys ? sys->eof : 1;
}

int file_seek(MXFFileSysData *sys, int64_t offset, int whence)
{
    if (!sys || !sys->vt.seek || !sys->ctx) {
        return 0;
    }
    int ok = sys->vt.seek(sys->ctx, offset, whence);
    if (ok) {
        sys->eof = 0;
    }
    return ok;
}

int64_t file_tell(MXFFileSysData *sys)
{
    if (!sys || !sys->vt.tell || !sys->ctx) {
        return -1;
    }
    return sys->vt.tell(sys->ctx);
}

int file_is_seekable(MXFFileSysData *sys)
{
    if (!sys || !sys->vt.is_seekable || !sys->ctx) {
        return 1;
    }
    return sys->vt.is_seekable(sys->ctx);
}

int64_t file_size(MXFFileSysData *sys)
{
    if (!sys || !sys->vt.size || !sys->ctx) {
        return -1;
    }
    return sys->vt.size(sys->ctx);
}

void file_free_sys_data(MXFFileSysData *sys)
{
    std::free(sys);
}

MXFFile *create_mxf_file(const MxfuseByteSourceVtable *vt, void *ctx)
{
    MXFFile *mxf_file = static_cast<MXFFile *>(std::malloc(sizeof(MXFFile)));
    if (!mxf_file) {
        return 0;
    }
    std::memset(mxf_file, 0, sizeof(MXFFile));

    MXFFileSysData *sys = static_cast<MXFFileSysData *>(std::malloc(sizeof(MXFFileSysData)));
    if (!sys) {
        std::free(mxf_file);
        return 0;
    }
    sys->vt = *vt;
    sys->ctx = ctx;
    sys->eof = 0;

    mxf_file->close = file_close;
    mxf_file->read = file_read;
    mxf_file->write = file_write;
    mxf_file->get_char = file_get_char;
    mxf_file->put_char = file_put_char;
    mxf_file->eof = file_eof;
    mxf_file->seek = file_seek;
    mxf_file->tell = file_tell;
    mxf_file->is_seekable = file_is_seekable;
    mxf_file->size = file_size;
    mxf_file->free_sys_data = file_free_sys_data;
    mxf_file->sysData = sys;
    return mxf_file;
}

void copy_ul(uint8_t dest[MXFUSE_UL_LEN], const mxfUL &src)
{
    std::memcpy(dest, &src, MXFUSE_UL_LEN);
}

void copy_key(uint8_t dest[MXFUSE_KEY_LEN], const mxfKey &src)
{
    std::memcpy(dest, &src, MXFUSE_KEY_LEN);
}

} // namespace

struct MxfuseReader {
    bmx::MXFFileReader *reader;
};

int mxfuse_reader_open(
    const MxfuseByteSourceVtable *vt,
    void *ctx,
    uint32_t cache_bytes,
    MxfuseReader **out,
    MxfuseError *err
)
{
    if (!vt || !ctx || !out) {
        set_error(err, "invalid arguments to mxfuse_reader_open");
        return MXFUSE_ERR;
    }
    *out = 0;

    MXFFile *mxf_file = create_mxf_file(vt, ctx);
    if (!mxf_file) {
        set_error(err, "failed to allocate MXFFile");
        if (vt->close) {
            vt->close(ctx);
        }
        return MXFUSE_ERR;
    }

    if (cache_bytes > 0) {
        uint32_t page = 64 * 1024;
        if (cache_bytes < page) {
            page = cache_bytes;
        }
        MXFCacheFile *cache = 0;
        if (!mxf_cache_file_open(mxf_file, page, cache_bytes, &cache) || !cache) {
            mxf_file_close(&mxf_file);
            set_error(err, "mxf_cache_file_open failed");
            return MXFUSE_ERR;
        }
        mxf_file = mxf_cache_file_get_file(cache);
    }

    mxfpp::File *file = 0;
    bmx::MXFFileReader *reader = 0;
    try {
        file = new mxfpp::File(mxf_file);
        reader = new bmx::MXFFileReader();
        bmx::MXFFileReader::OpenResult result = reader->Open(file, "mxfuse-source");
        if (result != bmx::MXFFileReader::MXF_RESULT_SUCCESS) {
            delete file;
            delete reader;
            std::string message = bmx::MXFFileReader::ResultToString(result);
            set_error(err, message.c_str());
            return MXFUSE_ERR;
        }
        try {
            reader->SetReadLimits();
        } catch (const bmx::BMXException &) {
            // Incomplete files can still be read without default limits.
        }
        MxfuseReader *handle = new MxfuseReader();
        handle->reader = reader;
        *out = handle;
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        delete reader;
        delete file;
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        delete reader;
        delete file;
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (const std::exception &e) {
        delete reader;
        delete file;
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (...) {
        delete reader;
        delete file;
        set_error(err, "unknown error opening MXF reader");
        return MXFUSE_ERR;
    }
}

void mxfuse_reader_free(MxfuseReader *reader)
{
    if (!reader) {
        return;
    }
    delete reader->reader;
    delete reader;
}

int mxfuse_reader_clip_info(MxfuseReader *reader, MxfuseClipInfo *out, MxfuseError *err)
{
    if (!reader || !reader->reader || !out) {
        set_error(err, "invalid arguments to mxfuse_reader_clip_info");
        return MXFUSE_ERR;
    }
    try {
        mxfRational rate = reader->reader->GetEditRate();
        out->edit_rate.num = rate.numerator;
        out->edit_rate.den = rate.denominator;
        out->duration = reader->reader->GetDuration();
        out->num_tracks = reader->reader->GetNumTrackReaders();
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error reading clip info");
        return MXFUSE_ERR;
    }
}

int mxfuse_reader_track_info(
    MxfuseReader *reader,
    size_t index,
    MxfuseTrackInfo *out,
    MxfuseError *err
)
{
    if (!reader || !reader->reader || !out) {
        set_error(err, "invalid arguments to mxfuse_reader_track_info");
        return MXFUSE_ERR;
    }
    try {
        if (index >= reader->reader->GetNumTrackReaders()) {
            set_error(err, "track index out of range");
            return MXFUSE_ERR;
        }
        bmx::MXFTrackReader *track = reader->reader->GetTrackReader(index);
        const bmx::MXFTrackInfo *info = track->GetTrackInfo();
        out->index = index;
        out->data_def = static_cast<int32_t>(info->data_def);
        out->essence_type = static_cast<int32_t>(info->essence_type);
        copy_ul(out->essence_container_ul, info->essence_container_label);
        out->edit_rate.num = info->edit_rate.numerator;
        out->edit_rate.den = info->edit_rate.denominator;
        out->duration = info->duration;
        out->enabled = track->IsEnabled() ? 1 : 0;
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error reading track info");
        return MXFUSE_ERR;
    }
}

int mxfuse_reader_set_enable(
    MxfuseReader *reader,
    size_t index,
    int enable,
    MxfuseError *err
)
{
    if (!reader || !reader->reader) {
        set_error(err, "invalid arguments to mxfuse_reader_set_enable");
        return MXFUSE_ERR;
    }
    try {
        if (index >= reader->reader->GetNumTrackReaders()) {
            set_error(err, "track index out of range");
            return MXFUSE_ERR;
        }
        reader->reader->GetTrackReader(index)->SetEnable(enable != 0);
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error setting track enable");
        return MXFUSE_ERR;
    }
}

int mxfuse_reader_seek(MxfuseReader *reader, int64_t position, MxfuseError *err)
{
    if (!reader || !reader->reader) {
        set_error(err, "invalid arguments to mxfuse_reader_seek");
        return MXFUSE_ERR;
    }
    try {
        reader->reader->Seek(position);
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error seeking");
        return MXFUSE_ERR;
    }
}

int mxfuse_reader_read(
    MxfuseReader *reader,
    uint32_t num_samples,
    uint32_t *out_read,
    MxfuseError *err
)
{
    if (!reader || !reader->reader || !out_read) {
        set_error(err, "invalid arguments to mxfuse_reader_read");
        return MXFUSE_ERR;
    }
    try {
        uint32_t n = reader->reader->Read(num_samples);
        if (n == 0 && reader->reader->ReadError()) {
            set_error(err, reader->reader->ReadErrorMessage().c_str());
            return MXFUSE_ERR;
        }
        *out_read = n;
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error reading samples");
        return MXFUSE_ERR;
    }
}

int mxfuse_reader_pop_frame(
    MxfuseReader *reader,
    size_t track,
    MxfuseFrameView *out,
    MxfuseError *err
)
{
    if (!reader || !reader->reader || !out) {
        set_error(err, "invalid arguments to mxfuse_reader_pop_frame");
        return MXFUSE_ERR;
    }
    std::memset(out, 0, sizeof(*out));
    try {
        if (track >= reader->reader->GetNumTrackReaders()) {
            set_error(err, "track index out of range");
            return MXFUSE_ERR;
        }
        bmx::MXFTrackReader *track_reader = reader->reader->GetTrackReader(track);
        bmx::FrameBuffer *buffer = track_reader->GetFrameBuffer();
        bmx::Frame *frame = buffer ? buffer->GetLastFrame(true) : 0;
        if (!frame) {
            return MXFUSE_ERR_NO_FRAME;
        }
        if (frame->IsEmpty() || frame->GetSize() == 0 || !frame->GetBytes()) {
            delete frame;
            return MXFUSE_ERR_NO_FRAME;
        }
        uint32_t size = frame->GetSize();
        uint8_t *copy = static_cast<uint8_t *>(std::malloc(size));
        if (!copy) {
            delete frame;
            set_error(err, "out of memory copying frame");
            return MXFUSE_ERR;
        }
        std::memcpy(copy, frame->GetBytes(), size);
        out->data = copy;
        out->size = size;
        copy_key(out->element_key, frame->element_key);
        out->file_position = frame->file_position;
        out->kl_size = frame->kl_size;
        out->position = frame->position;
        delete frame;
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error popping frame");
        return MXFUSE_ERR;
    }
}

void mxfuse_frame_free(MxfuseFrameView *view)
{
    if (!view) {
        return;
    }
    std::free(view->data);
    view->data = 0;
    view->size = 0;
}

const char *mxfuse_essence_type_name(int32_t essence_type)
{
    return bmx::essence_type_to_enum_string(static_cast<bmx::EssenceType>(essence_type));
}

struct MxfuseWriter {
    bmx::ClipWriter *clip;
    bool completed;
};

static mxfUL ul_from_bytes(const uint8_t src[MXFUSE_UL_LEN])
{
    mxfUL ul;
    std::memcpy(&ul, src, MXFUSE_UL_LEN);
    return ul;
}

int mxfuse_writer_open(
    const MxfuseByteSourceVtable *vt,
    void *ctx,
    int flavour,
    MxfuseRational edit_rate,
    int64_t duration,
    MxfuseWriter **out,
    MxfuseError *err
)
{
    if (!vt || !ctx || !out) {
        set_error(err, "invalid arguments to mxfuse_writer_open");
        return MXFUSE_ERR;
    }
    *out = 0;

    MXFFile *mxf_file = create_mxf_file(vt, ctx);
    if (!mxf_file) {
        set_error(err, "failed to allocate MXFFile");
        if (vt->close) {
            vt->close(ctx);
        }
        return MXFUSE_ERR;
    }

    mxfpp::File *file = 0;
    bmx::ClipWriter *clip = 0;
    try {
        file = new mxfpp::File(mxf_file);
        bmx::Rational frame_rate;
        frame_rate.numerator = edit_rate.num;
        frame_rate.denominator = edit_rate.den;
        clip = bmx::ClipWriter::OpenNewOP1AClip(flavour, file, frame_rate);
        file = 0;
        if (duration >= 0) {
            clip->GetOP1AClip()->SetInputDuration(duration);
        }
        MxfuseWriter *handle = new MxfuseWriter();
        handle->clip = clip;
        handle->completed = false;
        *out = handle;
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        delete clip;
        delete file;
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        delete clip;
        delete file;
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (const std::exception &e) {
        delete clip;
        delete file;
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (...) {
        delete clip;
        delete file;
        set_error(err, "unknown error opening MXF writer");
        return MXFUSE_ERR;
    }
}

int mxfuse_writer_create_track(
    MxfuseWriter *writer,
    const MxfuseTrackSpec *spec,
    uint32_t *out_index,
    MxfuseError *err
)
{
    if (!writer || !writer->clip || !spec || !out_index) {
        set_error(err, "invalid arguments to mxfuse_writer_create_track");
        return MXFUSE_ERR;
    }
    try {
        bmx::EssenceType essence_type = static_cast<bmx::EssenceType>(spec->essence_type);
        bmx::ClipWriterTrack *track = writer->clip->CreateTrack(essence_type);
        if (spec->flags & MXFUSE_TRACK_HAS_SAMPLING_RATE) {
            bmx::Rational rate;
            rate.numerator = spec->sampling_rate.num;
            rate.denominator = spec->sampling_rate.den;
            track->SetSamplingRate(rate);
        }
        if (spec->flags & MXFUSE_TRACK_HAS_CHANNEL_COUNT) {
            track->SetChannelCount(spec->channel_count);
        }
        if (spec->flags & MXFUSE_TRACK_HAS_QUANT_BITS) {
            track->SetQuantizationBits(spec->quantization_bits);
        }
        bmx::OpaqueMXFDescriptorHelper *opaque =
            dynamic_cast<bmx::OpaqueMXFDescriptorHelper*>(track->GetMXFDescriptorHelper());
        if (opaque) {
            if (spec->flags & MXFUSE_TRACK_HAS_CONTAINER_UL) {
                opaque->SetEssenceContainerUL(ul_from_bytes(spec->essence_container_ul));
            }
            if (spec->flags & MXFUSE_TRACK_HAS_CODING_UL) {
                opaque->SetPictureCodingUL(ul_from_bytes(spec->picture_coding_ul));
            }
            if (spec->flags & MXFUSE_TRACK_HAS_STORED_WIDTH) {
                opaque->SetStoredWidth(spec->stored_width);
            }
            if (spec->flags & MXFUSE_TRACK_HAS_STORED_HEIGHT) {
                opaque->SetStoredHeight(spec->stored_height);
            }
            if (spec->flags & MXFUSE_TRACK_HAS_SAMPLING_RATE) {
                mxfRational rate;
                rate.numerator = spec->sampling_rate.num;
                rate.denominator = spec->sampling_rate.den;
                opaque->SetSamplingRate(rate);
            }
            if (spec->flags & MXFUSE_TRACK_HAS_CHANNEL_COUNT) {
                opaque->SetChannelCount(spec->channel_count);
            }
            if (spec->flags & MXFUSE_TRACK_HAS_QUANT_BITS) {
                opaque->SetQuantizationBits(spec->quantization_bits);
            }
        }
        *out_index = writer->clip->GetNumTracks() - 1;
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error creating track");
        return MXFUSE_ERR;
    }
}

int mxfuse_writer_prepare(MxfuseWriter *writer, MxfuseError *err)
{
    if (!writer || !writer->clip) {
        set_error(err, "invalid arguments to mxfuse_writer_prepare");
        return MXFUSE_ERR;
    }
    try {
        writer->clip->PrepareWrite();
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error preparing write");
        return MXFUSE_ERR;
    }
}

int mxfuse_writer_write_samples(
    MxfuseWriter *writer,
    uint32_t track_index,
    const uint8_t *data,
    uint32_t size,
    uint32_t num_samples,
    MxfuseError *err
)
{
    if (!writer || !writer->clip || !data) {
        set_error(err, "invalid arguments to mxfuse_writer_write_samples");
        return MXFUSE_ERR;
    }
    try {
        writer->clip->WriteSamples(track_index, data, size, num_samples);
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error writing samples");
        return MXFUSE_ERR;
    }
}

int mxfuse_writer_complete(MxfuseWriter *writer, MxfuseError *err)
{
    if (!writer || !writer->clip) {
        set_error(err, "invalid arguments to mxfuse_writer_complete");
        return MXFUSE_ERR;
    }
    try {
        writer->clip->CompleteWrite();
        writer->completed = true;
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error completing write");
        return MXFUSE_ERR;
    }
}

void mxfuse_writer_free(MxfuseWriter *writer)
{
    if (!writer) {
        return;
    }
    delete writer->clip;
    delete writer;
}
