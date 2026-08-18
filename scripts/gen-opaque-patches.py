#!/usr/bin/env python3
"""Generate the opaque-essence unified diffs under patches/."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / "vendor" / "bmx"
PATCHES = ROOT / "patches"


def run_diff(old: Path, new: Path, rel: str) -> str:
    result = subprocess.run(
        ["diff", "-u", "-N", "--label", f"a/{rel}", "--label", f"b/{rel}", str(old), str(new)],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode not in (0, 1):
        raise RuntimeError(result.stderr)
    return result.stdout


def write_file(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


OPAQUE_HELPER_H = r'''/*
 * Copyright (C) 2026, mxfuse contributors
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Opaque essence helper: caller-supplied container and coding ULs.
 */

#ifndef BMX_OPAQUE_MXF_DESCRIPTOR_HELPER_H_
#define BMX_OPAQUE_MXF_DESCRIPTOR_HELPER_H_


#include <bmx/mxf_helper/MXFDescriptorHelper.h>



namespace bmx
{


enum OpaqueDescriptorKind
{
    OPAQUE_DESC_DEFAULT = 0,
    OPAQUE_DESC_CDCI    = 1,
    OPAQUE_DESC_RGBA    = 2,
    OPAQUE_DESC_WAVE    = 3,
    OPAQUE_DESC_DATA    = 4,
};


class OpaqueMXFDescriptorHelper : public MXFDescriptorHelper
{
public:
    static bool IsSupported(EssenceType essence_type);
    static MXFDescriptorHelper* Create(EssenceType essence_type);

public:
    OpaqueMXFDescriptorHelper();
    virtual ~OpaqueMXFDescriptorHelper();

    virtual bool IsPicture() const;
    virtual bool IsSound() const;
    virtual bool IsData() const;

public:
    void SetEssenceContainerUL(mxfUL essence_container_ul);
    void SetPictureCodingUL(mxfUL picture_coding_ul);
    void SetStoredWidth(uint32_t width);
    void SetStoredHeight(uint32_t height);
    void SetSamplingRate(mxfRational sampling_rate);
    void SetChannelCount(uint32_t count);
    void SetQuantizationBits(uint32_t bits);
    void SetDescriptorKind(int kind);
    void SetComponentDepth(uint32_t depth);
    void SetHorizontalSubsampling(uint32_t value);
    void SetVerticalSubsampling(uint32_t value);
    void SetFrameLayout(uint8_t layout);
    void SetAspectRatio(mxfRational aspect_ratio);
    void SetVideoLineMap(int32_t first, int32_t second);
    void SetPixelLayout(const uint8_t *components, uint8_t count);
    void SetColorPrimaries(mxfUL label);
    void SetTransferCharacteristic(mxfUL label);
    void SetCodingEquations(mxfUL label);

    virtual mxfpp::FileDescriptor* CreateFileDescriptor(mxfpp::HeaderMetadata *header_metadata);
    virtual void UpdateFileDescriptor();
    virtual uint32_t GetSampleSize();

protected:
    virtual mxfUL ChooseEssenceContainerUL() const;

private:
    int ResolvedDescriptorKind() const;

    mxfUL mEssenceContainerUL;
    mxfUL mCodingUL;
    uint32_t mStoredWidth;
    uint32_t mStoredHeight;
    mxfRational mSamplingRate;
    uint32_t mChannelCount;
    uint32_t mQuantizationBits;
    int mDescriptorKind;
    uint32_t mComponentDepth;
    uint32_t mHorizSubsampling;
    uint32_t mVertSubsampling;
    uint8_t mFrameLayout;
    bool mHaveAspectRatio;
    mxfRational mAspectRatio;
    bool mHaveVideoLineMap;
    int32_t mVideoLineMap[2];
    bool mHavePixelLayout;
    mxfRGBALayout mPixelLayout;
    bool mHaveColorPrimaries;
    mxfUL mColorPrimaries;
    bool mHaveTransferCharacteristic;
    mxfUL mTransferCharacteristic;
    bool mHaveCodingEquations;
    mxfUL mCodingEquations;
};


};



#endif
'''

OPAQUE_HELPER_CPP = r'''/*
 * Copyright (C) 2026, mxfuse contributors
 * SPDX-License-Identifier: BSD-3-Clause
 */

#ifdef HAVE_CONFIG_H
#include "config.h"
#endif

#include <cstring>

#include <bmx/mxf_helper/OpaqueMXFDescriptorHelper.h>
#include <bmx/BMXTypes.h>
#include <bmx/BMXException.h>
#include <bmx/Logging.h>

#include <libMXF++/MXF.h>

using namespace std;
using namespace bmx;
using namespace mxfpp;



bool OpaqueMXFDescriptorHelper::IsSupported(EssenceType essence_type)
{
    return essence_type == OPAQUE_PICTURE ||
           essence_type == OPAQUE_SOUND ||
           essence_type == OPAQUE_DATA;
}

MXFDescriptorHelper* OpaqueMXFDescriptorHelper::Create(EssenceType essence_type)
{
    BMX_ASSERT(IsSupported(essence_type));
    OpaqueMXFDescriptorHelper *helper = new OpaqueMXFDescriptorHelper();
    helper->SetEssenceType(essence_type);
    return helper;
}

OpaqueMXFDescriptorHelper::OpaqueMXFDescriptorHelper()
: MXFDescriptorHelper()
{
    mEssenceType = OPAQUE_PICTURE;
    mEssenceContainerUL = g_Null_UL;
    mCodingUL = g_Null_UL;
    mStoredWidth = 0;
    mStoredHeight = 0;
    mSamplingRate = SAMPLING_RATE_48K;
    mChannelCount = 1;
    mQuantizationBits = 16;
    mDescriptorKind = OPAQUE_DESC_DEFAULT;
    mComponentDepth = 8;
    mHorizSubsampling = 2;
    mVertSubsampling = 1;
    mFrameLayout = MXF_FULL_FRAME;
    mHaveAspectRatio = false;
    mAspectRatio = ASPECT_RATIO_16_9;
    mHaveVideoLineMap = false;
    mVideoLineMap[0] = 1;
    mVideoLineMap[1] = 0;
    mHavePixelLayout = false;
    memset(&mPixelLayout, 0, sizeof(mPixelLayout));
    mHaveColorPrimaries = false;
    mColorPrimaries = g_Null_UL;
    mHaveTransferCharacteristic = false;
    mTransferCharacteristic = g_Null_UL;
    mHaveCodingEquations = false;
    mCodingEquations = g_Null_UL;
}

OpaqueMXFDescriptorHelper::~OpaqueMXFDescriptorHelper()
{
}

bool OpaqueMXFDescriptorHelper::IsPicture() const
{
    return mEssenceType == OPAQUE_PICTURE;
}

bool OpaqueMXFDescriptorHelper::IsSound() const
{
    return mEssenceType == OPAQUE_SOUND;
}

bool OpaqueMXFDescriptorHelper::IsData() const
{
    return mEssenceType == OPAQUE_DATA;
}

void OpaqueMXFDescriptorHelper::SetEssenceContainerUL(mxfUL essence_container_ul)
{
    mEssenceContainerUL = essence_container_ul;
}

void OpaqueMXFDescriptorHelper::SetPictureCodingUL(mxfUL picture_coding_ul)
{
    mCodingUL = picture_coding_ul;
}

void OpaqueMXFDescriptorHelper::SetStoredWidth(uint32_t width)
{
    mStoredWidth = width;
}

void OpaqueMXFDescriptorHelper::SetStoredHeight(uint32_t height)
{
    mStoredHeight = height;
}

void OpaqueMXFDescriptorHelper::SetSamplingRate(mxfRational sampling_rate)
{
    mSamplingRate = sampling_rate;
}

void OpaqueMXFDescriptorHelper::SetChannelCount(uint32_t count)
{
    mChannelCount = count;
}

void OpaqueMXFDescriptorHelper::SetQuantizationBits(uint32_t bits)
{
    mQuantizationBits = bits;
}

void OpaqueMXFDescriptorHelper::SetDescriptorKind(int kind)
{
    mDescriptorKind = kind;
}

void OpaqueMXFDescriptorHelper::SetComponentDepth(uint32_t depth)
{
    mComponentDepth = depth;
}

void OpaqueMXFDescriptorHelper::SetHorizontalSubsampling(uint32_t value)
{
    mHorizSubsampling = value;
}

void OpaqueMXFDescriptorHelper::SetVerticalSubsampling(uint32_t value)
{
    mVertSubsampling = value;
}

void OpaqueMXFDescriptorHelper::SetFrameLayout(uint8_t layout)
{
    mFrameLayout = layout;
}

void OpaqueMXFDescriptorHelper::SetAspectRatio(mxfRational aspect_ratio)
{
    mHaveAspectRatio = true;
    mAspectRatio = aspect_ratio;
}

void OpaqueMXFDescriptorHelper::SetVideoLineMap(int32_t first, int32_t second)
{
    mHaveVideoLineMap = true;
    mVideoLineMap[0] = first;
    mVideoLineMap[1] = second;
}

void OpaqueMXFDescriptorHelper::SetPixelLayout(const uint8_t *components, uint8_t count)
{
    mHavePixelLayout = true;
    memset(&mPixelLayout, 0, sizeof(mPixelLayout));
    if (!components)
        return;
    uint8_t n = count > 8 ? 8 : count;
    for (uint8_t i = 0; i < n; i++) {
        mPixelLayout.components[i].code = components[i * 2];
        mPixelLayout.components[i].depth = components[i * 2 + 1];
    }
}

void OpaqueMXFDescriptorHelper::SetColorPrimaries(mxfUL label)
{
    mHaveColorPrimaries = true;
    mColorPrimaries = label;
}

void OpaqueMXFDescriptorHelper::SetTransferCharacteristic(mxfUL label)
{
    mHaveTransferCharacteristic = true;
    mTransferCharacteristic = label;
}

void OpaqueMXFDescriptorHelper::SetCodingEquations(mxfUL label)
{
    mHaveCodingEquations = true;
    mCodingEquations = label;
}

int OpaqueMXFDescriptorHelper::ResolvedDescriptorKind() const
{
    if (mDescriptorKind != OPAQUE_DESC_DEFAULT)
        return mDescriptorKind;
    if (mEssenceType == OPAQUE_SOUND)
        return OPAQUE_DESC_WAVE;
    if (mEssenceType == OPAQUE_DATA)
        return OPAQUE_DESC_DATA;
    return OPAQUE_DESC_CDCI;
}

FileDescriptor* OpaqueMXFDescriptorHelper::CreateFileDescriptor(HeaderMetadata *header_metadata)
{
    switch (ResolvedDescriptorKind()) {
        case OPAQUE_DESC_WAVE:
            mFileDescriptor = new WaveAudioDescriptor(header_metadata);
            break;
        case OPAQUE_DESC_DATA:
            mFileDescriptor = new GenericDataEssenceDescriptor(header_metadata);
            break;
        case OPAQUE_DESC_RGBA:
            mFileDescriptor = new RGBAEssenceDescriptor(header_metadata);
            break;
        default:
            mFileDescriptor = new CDCIEssenceDescriptor(header_metadata);
            break;
    }
    UpdateFileDescriptor();
    return mFileDescriptor;
}

void OpaqueMXFDescriptorHelper::UpdateFileDescriptor()
{
    MXFDescriptorHelper::UpdateFileDescriptor();

    if (ResolvedDescriptorKind() == OPAQUE_DESC_WAVE) {
        WaveAudioDescriptor *wav = dynamic_cast<WaveAudioDescriptor*>(mFileDescriptor);
        BMX_ASSERT(wav);
        uint32_t bytes_per_sample = mChannelCount * ((mQuantizationBits + 7) / 8);
        wav->setAudioSamplingRate(mSamplingRate);
        wav->setChannelCount(mChannelCount);
        wav->setQuantizationBits(mQuantizationBits);
        wav->setLocked(true);
        wav->setBlockAlign(bytes_per_sample);
        wav->setAvgBps(bytes_per_sample * mSamplingRate.numerator / mSamplingRate.denominator);
        if (!mxf_equals_ul(&mCodingUL, &g_Null_UL))
            wav->setSoundEssenceCompression(mCodingUL);
        return;
    }

    if (ResolvedDescriptorKind() == OPAQUE_DESC_DATA) {
        GenericDataEssenceDescriptor *data = dynamic_cast<GenericDataEssenceDescriptor*>(mFileDescriptor);
        BMX_ASSERT(data);
        if (!mxf_equals_ul(&mCodingUL, &g_Null_UL))
            data->setDataEssenceCoding(mCodingUL);
        return;
    }

    GenericPictureEssenceDescriptor *pict = dynamic_cast<GenericPictureEssenceDescriptor*>(mFileDescriptor);
    BMX_ASSERT(pict);
    pict->setStoredWidth(mStoredWidth);
    pict->setStoredHeight(mStoredHeight);
    pict->setSampledWidth(mStoredWidth);
    pict->setSampledHeight(mStoredHeight);
    pict->setSampledXOffset(0);
    pict->setSampledYOffset(0);
    pict->setDisplayWidth(mStoredWidth);
    pict->setDisplayHeight(mStoredHeight);
    pict->setDisplayXOffset(0);
    pict->setDisplayYOffset(0);
    pict->setFrameLayout(mFrameLayout);
    if (!mxf_equals_ul(&mCodingUL, &g_Null_UL))
        pict->setPictureEssenceCoding(mCodingUL);
    if (mHaveAspectRatio)
        pict->setAspectRatio(mAspectRatio);
    if (mHaveVideoLineMap)
        pict->setVideoLineMap(mVideoLineMap[0], mVideoLineMap[1]);
    if (mHaveColorPrimaries)
        pict->setColorPrimaries(mColorPrimaries);
    if (mHaveTransferCharacteristic)
        pict->setCaptureGamma(mTransferCharacteristic);
    if (mHaveCodingEquations)
        pict->setCodingEquations(mCodingEquations);

    CDCIEssenceDescriptor *cdci = dynamic_cast<CDCIEssenceDescriptor*>(mFileDescriptor);
    if (cdci) {
        cdci->setComponentDepth(mComponentDepth);
        cdci->setHorizontalSubsampling(mHorizSubsampling);
        cdci->setVerticalSubsampling(mVertSubsampling);
    }

    RGBAEssenceDescriptor *rgba = dynamic_cast<RGBAEssenceDescriptor*>(mFileDescriptor);
    if (rgba && mHavePixelLayout)
        rgba->setPixelLayout(mPixelLayout);
}

uint32_t OpaqueMXFDescriptorHelper::GetSampleSize()
{
    return 0;
}

mxfUL OpaqueMXFDescriptorHelper::ChooseEssenceContainerUL() const
{
    return mEssenceContainerUL;
}
'''

OPAQUE_TRACK_H = r'''/*
 * Copyright (C) 2026, mxfuse contributors
 * SPDX-License-Identifier: BSD-3-Clause
 */

#ifndef BMX_OP1A_OPAQUE_TRACK_H_
#define BMX_OP1A_OPAQUE_TRACK_H_


#include <bmx/mxf_op1a/OP1ATrack.h>
#include <bmx/mxf_helper/OpaqueMXFDescriptorHelper.h>



namespace bmx
{


class OP1AOpaqueTrack : public OP1ATrack
{
public:
    OP1AOpaqueTrack(OP1AFile *file, uint32_t track_index, uint32_t track_id, uint8_t track_type_number,
                    mxfRational frame_rate, EssenceType essence_type);
    virtual ~OP1AOpaqueTrack();

    void SetElementType(uint8_t element_type);
    void SetElementLLen(uint8_t llen);
    void SetTemporalReordering(bool enable);

protected:
    virtual void PrepareWrite(uint8_t track_count);
    virtual void WriteSamplesInt(const unsigned char *data, uint32_t size, uint32_t num_samples);

private:
    void ApplyElementKey();

    OpaqueMXFDescriptorHelper *mOpaqueDescriptorHelper;
    int64_t mPosition;
    uint8_t mElementType;
    uint8_t mElementLLen;
    bool mTemporalReordering;
};


};



#endif
'''

OPAQUE_TRACK_CPP = r'''/*
 * Copyright (C) 2026, mxfuse contributors
 * SPDX-License-Identifier: BSD-3-Clause
 */

#ifdef HAVE_CONFIG_H
#include "config.h"
#endif

#include <bmx/mxf_op1a/OP1AOpaqueTrack.h>
#include <bmx/mxf_op1a/OP1AFile.h>
#include <bmx/BMXException.h>
#include <bmx/Logging.h>

using namespace std;
using namespace bmx;
using namespace mxfpp;



OP1AOpaqueTrack::OP1AOpaqueTrack(OP1AFile *file, uint32_t track_index, uint32_t track_id, uint8_t track_type_number,
                                 mxfRational frame_rate, EssenceType essence_type)
: OP1ATrack(file, track_index, track_id, track_type_number, frame_rate, essence_type)
{
    mOpaqueDescriptorHelper = dynamic_cast<OpaqueMXFDescriptorHelper*>(mDescriptorHelper);
    BMX_ASSERT(mOpaqueDescriptorHelper);
    mPosition = 0;
    mElementType = 0x7F;
    mElementLLen = 4;
    mTemporalReordering = false;
    ApplyElementKey();
}

OP1AOpaqueTrack::~OP1AOpaqueTrack()
{
}

void OP1AOpaqueTrack::SetElementType(uint8_t element_type)
{
    mElementType = element_type;
    ApplyElementKey();
}

void OP1AOpaqueTrack::SetElementLLen(uint8_t llen)
{
    mElementLLen = llen ? llen : 4;
}

void OP1AOpaqueTrack::SetTemporalReordering(bool enable)
{
    mTemporalReordering = enable;
}

void OP1AOpaqueTrack::ApplyElementKey()
{
    if (mEssenceType == OPAQUE_SOUND) {
        mTrackNumber = MXF_TRACK_NUM(0x16, 0x01, mElementType, 0x00);
        mEssenceElementKey = MXF_GENERIC_CONTAINER_ELEMENT_KEY(0x01, 0x16, 0x01, mElementType, 0x00);
    } else if (mEssenceType == OPAQUE_DATA) {
        mTrackNumber = MXF_TRACK_NUM(0x17, 0x01, mElementType, 0x00);
        mEssenceElementKey = MXF_GENERIC_CONTAINER_ELEMENT_KEY(0x01, 0x17, 0x01, mElementType, 0x00);
    } else {
        mTrackNumber = MXF_TRACK_NUM(0x15, 0x01, mElementType, 0x00);
        mEssenceElementKey = MXF_GENERIC_CONTAINER_ELEMENT_KEY(0x01, 0x15, 0x01, mElementType, 0x00);
    }
}

void OP1AOpaqueTrack::PrepareWrite(uint8_t track_count)
{
    CompleteEssenceKeyAndTrackNum(track_count);

    if (mEssenceType == OPAQUE_SOUND) {
        mCPManager->RegisterSoundTrackElement(mTrackIndex, mEssenceElementKey, mElementLLen);
        mIndexTable->RegisterSoundTrackElement(mTrackIndex);
    } else if (mEssenceType == OPAQUE_DATA) {
        mCPManager->RegisterDataTrackElement(mTrackIndex, mEssenceElementKey, 0, 0);
        mIndexTable->RegisterDataTrackElement(mTrackIndex, false);
    } else {
        mCPManager->RegisterPictureTrackElement(mTrackIndex, mEssenceElementKey, false, mElementLLen);
        mIndexTable->RegisterPictureTrackElement(mTrackIndex, false, mTemporalReordering);
    }
}

void OP1AOpaqueTrack::WriteSamplesInt(const unsigned char *data, uint32_t size, uint32_t num_samples)
{
    BMX_CHECK(num_samples == 1);
    BMX_CHECK(data && size);

    mCPManager->WriteSamples(mTrackIndex, data, size, num_samples);
    mIndexTable->AddIndexEntry(mTrackIndex, mPosition, 0, 0, 0x80, true, false);
    mPosition++;
}
'''


def patch_essence_type_h(text: str) -> str:
    return text.replace(
        "    TIMED_TEXT,\n} EssenceType;",
        "    TIMED_TEXT,\n"
        "    // Opaque / private codecs (mxfuse)\n"
        "    OPAQUE_PICTURE,\n"
        "    OPAQUE_SOUND,\n"
        "    OPAQUE_DATA,\n"
        "} EssenceType;",
    )


def patch_essence_type_cpp(text: str) -> str:
    return text.replace(
        '    {TIMED_TEXT,                DATA_ESSENCE,           "Timed Text",                           "Timed_Text"},\n};',
        '    {TIMED_TEXT,                DATA_ESSENCE,           "Timed Text",                           "Timed_Text"},\n'
        '    {OPAQUE_PICTURE,            PICTURE_ESSENCE,        "opaque picture",                      "Opaque_Picture"},\n'
        '    {OPAQUE_SOUND,              SOUND_ESSENCE,          "opaque sound",                        "Opaque_Sound"},\n'
        '    {OPAQUE_DATA,               DATA_ESSENCE,           "opaque data",                         "Opaque_Data"},\n'
        "};",
    )


def patch_descriptor_helper_cpp(text: str) -> str:
    if "#include <bmx/mxf_helper/OpaqueMXFDescriptorHelper.h>" not in text:
        text = text.replace(
            "#include <bmx/mxf_helper/TimedTextMXFDescriptorHelper.h>",
            "#include <bmx/mxf_helper/TimedTextMXFDescriptorHelper.h>\n"
            "#include <bmx/mxf_helper/OpaqueMXFDescriptorHelper.h>",
        )
    old = """MXFDescriptorHelper* MXFDescriptorHelper::Create(EssenceType essence_type)
{
    if (PictureMXFDescriptorHelper::IsSupported(essence_type))
        return PictureMXFDescriptorHelper::Create(essence_type);
    else if (SoundMXFDescriptorHelper::IsSupported(essence_type))
        return SoundMXFDescriptorHelper::Create(essence_type);
    else if (DataMXFDescriptorHelper::IsSupported(essence_type))
        return DataMXFDescriptorHelper::Create(essence_type);
"""
    new = """MXFDescriptorHelper* MXFDescriptorHelper::Create(EssenceType essence_type)
{
    if (OpaqueMXFDescriptorHelper::IsSupported(essence_type))
        return OpaqueMXFDescriptorHelper::Create(essence_type);
    if (PictureMXFDescriptorHelper::IsSupported(essence_type))
        return PictureMXFDescriptorHelper::Create(essence_type);
    else if (SoundMXFDescriptorHelper::IsSupported(essence_type))
        return SoundMXFDescriptorHelper::Create(essence_type);
    else if (DataMXFDescriptorHelper::IsSupported(essence_type))
        return DataMXFDescriptorHelper::Create(essence_type);
"""
    if old not in text:
        raise SystemExit("MXFDescriptorHelper::Create block not found")
    return text.replace(old, new)


def patch_helper_src_cmake(text: str) -> str:
    return text.replace(
        "    mxf_helper/MXFDescriptorHelper.cpp\n",
        "    mxf_helper/MXFDescriptorHelper.cpp\n    mxf_helper/OpaqueMXFDescriptorHelper.cpp\n",
    )


def patch_helper_hdr_cmake(text: str) -> str:
    return text.replace(
        "    bmx/mxf_helper/MXFDescriptorHelper.h\n",
        "    bmx/mxf_helper/MXFDescriptorHelper.h\n    bmx/mxf_helper/OpaqueMXFDescriptorHelper.h\n",
    )


def patch_op1a_track_cpp(text: str) -> str:
    if "#include <bmx/mxf_op1a/OP1AOpaqueTrack.h>" not in text:
        text = text.replace(
            "#include <bmx/mxf_op1a/OP1ATimedTextTrack.h>",
            "#include <bmx/mxf_op1a/OP1ATimedTextTrack.h>\n"
            "#include <bmx/mxf_op1a/OP1AOpaqueTrack.h>",
        )
    text = text.replace(
        "    {TIMED_TEXT,               {{-1, -1}, {0, 0}}},\n};",
        "    {TIMED_TEXT,               {{-1, -1}, {0, 0}}},\n"
        "    {OPAQUE_PICTURE,           {{-1, -1}, {0, 0}}},\n"
        "    {OPAQUE_SOUND,             {{-1, -1}, {0, 0}}},\n"
        "    {OPAQUE_DATA,              {{-1, -1}, {0, 0}}},\n"
        "};",
    )
    text = text.replace(
        "        case TIMED_TEXT:\n"
        "            return new OP1ATimedTextTrack(file, track_index, track_id, track_type_number, frame_rate, essence_type);\n"
        "        default:",
        "        case TIMED_TEXT:\n"
        "            return new OP1ATimedTextTrack(file, track_index, track_id, track_type_number, frame_rate, essence_type);\n"
        "        case OPAQUE_PICTURE:\n"
        "        case OPAQUE_SOUND:\n"
        "        case OPAQUE_DATA:\n"
        "            return new OP1AOpaqueTrack(file, track_index, track_id, track_type_number, frame_rate, essence_type);\n"
        "        default:",
    )
    return text


def patch_op1a_src_cmake(text: str) -> str:
    return text.replace(
        "    mxf_op1a/OP1ATrack.cpp\n",
        "    mxf_op1a/OP1AOpaqueTrack.cpp\n    mxf_op1a/OP1ATrack.cpp\n",
    )


def patch_op1a_hdr_cmake(text: str) -> str:
    return text.replace(
        "    bmx/mxf_op1a/OP1ATrack.h\n",
        "    bmx/mxf_op1a/OP1AOpaqueTrack.h\n    bmx/mxf_op1a/OP1ATrack.h\n",
    )


def main() -> None:
    PATCHES.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        old = Path(tmp) / "old"
        new = Path(tmp) / "new"
        shutil.copytree(VENDOR, old, ignore=shutil.ignore_patterns(".git"))
        shutil.copytree(old, new)

        write_file(
            new / "include/bmx/EssenceType.h",
            patch_essence_type_h((old / "include/bmx/EssenceType.h").read_text()),
        )
        write_file(
            new / "src/common/EssenceType.cpp",
            patch_essence_type_cpp((old / "src/common/EssenceType.cpp").read_text()),
        )
        write_file(
            new / "src/mxf_helper/MXFDescriptorHelper.cpp",
            patch_descriptor_helper_cpp((old / "src/mxf_helper/MXFDescriptorHelper.cpp").read_text()),
        )
        write_file(
            new / "src/mxf_helper/CMakeLists.txt",
            patch_helper_src_cmake((old / "src/mxf_helper/CMakeLists.txt").read_text()),
        )
        write_file(
            new / "include/bmx/mxf_helper/CMakeLists.txt",
            patch_helper_hdr_cmake((old / "include/bmx/mxf_helper/CMakeLists.txt").read_text()),
        )
        write_file(new / "include/bmx/mxf_helper/OpaqueMXFDescriptorHelper.h", OPAQUE_HELPER_H)
        write_file(new / "src/mxf_helper/OpaqueMXFDescriptorHelper.cpp", OPAQUE_HELPER_CPP)
        write_file(
            new / "src/mxf_op1a/OP1ATrack.cpp",
            patch_op1a_track_cpp((old / "src/mxf_op1a/OP1ATrack.cpp").read_text()),
        )
        write_file(
            new / "src/mxf_op1a/CMakeLists.txt",
            patch_op1a_src_cmake((old / "src/mxf_op1a/CMakeLists.txt").read_text()),
        )
        write_file(
            new / "include/bmx/mxf_op1a/CMakeLists.txt",
            patch_op1a_hdr_cmake((old / "include/bmx/mxf_op1a/CMakeLists.txt").read_text()),
        )
        write_file(new / "include/bmx/mxf_op1a/OP1AOpaqueTrack.h", OPAQUE_TRACK_H)
        write_file(new / "src/mxf_op1a/OP1AOpaqueTrack.cpp", OPAQUE_TRACK_CPP)

        p1 = "".join(
            [
                run_diff(
                    old / "include/bmx/EssenceType.h",
                    new / "include/bmx/EssenceType.h",
                    "include/bmx/EssenceType.h",
                ),
                run_diff(
                    old / "src/common/EssenceType.cpp",
                    new / "src/common/EssenceType.cpp",
                    "src/common/EssenceType.cpp",
                ),
            ]
        )
        p2 = "".join(
            [
                run_diff(
                    old / "src/mxf_helper/MXFDescriptorHelper.cpp",
                    new / "src/mxf_helper/MXFDescriptorHelper.cpp",
                    "src/mxf_helper/MXFDescriptorHelper.cpp",
                ),
                run_diff(
                    old / "src/mxf_helper/CMakeLists.txt",
                    new / "src/mxf_helper/CMakeLists.txt",
                    "src/mxf_helper/CMakeLists.txt",
                ),
                run_diff(
                    old / "include/bmx/mxf_helper/CMakeLists.txt",
                    new / "include/bmx/mxf_helper/CMakeLists.txt",
                    "include/bmx/mxf_helper/CMakeLists.txt",
                ),
                run_diff(
                    Path("/dev/null"),
                    new / "include/bmx/mxf_helper/OpaqueMXFDescriptorHelper.h",
                    "include/bmx/mxf_helper/OpaqueMXFDescriptorHelper.h",
                ),
                run_diff(
                    Path("/dev/null"),
                    new / "src/mxf_helper/OpaqueMXFDescriptorHelper.cpp",
                    "src/mxf_helper/OpaqueMXFDescriptorHelper.cpp",
                ),
            ]
        )
        p3 = "".join(
            [
                run_diff(
                    old / "src/mxf_op1a/OP1ATrack.cpp",
                    new / "src/mxf_op1a/OP1ATrack.cpp",
                    "src/mxf_op1a/OP1ATrack.cpp",
                ),
                run_diff(
                    old / "src/mxf_op1a/CMakeLists.txt",
                    new / "src/mxf_op1a/CMakeLists.txt",
                    "src/mxf_op1a/CMakeLists.txt",
                ),
                run_diff(
                    old / "include/bmx/mxf_op1a/CMakeLists.txt",
                    new / "include/bmx/mxf_op1a/CMakeLists.txt",
                    "include/bmx/mxf_op1a/CMakeLists.txt",
                ),
                run_diff(
                    Path("/dev/null"),
                    new / "include/bmx/mxf_op1a/OP1AOpaqueTrack.h",
                    "include/bmx/mxf_op1a/OP1AOpaqueTrack.h",
                ),
                run_diff(
                    Path("/dev/null"),
                    new / "src/mxf_op1a/OP1AOpaqueTrack.cpp",
                    "src/mxf_op1a/OP1AOpaqueTrack.cpp",
                ),
            ]
        )

        (PATCHES / "0001-opaque-essence-type.patch").write_text(p1)
        (PATCHES / "0002-opaque-descriptor-helper.patch").write_text(p2)
        (PATCHES / "0003-op1a-opaque-track.patch").write_text(p3)
        print("wrote", PATCHES)


if __name__ == "__main__":
    main()
