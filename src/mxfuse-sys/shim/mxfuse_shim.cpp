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
#include <cstdint>
#include <cstring>
#include <limits>
#include <new>
#include <string>
#include <vector>

#include <bmx/BMXException.h>
#include <bmx/BMXTypes.h>
#include <bmx/EssenceType.h>
#include <bmx/clip_writer/ClipWriter.h>
#include <bmx/clip_writer/ClipWriterTrack.h>
#include <bmx/frame/Frame.h>
#include <bmx/frame/FrameBuffer.h>
#include <bmx/mxf_helper/MXFDescriptorHelper.h>
#include <bmx/mxf_helper/OpaqueMXFDescriptorHelper.h>
#include <bmx/mxf_op1a/OP1AFile.h>
#include <bmx/mxf_op1a/OP1AOpaqueTrack.h>
#include <bmx/mxf_op1a/OP1AXMLTrack.h>
#include <bmx/mxf_reader/MXFFileReader.h>
#include <bmx/mxf_reader/MXFReader.h>
#include <bmx/mxf_reader/MXFTextObject.h>
#include <bmx/mxf_reader/MXFTrackInfo.h>
#include <bmx/mxf_reader/MXFTrackReader.h>
#include <libMXF++/File.h>
#include <libMXF++/MXFException.h>
#include <libMXF++/metadata/GenericPictureEssenceDescriptor.h>
#include <libMXF++/metadata/GenericSoundEssenceDescriptor.h>
#include <libMXF++/metadata/GenericDataEssenceDescriptor.h>
#include <libMXF++/metadata/RGBAEssenceDescriptor.h>
#include <libMXF++/metadata/CDCIEssenceDescriptor.h>
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
        out->has_start_timecode = 0;
        std::memset(&out->start_timecode, 0, sizeof(out->start_timecode));
        if (reader->reader->HaveMaterialTimecode()) {
            bmx::Timecode tc = reader->reader->GetMaterialTimecode(0);
            if (!tc.IsInvalid()) {
                out->has_start_timecode = 1;
                out->start_timecode.hour = tc.GetHour();
                out->start_timecode.minute = tc.GetMin();
                out->start_timecode.second = tc.GetSec();
                out->start_timecode.frame = tc.GetFrame();
                out->start_timecode.drop_frame = tc.IsDropFrame() ? 1 : 0;
            }
        }
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
        std::memset(out, 0, sizeof(*out));
        out->index = index;
        out->data_def = static_cast<int32_t>(info->data_def);
        out->essence_type = static_cast<int32_t>(info->essence_type);
        copy_ul(out->essence_container_ul, info->essence_container_label);
        out->edit_rate.num = info->edit_rate.numerator;
        out->edit_rate.den = info->edit_rate.denominator;
        out->duration = info->duration;
        out->enabled = track->IsEnabled() ? 1 : 0;
        if (const bmx::MXFPictureTrackInfo *picture = dynamic_cast<const bmx::MXFPictureTrackInfo*>(info)) {
            copy_ul(out->coding_ul, picture->picture_essence_coding_label);
            out->stored_width = picture->stored_width;
            out->stored_height = picture->stored_height;
            out->display_width = picture->display_width;
            out->display_height = picture->display_height;
            out->component_depth = picture->component_depth;
            out->horiz_subsampling = picture->horiz_subsampling;
            out->vert_subsampling = picture->vert_subsampling;
            out->frame_layout = picture->frame_layout;
            out->aspect_ratio.num = picture->aspect_ratio.numerator;
            out->aspect_ratio.den = picture->aspect_ratio.denominator;
            out->descriptor_kind = picture->is_cdci ? MXFUSE_DESCRIPTOR_CDCI : MXFUSE_DESCRIPTOR_RGBA;
        }
        if (const bmx::MXFSoundTrackInfo *sound = dynamic_cast<const bmx::MXFSoundTrackInfo*>(info)) {
            out->sampling_rate = static_cast<uint32_t>(sound->sampling_rate.numerator);
            out->channel_count = sound->channel_count;
            out->quantization_bits = sound->bits_per_sample;
            out->descriptor_kind = MXFUSE_DESCRIPTOR_WAVE;
        }
        if (dynamic_cast<const bmx::MXFDataTrackInfo*>(info)) {
            out->descriptor_kind = MXFUSE_DESCRIPTOR_DATA;
        }
        if (mxfpp::FileDescriptor *desc = track->GetFileDescriptor()) {
            if (mxfpp::GenericPictureEssenceDescriptor *pict =
                    dynamic_cast<mxfpp::GenericPictureEssenceDescriptor*>(desc)) {
                if (pict->haveVideoLineMap()) {
                    std::vector<int32_t> map = pict->getVideoLineMap();
                    if (map.size() >= 1) {
                        out->video_line_map[0] = map[0];
                    }
                    if (map.size() >= 2) {
                        out->video_line_map[1] = map[1];
                    }
                }
                if (pict->haveColorPrimaries()) {
                    copy_ul(out->color_primaries, pict->getColorPrimaries());
                }
                if (pict->haveCaptureGamma()) {
                    copy_ul(out->transfer_characteristic, pict->getCaptureGamma());
                }
                if (pict->haveCodingEquations()) {
                    copy_ul(out->coding_equations, pict->getCodingEquations());
                }
                if (pict->havePictureEssenceCoding()) {
                    copy_ul(out->coding_ul, pict->getPictureEssenceCoding());
                }
            }
            if (mxfpp::GenericSoundEssenceDescriptor *sound_desc =
                    dynamic_cast<mxfpp::GenericSoundEssenceDescriptor*>(desc)) {
                if (sound_desc->haveSoundEssenceCompression()) {
                    copy_ul(out->coding_ul, sound_desc->getSoundEssenceCompression());
                }
            }
            if (mxfpp::GenericDataEssenceDescriptor *data_desc =
                    dynamic_cast<mxfpp::GenericDataEssenceDescriptor*>(desc)) {
                if (data_desc->haveDataEssenceCoding()) {
                    copy_ul(out->coding_ul, data_desc->getDataEssenceCoding());
                }
            }
            if (mxfpp::RGBAEssenceDescriptor *rgba = dynamic_cast<mxfpp::RGBAEssenceDescriptor*>(desc)) {
                out->descriptor_kind = MXFUSE_DESCRIPTOR_RGBA;
                if (rgba->havePixelLayout()) {
                    mxfRGBALayout layout = rgba->getPixelLayout();
                    uint8_t count = 0;
                    for (uint8_t i = 0; i < 8; i++) {
                        if (layout.components[i].code == 0) {
                            break;
                        }
                        out->pixel_layout[count * 2] = layout.components[i].code;
                        out->pixel_layout[count * 2 + 1] = layout.components[i].depth;
                        count++;
                    }
                    out->pixel_layout_count = count;
                }
            }
            if (dynamic_cast<mxfpp::CDCIEssenceDescriptor*>(desc)) {
                out->descriptor_kind = MXFUSE_DESCRIPTOR_CDCI;
            }
        }
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

int mxfuse_reader_num_xml(MxfuseReader *reader, size_t *out, MxfuseError *err)
{
    if (!reader || !reader->reader || !out) {
        set_error(err, "invalid arguments to mxfuse_reader_num_xml");
        return MXFUSE_ERR;
    }
    try {
        *out = reader->reader->GetNumTextObjects();
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error counting XML objects");
        return MXFUSE_ERR;
    }
}

int mxfuse_reader_xml(
    MxfuseReader *reader,
    size_t index,
    MxfuseXmlView *out,
    MxfuseError *err
)
{
    if (!reader || !reader->reader || !out) {
        set_error(err, "invalid arguments to mxfuse_reader_xml");
        return MXFUSE_ERR;
    }
    std::memset(out, 0, sizeof(*out));
    try {
        if (index >= reader->reader->GetNumTextObjects()) {
            set_error(err, "xml index out of range");
            return MXFUSE_ERR;
        }
        bmx::MXFTextObject *obj = reader->reader->GetTextObject(index);
        if (!obj) {
            set_error(err, "xml object is null");
            return MXFUSE_ERR;
        }
        unsigned char *raw = 0;
        size_t raw_size = 0;
        obj->Read(&raw, &raw_size);
        if (raw && raw_size > 0) {
            if (raw_size > std::numeric_limits<uint32_t>::max()) {
                delete[] raw;
                set_error(err, "xml payload exceeds u32");
                return MXFUSE_ERR;
            }
            uint8_t *copy = static_cast<uint8_t *>(std::malloc(raw_size));
            if (!copy) {
                delete[] raw;
                set_error(err, "out of memory copying xml");
                return MXFUSE_ERR;
            }
            std::memcpy(copy, raw, raw_size);
            out->data = copy;
            out->size = static_cast<uint32_t>(raw_size);
        }
        delete[] raw;
        copy_ul(out->scheme_id, obj->GetSchemeId());
        std::snprintf(out->language, MXFUSE_XML_LANG_LEN, "%s", obj->GetLanguageCode().c_str());
        std::snprintf(out->mime_type, MXFUSE_XML_MIME_LEN, "%s", obj->GetMimeType().c_str());
        std::snprintf(out->ns, MXFUSE_XML_NS_LEN, "%s", obj->GetTextDataDescription().c_str());
        out->is_xml = obj->IsXML() ? 1 : 0;
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error reading xml");
        return MXFUSE_ERR;
    }
}

void mxfuse_xml_free(MxfuseXmlView *view)
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

static mxfUUID uuid_from_bytes(const uint8_t src[MXFUSE_UL_LEN])
{
    mxfUUID uuid;
    std::memcpy(&uuid, src, MXFUSE_UL_LEN);
    return uuid;
}

static mxfUMID umid_from_bytes(const uint8_t src[MXFUSE_UMID_LEN])
{
    mxfUMID umid;
    std::memcpy(&umid, src, MXFUSE_UMID_LEN);
    return umid;
}

int mxfuse_writer_configure(
    MxfuseWriter *writer,
    const MxfuseClipOptions *options,
    MxfuseError *err
)
{
    if (!writer || !writer->clip || !options) {
        set_error(err, "invalid arguments to mxfuse_writer_configure");
        return MXFUSE_ERR;
    }
    try {
        bmx::OP1AFile *op1a = writer->clip->GetOP1AClip();
        if (!op1a) {
            set_error(err, "clip options require an OP1a writer");
            return MXFUSE_ERR;
        }
        if (options->flags & MXFUSE_CLIP_HAS_START_TIMECODE) {
            bmx::Rational rate = writer->clip->GetFrameRate();
            bmx::Timecode tc(
                rate,
                options->start_timecode.drop_frame != 0,
                options->start_timecode.hour,
                options->start_timecode.minute,
                options->start_timecode.second,
                options->start_timecode.frame
            );
            writer->clip->SetStartTimecode(tc);
        }
        if (options->flags & MXFUSE_CLIP_HAS_TIMECODE_TRACK) {
            op1a->SetAddTimecodeTrack(options->timecode_track != 0);
        }
        if (options->flags & MXFUSE_CLIP_HAS_SYSTEM_ITEM) {
            op1a->SetAddSystemItem(options->system_item != 0);
        }
        bool have_product = (options->flags & (
            MXFUSE_CLIP_HAS_COMPANY |
            MXFUSE_CLIP_HAS_PRODUCT |
            MXFUSE_CLIP_HAS_VERSION_STRING |
            MXFUSE_CLIP_HAS_PRODUCT_VERSION |
            MXFUSE_CLIP_HAS_PRODUCT_UID
        )) != 0;
        if (have_product) {
            mxfProductVersion version = {0, 0, 0, 0, 0};
            if (options->flags & MXFUSE_CLIP_HAS_PRODUCT_VERSION) {
                version.major = options->product_version[0];
                version.minor = options->product_version[1];
                version.patch = options->product_version[2];
                version.build = options->product_version[3];
                version.release = options->product_version[4];
            }
            mxfUUID product_uid = g_Null_UUID;
            if (options->flags & MXFUSE_CLIP_HAS_PRODUCT_UID) {
                product_uid = uuid_from_bytes(options->product_uid);
            }
            writer->clip->SetProductInfo(
                options->company_name,
                options->product_name,
                version,
                options->version_string,
                product_uid
            );
        }
        if (options->flags & MXFUSE_CLIP_HAS_CREATION_DATE) {
            mxfTimestamp ts;
            ts.year = options->creation_year;
            ts.month = options->creation_month;
            ts.day = options->creation_day;
            ts.hour = options->creation_hour;
            ts.min = options->creation_min;
            ts.sec = options->creation_sec;
            ts.qmsec = options->creation_qmsec;
            writer->clip->SetCreationDate(ts);
        }
        if (options->flags & MXFUSE_CLIP_HAS_GENERATION_UID) {
            op1a->SetGenerationUID(uuid_from_bytes(options->generation_uid));
        }
        if (options->flags & MXFUSE_CLIP_HAS_MATERIAL_UID) {
            op1a->SetMaterialPackageUID(umid_from_bytes(options->material_package_uid));
        }
        if (options->flags & MXFUSE_CLIP_HAS_FILE_SOURCE_UID) {
            op1a->SetFileSourcePackageUID(umid_from_bytes(options->file_source_package_uid));
        }
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error configuring writer");
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
            if (spec->flags & MXFUSE_TRACK_HAS_DESCRIPTOR) {
                opaque->SetDescriptorKind(spec->descriptor_kind);
            }
            if (spec->flags & MXFUSE_TRACK_HAS_COMPONENT_DEPTH) {
                opaque->SetComponentDepth(spec->component_depth);
            }
            if (spec->flags & MXFUSE_TRACK_HAS_SUBSAMPLING) {
                opaque->SetHorizontalSubsampling(spec->horiz_subsampling);
                opaque->SetVerticalSubsampling(spec->vert_subsampling);
            }
            if (spec->flags & MXFUSE_TRACK_HAS_FRAME_LAYOUT) {
                opaque->SetFrameLayout(spec->frame_layout);
            }
            if (spec->flags & MXFUSE_TRACK_HAS_ASPECT_RATIO) {
                mxfRational ratio;
                ratio.numerator = spec->aspect_ratio.num;
                ratio.denominator = spec->aspect_ratio.den;
                opaque->SetAspectRatio(ratio);
            }
            if (spec->flags & MXFUSE_TRACK_HAS_VIDEO_LINE_MAP) {
                opaque->SetVideoLineMap(spec->video_line_map[0], spec->video_line_map[1]);
            }
            if (spec->flags & MXFUSE_TRACK_HAS_PIXEL_LAYOUT) {
                opaque->SetPixelLayout(spec->pixel_layout, spec->pixel_layout_count);
            }
            if (spec->flags & MXFUSE_TRACK_HAS_COLOR_PRIMARIES) {
                opaque->SetColorPrimaries(ul_from_bytes(spec->color_primaries));
            }
            if (spec->flags & MXFUSE_TRACK_HAS_TRANSFER) {
                opaque->SetTransferCharacteristic(ul_from_bytes(spec->transfer_characteristic));
            }
            if (spec->flags & MXFUSE_TRACK_HAS_CODING_EQ) {
                opaque->SetCodingEquations(ul_from_bytes(spec->coding_equations));
            }
        }
        if (bmx::OP1ATrack *op1a_track = track->GetOP1ATrack()) {
            if (bmx::OP1AOpaqueTrack *opaque_track = dynamic_cast<bmx::OP1AOpaqueTrack*>(op1a_track)) {
                if (spec->flags & MXFUSE_TRACK_HAS_ELEMENT_TYPE) {
                    opaque_track->SetElementType(spec->element_type);
                }
                if (spec->flags & MXFUSE_TRACK_HAS_ELEMENT_LLEN) {
                    opaque_track->SetElementLLen(spec->element_llen);
                }
                if (spec->flags & MXFUSE_TRACK_HAS_TEMPORAL_REORDER) {
                    opaque_track->SetTemporalReordering(spec->temporal_reordering != 0);
                }
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

int mxfuse_writer_add_xml(
    MxfuseWriter *writer,
    const uint8_t *data,
    uint32_t size,
    const uint8_t *scheme_id,
    const char *language,
    const char *ns,
    MxfuseError *err
)
{
    if (!writer || !writer->clip || !data || size == 0) {
        set_error(err, "invalid arguments to mxfuse_writer_add_xml");
        return MXFUSE_ERR;
    }
    try {
        bmx::OP1AFile *op1a = writer->clip->GetOP1AClip();
        if (!op1a) {
            set_error(err, "XML metadata requires an OP1a writer");
            return MXFUSE_ERR;
        }
        bmx::OP1AXMLTrack *xml = op1a->CreateXMLTrack();
        xml->SetSource(data, size, true);
        xml->SetTextEncoding(bmx::UTF8);
        if (scheme_id) {
            xml->SetSchemeId(ul_from_bytes(scheme_id));
        }
        if (language && language[0]) {
            xml->SetLanguageCode(language);
        }
        if (ns && ns[0]) {
            xml->SetNamespace(ns);
        }
        return MXFUSE_OK;
    } catch (const bmx::BMXException &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (const mxfpp::MXFException &e) {
        set_error(err, e.getMessage().c_str());
        return MXFUSE_ERR;
    } catch (const std::exception &e) {
        set_error(err, e.what());
        return MXFUSE_ERR;
    } catch (...) {
        set_error(err, "unknown error adding xml");
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
